// UI smoke test: drives the real interface (served by kjv-devserver) in headless Chrome
// over the DevTools protocol. No npm dependencies; needs Node 22+ (global WebSocket).
//
//   cargo run --release -p kjv-devserver &      # serves http://localhost:1420
//   python3 tests/ui/mock_llm.py &              # a stand-in model at :8765
//   node tests/ui/smoke.mjs [path-to-chrome]

import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const BASE = "http://localhost:1420";
const PORT = 9333;
const chromePath = process.argv[2] || process.env.CHROME || "google-chrome";

// A fresh profile for this run, removed when Chrome exits (see the end)
const profile = mkdtempSync(join(tmpdir(), "kjv-ui-"));
const chrome = spawn(chromePath, [
  "--headless=new", "--disable-gpu", "--no-first-run", "--no-default-browser-check",
  `--remote-debugging-port=${PORT}`, `--user-data-dir=${profile}`, "about:blank",
], { stdio: "ignore" });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function connect() {
  for (let i = 0; i < 100; i++) {
    try {
      const pages = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
      const page = pages.find((p) => p.type === "page");
      if (page) return page.webSocketDebuggerUrl;
    } catch {}
    await sleep(200);
  }
  throw new Error("Chrome DevTools did not start");
}

const ws = new WebSocket(await connect());
await new Promise((r) => ws.addEventListener("open", r, { once: true }));
let nextId = 0;
const pending = new Map();
const consoleErrors = [];
ws.addEventListener("message", (event) => {
  const msg = JSON.parse(event.data);
  if (msg.id && pending.has(msg.id)) {
    const { resolve, reject } = pending.get(msg.id);
    pending.delete(msg.id);
    msg.error ? reject(new Error(msg.error.message)) : resolve(msg.result);
  } else if (msg.method === "Runtime.exceptionThrown") {
    consoleErrors.push(msg.params.exceptionDetails.exception?.description ?? msg.params.exceptionDetails.text);
  } else if (msg.method === "Runtime.consoleAPICalled" && msg.params.type === "error") {
    consoleErrors.push(msg.params.args.map((a) => a.value ?? a.description).join(" "));
  }
});
const send = (method, params = {}) =>
  new Promise((resolve, reject) => {
    const id = ++nextId;
    pending.set(id, { resolve, reject });
    ws.send(JSON.stringify({ id, method, params }));
  });

await send("Runtime.enable");

/** Evaluate `fn` (an async function source) in the page and return its value. */
async function run(fn) {
  const r = await send("Runtime.evaluate", { expression: `(${fn})()`, awaitPromise: true, returnByValue: true });
  if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? r.exceptionDetails.text);
  return r.result.value;
}

async function open(query, { width = 1280, height = 800, mobile = false } = {}) {
  await send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile });
  await send("Page.navigate", { url: `${BASE}/?${query}` });
  for (let i = 0; i < 150; i++) {
    await sleep(100);
    const ready = await run(`async () => document.readyState === "complete" && document.getElementById("reader")?.getAttribute("aria-busy") === "false"`).catch(() => false);
    if (ready) return;
  }
  throw new Error(`page never finished loading: ${query}`);
}

// Helpers injected into each test function
const HELPERS = `
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const until = async (fn, what, ms = 5000) => {
    for (const end = Date.now() + ms; Date.now() < end; ) { const v = fn(); if (v) return v; await wait(25); }
    throw new Error("timed out waiting for " + what);
  };
  const $ = (s) => document.querySelector(s);
  const $$ = (s) => [...document.querySelectorAll(s)];
  const assert = (cond, msg) => { if (!cond) throw new Error(msg); };
  // Rendered boxes, not offsetParent (always null for position: fixed, like the tab bar)
  const visible = (el) => !!el && el.getClientRects().length > 0;
  const viewButton = (label) => $$('[aria-label="View"] button').find((b) => visible(b) && b.textContent === label);
`;

const results = [];
async function test(name, query, viewport, body) {
  try {
    await open(query, viewport);
    await run(`async () => { ${HELPERS} ${body} }`);
    results.push([name, "ok"]);
  } catch (error) {
    results.push([name, `FAIL: ${error.message}`]);
  }
}

// ------------------------------------------------------------------ tests

await test("Genesis 1 loads with 31 verses", "book=Genesis&chapter=1&view=kjv", {}, `
  assert($("#ref-label").textContent === "Genesis 1", "heading");
  assert($$(".verse").length === 31, "31 verses, got " + $$(".verse").length);
  assert($("#v1").textContent.includes("In the beginning God created the heaven and the earth."), "verse 1 text");
`);

await test("Picker opens Romans 8", "book=Genesis&chapter=1", {}, `
  $("#ref-button").click();
  await until(() => $("#picker").open, "picker");
  $$(".book-grid button").find((b) => b.textContent === "Romans").click();
  await until(() => $(".chapter-grid"), "chapter grid");
  $$(".chapter-grid button").find((b) => b.textContent === "8").click();
  await until(() => $("#ref-label").textContent === "Romans 8", "Romans 8");
  assert(!$("#picker").open, "picker closed");
`);

await test("Bookmark a verse and find it under Saved", "book=Romans&chapter=8", {}, `
  $("#v28").click();
  await until(() => !$("#verse-actions").hidden, "verse actions");
  assert($("#verse-actions-ref .ref-long").textContent === "Romans 8:28", "action bar reference");
  $('[data-action="bookmark"]').click();
  await until(() => $('[data-action="bookmark"]').getAttribute("aria-pressed") === "true", "bookmarked");
  $('[data-open-panel="saved"]').click();
  await until(() => $$("#panel-body .row-main").some((e) => e.textContent === "Romans 8:28"), "saved list");
`);

await test("Search result opens the verse with the match highlighted", "book=Romans&chapter=8", {}, `
  $('[data-open-panel="search"]').click();
  const input = await until(() => $("#search-input"), "search input");
  input.value = "Jesus wept";
  input.dispatchEvent(new Event("input"));
  await until(() => $("#panel-body .result"), "results");
  assert($("#panel-body .result-summary").textContent === "1 verse", "one result");
  $("#panel-body .result").click();
  await until(() => $("#ref-label").textContent === "John 11" && $("#v35 mark"), "John 11 with highlight");
  assert($("#v35").getAttribute("aria-current") === "true", "verse selected");
  assert(!$("#v35 .red"), "narration is not red");
  assert($("#v43 .red")?.textContent === "Lazarus, come forth.", "only the spoken words are red");
`);

await test("Search everything: grouped by source, each opening where it is", "book=John&chapter=11&tr=kjv", {}, `
  $('[data-open-panel="search"]').click();
  const input = await until(() => $("#search-input"), "search input");
  assert(input.placeholder === "Search the KJV", "the translation being read by default: " + input.placeholder);
  $$(".search-choices [role=radio]").find((b) => b.textContent === "Everything").click();
  const box = await until(() => $("#search-input")?.placeholder === "Search everything" && $("#search-input"), "everything");
  box.value = "Melchizedek";
  box.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
  await until(() => $(".result-summary") && !$(".result-summary").textContent.includes("searching"), "every source searched", 60000);
  assert($(".result-summary").textContent.startsWith("422 results in 40 sources"), "summary: " + $(".result-summary").textContent);
  // The translation being read first, then the rest, then the commentaries
  const groups = $$(".search-group").map((g) => g.dataset.group);
  assert(groups[0] === "bible:kjv" && groups.indexOf("commentary:mhc") > groups.indexOf("bible:web"), "order: " + groups.join());
  assert($(".search-none").textContent.includes("DRA"), "sources without it listed: the Douay-Rheims spells it Melchisedec");
  // Tyndale's Bible and the Tyndale notes are two sources
  assert(!groups.includes("bible:tyndale") && groups.includes("commentary:tyndale"), "the notes, not the Bible: " + groups.join());
  const gill = $('[data-group="commentary:gill"]');
  assert(gill.querySelector(".search-group-name").textContent.endsWith("59 notes"), "Gill: " + gill.querySelector(".search-group-name").textContent);
  assert(gill.querySelectorAll(".result").length === 20 && gill.querySelector(".search-more").textContent === "Show all 59", "the first 20, then the rest");
  gill.querySelector(".search-more").click();
  await until(() => $('[data-group="commentary:gill"]').querySelectorAll(".result").length === 59, "all of Gill's");
  // A note opens in the Commentary panel, shown and opened even if not chosen
  const mhc = $('[data-group="commentary:mhc"] .result');
  assert(mhc.querySelector(".result-ref").textContent === "Genesis 14:17-20", "Henry's first: " + mhc.querySelector(".result-ref").textContent);
  assert(mhc.querySelector("mark.hit").textContent === "Melchizedek", "the match marked in the snippet");
  mhc.click();
  await until(() => $("#panel-title").textContent === "Commentary" && $(".note.is-focus"), "the note, in the Commentary panel");
  assert($(".note.is-focus").open, "opened");
  assert($(".note.is-focus").closest("[data-commentary]").dataset.commentary === "mhc", "Henry's");
  // A verse in another translation opens in that translation, with the match marked
  $('[data-open-panel="search"]').click();
  const web = await until(() => $('[data-group="bible:web"] .result'), "results kept");
  web.click();
  await until(() => $("#translation-label").textContent === "WEB" && $(".chapter.library mark.hit"), "the WEB, with the match marked");
  assert($$(".chapter.library mark.hit").map((m) => m.textContent).join() === "Melchizedek", "marked: " + $$(".chapter.library mark.hit").map((m) => m.textContent));
  assert($$(".search-choices [role=radio]")[0].textContent === "WEB", "Search names the translation now being read");
  // Back to searching the translation being read, and to the KJV (settings persist between tests)
  $$(".search-choices [role=radio]")[0].click();
  await until(() => $("#search-input")?.placeholder === "Search the WEB", "the translation being read again");
  $("#translation-button").click();
  await until(() => $("#translations").open, "translation picker");
  $$(".translation-item").find((b) => b.querySelector(".translation-abbr").textContent === "KJV").click();
  await until(() => $("#translation-label").textContent === "KJV" && $("#reader").getAttribute("aria-busy") === "false", "back to the KJV");
`);
await test("Typography-insensitive search", "book=Genesis&chapter=1", {}, `
  $('[data-open-panel="search"]').click();
  const input = await until(() => $("#search-input"), "search input");
  input.value = "Caesar's";
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
  await until(() => $("#panel-body .result-summary"), "summary");
  assert($("#panel-body .result-summary").textContent === "8 verses", "8 verses for Caesar's");
`);

await test("Interlinear Hebrew is right-to-left with lexicon lookup", "book=Genesis&chapter=1&view=interlinear", {}, `
  const words = await until(() => $("#v1 .words"), "word cards");
  assert(words.getAttribute("dir") === "rtl", "Hebrew cards flow right to left");
  const cards = words.querySelectorAll(".word");
  assert(cards.length === 7, "7 words in Genesis 1:1");
  const first = cards[0].querySelector(".word-orig");
  assert(first.getAttribute("lang") === "he" && first.getAttribute("dir") === "rtl", "lang/dir on Hebrew");
  assert(cards[0].getBoundingClientRect().left > cards[6].getBoundingClientRect().left, "first word is rightmost");
  cards[2].click();
  await until(() => $(".lexicon-gloss"), "lexicon");
  assert($(".lexicon-gloss").textContent === "God", "H430 gloss");
  assert($("#strongs-input").value === "H430", "Strong's number shown");
`);

await test("Greek interlinear flows left-to-right", "book=John&chapter=1&view=interlinear", {}, `
  const words = await until(() => $("#v1 .words"), "word cards");
  assert(words.getAttribute("dir") === "ltr", "Greek cards flow left to right");
  assert(words.querySelector(".word-orig").getAttribute("lang") === "grc", "lang=grc");
`);

await test("Psalm title shows above verse 1", "book=Psalms&chapter=51&view=parallel", {}, `
  assert($("#v0.is-title")?.textContent.startsWith("To the chief Musician"), "title text");
  assert($("#v0 .orig-text[lang=he]"), "Hebrew title");
`);

await test("Parallel: translations side by side, verse by verse in each one's numbering", "book=Psalms&chapter=23&tr=kjv", {}, `
  const switchButtons = () => [...$$("[data-view-switch]").find((g) => g.offsetParent !== null).querySelectorAll("button")].filter((b) => b.offsetParent !== null);
  switchButtons().find((b) => b.textContent === "Parallel").click();
  await until(() => $(".parallel-reading"), "parallel");
  const names = () => $$(".pr-column-name").map((c) => c.textContent).join(",");
  assert(names() === "KJV,Hebrew", "the KJV beside its Hebrew: " + names());
  const add = async (abbr) => {
    $$(".pr-bar .chip").find((b) => b.textContent === "Translation").click();
    await until(() => $("#translations").open && $("#translations-title").textContent === "Read beside", "picker");
    $$("#translations .translation-item").find((b) => b.querySelector(".translation-abbr").textContent === abbr).click();
    await until(() => names().endsWith(abbr), abbr);
  };
  await add("WEB");
  await add("DRA");
  assert(names() === "KJV,Hebrew,WEB,DRA", names());
  assert($$(".pr-bar .chip").find((b) => b.textContent === "Translation").disabled, "four columns at most");
  // The Douay-Rheims' Psalm 22:1 holds the KJV's title and verse 1: given once
  const cells = (id) => [...$(id).querySelectorAll(".pr-cell")];
  assert(cells("#v0")[3].textContent.startsWith("22:1 A psalm for David. The Lord ruleth me"), "DRA 22:1 beside the title: " + cells("#v0")[3].textContent);
  assert(cells("#v1")[3].textContent === "With the verse above", "then above: " + cells("#v1")[3].textContent);
  assert(cells("#v1")[1].querySelector(".orig-text[lang=he][dir=rtl]"), "Hebrew, right to left");
  assert(cells("#v1")[2].textContent.startsWith("1 The LORD is my shepherd;"), "WEB: " + cells("#v1")[2].textContent);
  // Remove a column
  $$(".pr-chip").find((c) => c.textContent.startsWith("Hebrew/Greek")).querySelector("button").click();
  await until(() => names() === "KJV,WEB,DRA", "Hebrew removed: " + names());
  // Read the WEB beside the others: it leads, in its own numbering
  $("#translation-button").click();
  await until(() => $("#translations").open, "translation picker");
  $$(".translation-item").find((b) => b.querySelector(".translation-abbr").textContent === "DRA").click();
  await until(() => $("#translation-label").textContent === "DRA" && $(".chapter-heading")?.textContent === "Psalm 22" && names().startsWith("DRA"), "the DRA leads: " + names());
  assert(names() === "DRA,WEB", "the others follow: " + names());
  assert(switchButtons().map((b) => b.textContent).join() === "DRA,Parallel", "the KJV's own views aren't offered: " + switchButtons().map((b) => b.textContent));
  assert(cells("#v1")[1].textContent.startsWith("title A Psalm by David.") || cells("#v1")[1].textContent.includes("23 (title)"), "the WEB's title and verse 1 beside DRA 22:1: " + cells("#v1")[1].textContent);
  // Back to the KJV, plain text, and the usual columns (settings persist between tests)
  switchButtons()[0].click();
  await until(() => !$(".parallel-reading"), "plain text");
  $("#translation-button").click();
  await until(() => $("#translations").open, "translation picker");
  $$(".translation-item").find((b) => b.querySelector(".translation-abbr").textContent === "KJV").click();
  await until(() => $("#translation-label").textContent === "KJV" && $("#reader").getAttribute("aria-busy") === "false", "back to the KJV");
`);

await test("Parallel on a phone: the columns stack, each named", "book=John&chapter=3&view=parallel&tr=kjv", { width: 390, height: 844, mobile: true }, `
  await until(() => $(".parallel-reading"), "parallel");
  const cell = $("#v16 .pr-cell");
  assert(getComputedStyle(cell, "::before").content === '"KJV"', "named: " + getComputedStyle(cell, "::before").content);
  assert(cell.textContent.startsWith("16 For God so loved"), "the name isn't part of the text: " + cell.textContent);
  const [a, b] = $("#v16").querySelectorAll(".pr-cell");
  assert(b.getBoundingClientRect().top >= a.getBoundingClientRect().bottom - 1, "stacked");
  assert(document.documentElement.scrollWidth <= document.documentElement.clientWidth, "no sideways scrolling");
  // Back to the plain text (settings persist between tests)
  [...$$("[data-view-switch] button")].filter(visible)[0].click();
  await until(() => !$(".parallel-reading"), "plain text");
`);

await test("Keyboard: arrows change chapter and view", "book=Genesis&chapter=50", {}, `
  document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
  await until(() => $("#ref-label").textContent === "Exodus 1", "Exodus 1");
  const kjv = viewButton("KJV");
  kjv.focus();
  kjv.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
  await until(() => $("#app").dataset.view === "parallel", "parallel view");
  assert(document.activeElement.textContent === "Parallel", "focus follows the view");
`);

await test("Phone: tab bar and full-screen panels", "book=John&chapter=3", { width: 390, height: 844, mobile: true }, `
  assert(visible($(".tabbar")), "tab bar visible");
  $('[data-tab="search"]').click();
  await until(() => !$("#panel").hidden, "panel");
  const header = $(".panel-header").getBoundingClientRect();
  const hit = document.elementFromPoint(header.left + 40, header.top + header.height / 2);
  assert($(".panel-header").contains(hit), "panel header is on top");
  $('[data-tab="read"]').click();
  await until(() => $("#panel").hidden, "panel closed");
`);

// ------------------------------------------------------------------ AI assistant

const MOCK = { id: "mock", preset: "local", name: "Test server", kind: "openai", baseUrl: "http://127.0.0.1:8765/v1", contextWindow: null };
async function aiSettings(ai) {
  // Leave the app first: it saves its own settings as it unloads
  await send("Page.navigate", { url: "about:blank" });
  await sleep(300);
  await fetch(`${BASE}/api/settings_save`, {
    method: "POST",
    body: JSON.stringify({ ai: { providers: [MOCK], providerId: "mock", model: null, scope: "chapter", books: [], consent: {}, calibration: {}, ...ai } }),
  });
}
const CHAT_HELPERS = `
  const ask = async (text) => {
    const input = await until(() => $("#chat-input"), "chat input");
    input.value = text;
    input.dispatchEvent(new Event("input"));
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
  };
  const lastAnswer = () => $$(".msg.assistant").at(-1);
  const finished = () => $(".chat-send").getAttribute("aria-label") === "Send" && lastAnswer();
`;

await aiSettings({});
await test("Chat asks consent, then streams an answer about the attached chapter", "book=John&chapter=11&verse=35", {}, `${CHAT_HELPERS}
  $('[data-open-panel="chat"]').click();
  await until(() => $(".chat-model")?.value.endsWith("mock-model"), "model list");
  assert($(".chat-scope-label").textContent === "Reads: John 11", "chapter attached: " + $(".chat-scope-label").textContent);
  assert($(".chat-scope-size").textContent === "≈2k of 32k tokens", "budget: " + $(".chat-scope-size").textContent);
  await ask("Why did Jesus weep?");
  await until(() => !$(".chat-consent").hidden, "consent prompt");
  assert($(".chat-consent").textContent.includes("127.0.0.1:8765"), "consent names the server");
  $$(".chat-consent button").find((b) => b.textContent === "Allow and send").click();
  await until(() => finished() && lastAnswer().querySelector(".msg-tools"), "answer");
  assert($("#chat-input").value === "", "the question box is cleared after sending");
  const body = lastAnswer().querySelector(".msg-body");
  assert(body.querySelector("strong")?.textContent === "John 11", "the model got John 11: " + body.textContent);
  assert(body.textContent.includes("(57 verses)"), "all 57 verses sent");
  assert(lastAnswer().querySelector(".msg-reasoning summary").textContent.startsWith("Reasoning ·"), "reasoning kept apart");
  const refs = $$(".msg.assistant .ref-link").map((b) => b.textContent);
  assert(refs.join() === "John 11:35,Romans 12:15", "references linked: " + refs);
  refs && $$(".msg.assistant .ref-link")[1].click();
  await until(() => $("#ref-label").textContent === "Romans 12" && $("#v15")?.getAttribute("aria-current") === "true", "reference opens the verse");
  await until(() => $(".chat-scope-label").textContent === "Reads: Romans 12", "the passage follows the reader");
`);

await aiSettings({ consent: { mock: true } });
await test("Chat: Stop, errors, and a scope too large for the model", "book=John&chapter=11", {}, `${CHAT_HELPERS}
  $('[data-open-panel="chat"]').click();
  await until(() => $(".chat-model")?.value.endsWith("mock-model"), "model list");
  await ask("slow answer please");
  await until(() => $(".chat-send").getAttribute("aria-label") === "Stop", "streaming");
  await wait(1200);
  $(".chat-send").click();
  await until(() => finished() && lastAnswer().querySelector(".chat-note")?.textContent === "Stopped.", "stopped");
  await ask("please fail");
  await until(() => finished() && lastAnswer().querySelector(".chat-error"), "error shown");
  assert(lastAnswer().querySelector(".chat-error").textContent === "The service had an error (500): mock failure", "error text");
  $(".chat-scope-button").click();
  await until(() => $("#context-editor")?.open && $(".ctx-passage-size"), "context editor");
  $('.ctx-passage .icon-btn[aria-label^="Remove"]').click();
  await until(() => !$(".ctx-passage"), "passage removed");
  $$(".ctx-quick .chip").find((b) => b.textContent === "Whole Bible").click();
  await until(() => $(".ctx-size .meter.over"), "over budget in the editor");
  assert($(".ctx-size-line").textContent.startsWith("≈1.12M tokens of 32k"), "whole Bible size: " + $(".ctx-size-line").textContent);
  $("#context-editor .picker-header .icon-btn").click();
  await until(() => $(".chat-scope-size.over"), "over budget");
  assert($(".chat-scope-label").textContent === "Reads: The whole Bible", "label: " + $(".chat-scope-label").textContent);
  assert($(".chat-scope-size").textContent === "≈1.12M of 32k tokens", "whole Bible size: " + $(".chat-scope-size").textContent);
  const before = $$(".msg").length;
  await ask("anything");
  await wait(400);
  assert($$(".msg").length === before, "nothing sent when it can't fit");
  assert($("#context-editor").open, "the editor opens to make room");
  $("#context-editor").close();
`);
await aiSettings({ consent: { mock: true }, context: { passages: [{ follow: "verse" }], translations: ["reading"] } });
await test("Chat: choose passages, translations, commentaries, and cross-references, and see what is sent", "book=John&chapter=11&verse=35", {}, `${CHAT_HELPERS}
  const section = (title) => $$(".ctx-section").find((s) => s.querySelector(".section-title").textContent === title);
  const chip = (title, name) => [...section(title).querySelectorAll(".chip")].find((c) => c.textContent === name);
  $('[data-open-panel="chat"]').click();
  await until(() => $(".chat-model")?.value.endsWith("mock-model"), "model list");
  await until(() => $(".chat-scope-label").textContent === "Reads: John 11:35", "this verse: " + $(".chat-scope-label").textContent);
  $(".chat-scope-button").click();
  await until(() => $("#context-editor")?.open && $(".ctx-passage-size"), "context editor");
  assert($(".ctx-passage-label").textContent === "John 11:35This verse, as you read", "following passage: " + $(".ctx-passage-label").textContent);
  // Typed passages: one per chapter, a bad one explained
  const input = $(".ctx-add input");
  input.value = "Hezekiah 4";
  input.dispatchEvent(new Event("input"));
  $$(".ctx-add .button").find((b) => b.textContent === "Add").click();
  await until(() => $(".ctx-section .chat-error")?.textContent === "No book called “Hezekiah”", "bad reference explained");
  $(".ctx-add input").value = "Luke 2:14; Rom 5:1-2";
  $(".ctx-add input").dispatchEvent(new Event("input"));
  $$(".ctx-add .button").find((b) => b.textContent === "Add").click();
  await until(() => $$(".ctx-passage").length === 3, "two passages added");
  // Sources: the WEB beside the KJV, two commentaries, OpenBible's top 5 with their words
  chip("Commentaries", "Jamieson-Fausset-Brown").click();
  chip("Commentaries", "Tyndale").click();
  chip("Cross-references", "OpenBible").click();
  await until(() => section("Cross-references").querySelector("[role=radiogroup]"), "limit choices");
  [...section("Cross-references").querySelectorAll("[role=radio]")].find((b) => b.textContent === "5").click();
  assert(!chip("Commentaries", "Treasury of Scripture Knowledge"), "the Treasury is offered as cross-references only");
  $$(".chip").find((c) => c.textContent === "Add a translation").click();
  await until(() => $("#translations").open, "translation picker");
  assert($("#translations-title").textContent === "Add a translation", "picker titled for adding");
  $$("#translations .translation-item").find((b) => b.querySelector(".translation-abbr").textContent === "WEB").click();
  await until(() => $$(".ctx-chip-removable").length === 2, "WEB added");
  await until(() => $$(".ctx-passage-size").length === 3 && $$(".ctx-parts").length === 3 && $(".ctx-parts").textContent.includes("WEB"), "sized");
  const parts = $$(".ctx-parts")[1].textContent;
  assert(/^KJV \\d+ · WEB \\d+ · Jamieson-Fausset-Brown \\d+ · Tyndale \\d+ · OpenBible \\d+/.test(parts), "parts of Luke 2:14: " + parts);
  // Romans gets its own commentaries: none
  $$(".ctx-options-button")[2].click();
  const commentaryRow = await until(() => $$(".ctx-override").find((o) => o.textContent.startsWith("Commentaries")), "options");
  [...commentaryRow.querySelectorAll("[role=radio]")].find((b) => b.textContent === "Its own").click();
  await until(() => $$(".ctx-override").find((o) => o.textContent.startsWith("Commentaries"))?.querySelector(".chip[aria-pressed=true]"), "its own list, copied from the rest");
  for (const c of $$(".ctx-override")[1].querySelectorAll(".chip[aria-pressed=true]")) c.click();
  await until(() => !$$(".ctx-parts")[2]?.textContent.includes("Tyndale"), "Romans without commentaries: " + $$(".ctx-parts")[2]?.textContent);
  // What's sent
  $$(".button").find((b) => b.textContent === "Show what’s sent").click();
  const pre = await until(() => $(".ctx-preview"), "preview");
  const sent = pre.textContent;
  assert(sent.startsWith("You are the study assistant"), "instructions first");
  assert(sent.includes("The reader has attached John 11:35; Luke 2:14; Romans 5:1–2 below"), "label in the instructions");
  assert(sent.includes('<passage ref="Luke 2:14">\\n<bible translation="King James Version" abbr="KJV"'), "passages in order");
  assert(sent.includes('abbr="WEB" year="2020" ref="Luke 2:14">\\n## Luke 2\\n14 “Glory to God in the highest'), "the WEB's text");
  assert(sent.includes('<commentary name="Jamieson-Fausset-Brown Commentary"'), "commentary notes");
  const romans = sent.split('<passage ref="Romans 5:1–2">')[1];
  assert(romans && !romans.includes("<commentary"), "Romans has its own (no) commentaries");
  assert(romans.includes('<crossrefs name="OpenBible.info Cross References" numbering="KJV">\\nRomans 5:1\\n- '), "cross-references with words");
  // Save it, then use it again after changing things
  $$(".button").find((b) => b.textContent === "Save this context…").click();
  const name = await until(() => $('[data-focus-key="name"]'), "name box");
  name.value = "Christmas peace";
  name.dispatchEvent(new Event("input"));
  $$(".button").find((b) => b.textContent === "Save").click();
  await until(() => $(".ctx-set-name")?.textContent === "Christmas peace", "saved");
  $$('.ctx-passage .icon-btn[aria-label^="Remove"]')[1].click();
  await until(() => $$(".ctx-passage").length === 2, "removed one");
  $$(".ctx-set .button").find((b) => b.textContent === "Use").click();
  await until(() => $$(".ctx-passage").length === 3, "the saved context back");
  // Reorder: Romans before Luke, and back
  const order = () => $$(".ctx-passage-label").map((l) => l.firstChild.textContent).join(" | ");
  $$(".ctx-options-button")[2].click();
  (await until(() => $$(".ctx-move .button").find((b) => b.textContent === "Move up"), "move buttons")).click();
  await until(() => order() === "John 11:35 | Romans 5:1–2 | Luke 2:14", "moved up: " + order());
  $$(".ctx-move .button").find((b) => b.textContent === "Move down").click();
  await until(() => order() === "John 11:35 | Luke 2:14 | Romans 5:1–2", "moved back: " + order());
  $("#context-editor").close();
  await until(() => $(".chat-scope-label").textContent === "Reads: John 11:35; Luke 2:14; Romans 5:1–2", "bar: " + $(".chat-scope-label").textContent);
  await ask("What peace is meant?");
  await until(() => finished() && lastAnswer().querySelector(".msg-tools"), "answer");
  assert(lastAnswer().querySelector(".msg-body strong")?.textContent === "John 11:35; Luke 2:14; Romans 5:1–2", "the model got all three: " + lastAnswer().textContent);
  assert($(".msg-scope").textContent === "With John 11:35; Luke 2:14; Romans 5:1–2", "the question says what it went with");
`);
await aiSettings({ consent: { mock: true } });
await test("Chat: scrolling stays with the reader while an answer streams", "book=John&chapter=11", {}, `${CHAT_HELPERS}
  const gap = (el) => Math.round(el.scrollHeight - el.scrollTop - el.clientHeight);
  $('[data-open-panel="chat"]').click();
  await until(() => $(".chat-model")?.value.endsWith("mock-model"), "model list");
  await ask("long answer please");
  const rb = await until(() => { const b = $(".msg.assistant .msg-reasoning-body"); return b && b.scrollHeight > b.clientHeight + 80 ? b : null; }, "long reasoning", 20000);
  await wait(200);
  assert(gap(rb) === 0, "reasoning follows its newest line: gap " + gap(rb));
  rb.dispatchEvent(new WheelEvent("wheel"));
  rb.scrollTop = 40;
  await wait(500);
  assert(rb.scrollTop === 40, "reasoning stays where the reader scrolled it: " + rb.scrollTop);
  await until(() => $(".msg.assistant .msg-body")?.textContent.length > 300, "answer", 60000);
  assert($(".msg-reasoning").open && rb.isConnected, "reasoning the reader is in stays open and isn't rebuilt");
  const pane = $(".chat-messages");
  await until(() => pane.scrollHeight > pane.clientHeight + 400, "long answer", 30000);
  pane.scrollTop = 120;
  await wait(600);
  assert(pane.scrollTop === 120, "conversation stays put while text streams in: " + pane.scrollTop);
  assert(!$(".chat-jump").hidden, "Latest button shown");
  $(".chat-jump").click();
  await wait(300);
  assert(gap(pane) === 0 && $(".chat-jump").hidden, "Latest jumps to the bottom and follows again");
  await until(() => $(".msg-tools"), "finished", 60000);
  assert(gap(pane) === 0, "still at the bottom when it finishes");
`);
await aiSettings({
  providers: [MOCK, { ...MOCK, id: "mock2", preset: "custom", name: "Second server" }],
  consent: { mock: true, mock2: true },
});
await test("Chat: several providers in one menu", "book=John&chapter=11", {}, `${CHAT_HELPERS}
  $('[data-open-panel="chat"]').click();
  await until(() => $$(".chat-model option").filter((o) => o.value.endsWith("mock-model")).length === 2, "both model lists");
  assert($$(".chat-model optgroup").map((g) => g.label).join() === "Test server,Second server", "a group per provider");
  assert($$(".chat-model option").at(-1).textContent === "Add a provider…", "add entry at the end");
  assert($(".think-toggle"), "Think first for your own server");
  $(".chat-model").value = "mock2\\nmock-model";
  $(".chat-model").dispatchEvent(new Event("change"));
  await until(() => !$(".think-toggle"), "Think first hidden for other services");
  await ask("hello");
  await until(() => finished() && lastAnswer().querySelector(".msg-tools"), "answer from the second provider");
  $(".chat-model").value = "__add__";
  $(".chat-model").dispatchEvent(new Event("change"));
  await until(() => $("#provider-preset"), "new provider form");
  $("#provider-preset").value = "deepseek";
  $("#provider-preset").dispatchEvent(new Event("change"));
  await until(() => $("#provider-url")?.value === "https://api.deepseek.com/v1", "DeepSeek's address filled in");
`);
await aiSettings({ consent: { mock: true } });
await test("Chat: conversations are saved, starred, renamed, reopened, and cleared", "book=John&chapter=11", {}, `${CHAT_HELPERS}
  const rows = () => $$(".conversation-row .row-main").map((e) => e.textContent);
  const historyView = () => $('[aria-label="Conversations"]');
  // Start from an empty list (the earlier chat tests saved theirs)
  const api = (name, body) => fetch("/api/" + name, { method: "POST", body: JSON.stringify(body) }).then((r) => r.json());
  for (const c of await api("conversations_list", {})) await api("conversation_delete", { id: c.id });
  $('[data-open-panel="chat"]').click();
  await until(() => $(".chat-model")?.value.endsWith("mock-model"), "model list");
  await ask("Why did Jesus weep at the tomb of Lazarus?");
  await until(() => finished() && lastAnswer().querySelector(".msg-tools"), "first answer");
  assert($(".chat-scope-label").textContent === "Reads: John 11", "the usual context: " + $(".chat-scope-label").textContent);
  await wait(200);
  assert(!$("[data-new-conversation]").disabled, "New conversation is enabled once there's a conversation");
  $("[data-new-conversation]").click();
  await ask("What does the word Logos mean in John 1?");
  await until(() => finished() && $$(".msg-tools").length === 1 && $(".msg.user").textContent.includes("Logos"), "second answer in a new conversation");
  await wait(200);
  historyView().click();
  await until(() => rows().length === 2, "both in the list");
  assert(rows()[0].startsWith("What does the word Logos"), "newest first: " + rows());
  $$(".conversation-row").find((r) => r.textContent.includes("Lazarus")).querySelector("[aria-pressed]").click();
  await until(() => $$(".chat-history .section-title").map((e) => e.textContent).join() === "Saved,Recent", "starred one under Saved");
  $$(".conversation-row").find((r) => r.textContent.includes("Logos")).querySelector('[title="Rename"]').click();
  const box = await until(() => $(".conversation-edit input"), "rename box");
  box.value = "The Word in John 1";
  box.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
  await until(() => rows().includes("The Word in John 1"), "renamed");
  $$(".conversation-row").find((r) => r.textContent.includes("Lazarus")).querySelector(".row-button").click();
  await until(() => $(".msg.user")?.textContent.includes("Lazarus"), "reopened");
  // Follow-ups go with what the conversation was asked with, wherever the reader is
  await until(() => $(".chat-scope-label").textContent === "This conversation reads: John 11", "its own context: " + $(".chat-scope-label").textContent);
  await ask("Where else did Jesus weep?");
  await until(() => finished() && $$(".msg-tools").length === 2, "continued");
  assert(lastAnswer().querySelector(".msg-body strong")?.textContent === "John 11", "sent with John 11");
  await wait(200);
  historyView().click();
  await until(() => $$(".conversation-row .row-sub").some((e) => e.textContent.endsWith("2 questions")), "follow-up saved to the same conversation");
  $$(".conversation-clear button").find((b) => b.textContent === "Clear history").click();
  $$(".conversation-clear button").find((b) => b.textContent === "Delete").click();
  await until(() => rows().length === 1 && rows()[0].includes("Lazarus"), "clear history keeps the saved one");
`);
await aiSettings({ providers: [], providerId: null });

// Longest chapter, longest glosses, longest book name; smallest and largest text
const overflowCases = [["Psalms", 119], ["Deuteronomy", 25], ["Second%20Thessalonians", 3]];
for (const width of [320, 768, 1440]) {
  for (const [book, chapter] of overflowCases) {
    for (const scale of [1, 1.7]) {
      for (const view of ["kjv", "parallel", "interlinear", "original"]) {
    await test(`No horizontal overflow: ${width}px ${book} ${chapter} ${view} ${scale}x`, `book=${book}&chapter=${chapter}&view=${view}&scale=${scale}`, { width, height: 900, mobile: width < 900 }, `
      const doc = document.documentElement;
      assert(doc.scrollWidth <= doc.clientWidth, "page scrolls sideways: " + doc.scrollWidth + " > " + doc.clientWidth);
      const reader = $("#reader");
      assert(reader.scrollWidth <= reader.clientWidth + 1, "reader scrolls sideways: " + reader.scrollWidth + " > " + reader.clientWidth);
    `);
      }
    }
  }
}

// ------------------------------------------------------------------ translations

await test("Translations: pick one, read it, keep the place", "book=John&chapter=3&tr=kjv", {}, `
  assert($("#translation-label").textContent === "KJV", "starts on the KJV");
  $("#translation-button").click();
  await until(() => $("#translations").open, "translation picker");
  assert($$(".translation-item").length === 44, "44 translations: " + $$(".translation-item").length);
  assert($(".translation-item[aria-current='true'] .translation-abbr").textContent === "KJV", "current marked");
  const find = $("#translations input");
  find.value = "berean";
  find.dispatchEvent(new Event("input"));
  await until(() => $$(".translation-item").length === 1, "filtered to the BSB");
  $(".translation-item").click();
  await until(() => $("#translation-label").textContent === "BSB" && $("#ref-label").textContent === "John 3" && $("#reader").getAttribute("aria-busy") === "false", "BSB John 3");
  assert($("#v16").textContent.includes("For God so loved the world"), "John 3:16 in the BSB");
  const offered = $$("[data-view-switch] button").filter(visible).map((b) => b.textContent).join();
  assert(offered === "BSB,Parallel", "the text and Parallel, not the KJV's own views: " + offered);
  // Footnotes open under their verse
  const note = $("#reader .note-ref");
  note.click();
  await until(() => note.closest(".verse").querySelector(".verse-note"), "footnote shown");
  note.click();
  assert(!note.closest(".verse").querySelector(".verse-note"), "footnote hidden again");
  // Back to the KJV (settings persist between tests)
  $("#translation-button").click();
  await until(() => $("#translations").open, "translation picker again");
  $$(".translation-item").find((b) => b.querySelector(".translation-abbr").textContent === "KJV").click();
  await until(() => $("#translation-label").textContent === "KJV" && $("#reader").getAttribute("aria-busy") === "false", "back to the KJV");
`);

await test("Translations: switching keeps the place, across different numbering", "book=Psalms&chapter=23&tr=kjv", {}, `
  $("#v4").click();
  $("#translation-button").click();
  await until(() => $("#translations").open, "translation picker");
  $$(".translation-item").find((b) => b.querySelector(".translation-abbr").textContent === "DRA").click();
  // The Douay-Rheims numbers the Psalms as the Vulgate does: KJV 23 is its 22
  await until(() => $("#translation-label").textContent === "DRA" && $("#reader").getAttribute("aria-busy") === "false", "Douay-Rheims");
  assert($("#ref-label").textContent === "Psalm 22", "Psalm 22: " + $("#ref-label").textContent);
  assert($(".verse[aria-current='true']")?.dataset.label === "4", "verse 4 still selected");
  assert($(".verse[aria-current='true']").textContent.includes("shadow of death"), "the same verse");
  $("#translation-button").click();
  await until(() => $("#translations").open, "translation picker again");
  $$(".translation-item").find((b) => b.querySelector(".translation-abbr").textContent === "KJV").click();
  await until(() => $("#translation-label").textContent === "KJV" && $("#ref-label").textContent === "Psalm 23", "back to KJV Psalm 23");
`);

await test("Translations: words of Jesus in red", "book=John&chapter=11&tr=web", {}, `
  assert($("#v43 .red")?.textContent.includes("Lazarus, come out"), "John 11:43 in red");
  assert(!$("#v35 .red"), "narration is not red");
`);

await test("Translations: the divine name in small capitals, poetry in lines", "book=Psalms&chapter=23&tr=kjvcpb", {}, `
  const lord = $("#v1 .sc");
  assert(lord?.textContent === "Lord", "KJV-CPB Psalm 23:1 marks Lord: " + lord?.textContent);
  assert(getComputedStyle(lord).fontVariantCaps === "small-caps", "drawn in small capitals");
  assert($("#v0.is-title")?.textContent.startsWith("A Psalm of David"), "the title");
  assert($$("#v1 .line").length >= 1, "verse set in lines");
`);

await test("Translations: the KJV keeps its interlinear and gains the Apocrypha", "book=Tobit&chapter=1&tr=kjv", {}, `
  assert($("#ref-label").textContent === "Tobit 1", "KJV Tobit 1 opens");
  assert($("#v1").textContent.includes("The book of the words of Tobit"), "Tobit 1:1");
  const offered = $$("[data-view-switch] button").filter(visible).map((b) => b.textContent).join();
  assert(offered === "KJV,Parallel", "no interlinear for the Apocrypha: " + offered);
  $("#ref-button").click();
  await until(() => $("#picker").open, "book picker");
  assert($$(".picker .section-title").map((e) => e.textContent).join() === "Old Testament,Apocrypha,New Testament", "three sections");
  $$(".book-grid button").find((b) => b.textContent === "John").click();
  await until(() => $(".chapter-grid"), "chapters");
  $$(".chapter-grid button").find((b) => b.textContent === "1").click();
  await until(() => $("#ref-label").textContent === "John 1" && $("#reader").getAttribute("aria-busy") === "false", "John 1");
  assert($(".word") || visible($("[data-view-switch]")), "back in the KJV's own reader");
`);

const NOTES_HELPERS = `
  const panel = $("#panel");
  const where = () => panel.querySelector(".notes-where")?.textContent;
  const section = (name) => $$("#panel .commentary").find((c) => c.querySelector(".commentary-name").textContent.startsWith(name));
  const labels = (name) => [...(section(name)?.querySelectorAll(".note-label .note-place") ?? [])].map((l) => l.textContent);
`;

await test("Commentary: notes on the selected verse, through the KJV's numbering", "book=Psalms&chapter=22&tr=dra&verse=4&select=1&panel=notes", {}, `${NOTES_HELPERS}
  await until(() => section("Matthew Henry"), "Matthew Henry's notes");
  assert(where() === "On Psalm 22:4", "the Douay-Rheims' verse: " + where());
  assert($(".notes-kjv")?.textContent.includes("Psalm 23:4"), "which is the KJV's Psalm 23:4");
  assert(labels("Matthew Henry").join() === "Psalm 23:1-6", "Matthew Henry on KJV Psalm 23: " + labels("Matthew Henry"));
  assert(labels("Treasury").join() === "Psalm 23:4", "the Treasury on 23:4: " + labels("Treasury"));
  assert(section("Treasury").querySelector(".commentary-credit").textContent.length > 20, "credited");
  assert(panel.textContent.includes("Nothing on Psalms from Catena Aurea"), "Catena covers the Gospels only");
  $("#v5").click();
  await until(() => where() === "On Psalm 22:5" && labels("Treasury").join() === "Psalm 23:5", "follows the selected verse");
  $('[data-action="deselect"]').click();
  await until(() => /introductions/.test(where() ?? "") && labels("Matthew Henry").join() === "Psalm 23 (introduction)", "the chapter's introduction");
`);

await test("Commentary: choose commentaries and follow a reference", "book=John&chapter=3&tr=kjv&verse=16&select=1&panel=notes", {}, `${NOTES_HELPERS}
  await until(() => section("Treasury"), "the Treasury's notes");
  assert(where() === "On John 3:16" && !$(".notes-kjv"), "the KJV's own numbering");
  assert(section("Catena Aurea") && section("John Gill") && section("Wesley"), "Catena, Gill, Wesley");
  assert(panel.textContent.includes("Nothing on John from Keil & Delitzsch"), "Keil & Delitzsch is the Old Testament only");
  const chip = (name) => $$("#panel .chip").find((c) => c.textContent === name);
  chip("Wesley").click();
  await until(() => !section("Wesley") && chip("Wesley")?.getAttribute("aria-pressed") === "false", "Wesley turned off");
  chip("Wesley").click();
  await until(() => section("Wesley"), "Wesley back");
  // Long notes start folded
  assert(section("Matthew Henry").querySelector("details.note"), "Matthew Henry's long note folded");
  // A reference in a note opens the passage, and the notes follow
  // The Treasury lists several places under one reference; each opens its own
  const links = [...section("Treasury").querySelectorAll(".note-link")];
  assert(links.find((a) => a.textContent === "10")?.title === "Open 1 John 4:10", "1Jo 4:9,10,19: 10 opens 1 John 4:10");
  assert(links.find((a) => a.textContent === "2Co 5:19-21")?.title === "Open 2 Corinthians 5:19-21", "a range");
  const link = links.find((a) => a.textContent === "Lu 2:14");
  assert(link, "the Treasury links Luke 2:14");
  link.click();
  await until(() => $("#ref-label").textContent === "Luke 2" && $("#reader").getAttribute("aria-busy") === "false", "Luke 2");
  await until(() => $(".verse[aria-current='true']")?.id === "v14", "Luke 2:14 selected");
  await until(() => where() === "On Luke 2:14", "notes on Luke 2:14");
  // The verse bar opens the panel too
  $("#panel-close").click();
  await until(() => $("#panel").hidden, "panel closed");
  $('[data-action="notes"]').click();
  await until(() => !$("#panel").hidden && where() === "On Luke 2:14", "Notes from the verse bar");
`);

await test("Commentary: the Tyndale notes, book introductions, and articles", "book=Romans&chapter=2&tr=kjv&verse=8&select=1&panel=notes", {}, `${NOTES_HELPERS}
  await until(() => section("Tyndale Open Study Notes"), "the Tyndale notes");
  const ty = section("Tyndale Open Study Notes");
  // The chiasm of Romans 2:6-11, indented as printed
  const items = [...ty.querySelectorAll(".note-item")];
  assert(items.map((i) => i.className).join() === "note-item,note-item level-2,note-item level-3,note-item level-3,note-item level-2,note-item", "the chiasm's levels");
  // A transliteration: italic, in the text's own font
  const greek = ty.querySelector('[lang="grc-Latn"]');
  assert(greek && getComputedStyle(greek).fontStyle === "italic", "eritheia in italic");
  assert(ty.querySelector(".commentary-credit").textContent.startsWith("Adapted from Tyndale Open Study Notes."), "Tyndale's attribution");
  // A book's introductions come with its first chapter
  {
    $("#ref-button").click();
    await until(() => $("#picker").open, "book picker");
    $$(".book-grid button").find((b) => b.textContent === "Genesis").click();
    await until(() => $(".chapter-grid"), "chapters");
    $$(".chapter-grid button").find((b) => b.textContent === "1").click();
  }
  await until(() => $("#ref-label").textContent === "Genesis 1" && /introductions/.test(where() ?? ""), "Genesis 1");
  await until(() => labels("Tyndale Open Study Notes").join() === "Genesis (introduction),Genesis (introduction)", "Genesis's summary and introduction");
  // Articles on a passage fold, named by their titles
  $("#ref-button").click();
  await until(() => $("#picker").open, "book picker");
  $$(".book-grid button").find((b) => b.textContent === "Genesis").click();
  await until(() => $(".chapter-grid"), "chapters");
  $$(".chapter-grid button").find((b) => b.textContent === "3").click();
  await until(() => $("#ref-label").textContent === "Genesis 3" && $("#reader").getAttribute("aria-busy") === "false", "Genesis 3");
  $("#v6").click();
  await until(() => section("Tyndale Open Study Notes: Profiles and Themes"), "the articles");
  const titles = [...section("Tyndale Open Study Notes: Profiles and Themes").querySelectorAll("summary")].map((x) => x.textContent);
  assert(titles.some((t) => t.startsWith("Adam and Eve · Genesis 2:7-4:2")), "Adam and Eve, folded: " + titles.join(" | "));
`);

await test("Commentary: the Church Fathers, homily by homily", "book=Matthew&chapter=5&tr=kjv&verse=3&select=1&panel=notes", {}, `${NOTES_HELPERS}
  await until(() => section("John Chrysostom") && section("Augustine"), "Chrysostom and Augustine");
  // Homily XV runs from 5:1 to the next homily's 5:17, folded under its own heading
  const homily = [...section("John Chrysostom").querySelectorAll("summary")].map((x) => x.textContent);
  assert(homily.some((t) => t.startsWith("Homily XV. · Matthew 5:1-16")), "Homily XV: " + homily.join(" | "));
  assert(section("John Chrysostom").querySelector(".commentary-credit").textContent.includes("Nicene and Post-Nicene Fathers"), "credited");
  // A heading the edition prints in capitals keeps its letters, drawn in small capitals
  const sermon = section("Augustine").querySelector("summary");
  assert(sermon && /sermon on the mount/i.test(sermon.textContent), "Augustine on the Sermon on the Mount");
  assert(getComputedStyle(section("Augustine").querySelector(".note-body .sc") ?? document.body).fontVariantCaps === "small-caps", "capitals as small capitals");
  // In the Douay-Rheims, its Psalm 22 is Augustine's Psalm XXIII (Lat. XXII)
  $("#translation-button").click();
  await until(() => $("#translations").open, "translation picker");
  $$(".translation-item").find((b) => b.querySelector(".translation-abbr").textContent === "DRA").click();
  await until(() => $("#translation-label").textContent === "DRA" && $("#reader").getAttribute("aria-busy") === "false", "Douay-Rheims");
  $("#ref-button").click();
  await until(() => $("#picker").open, "book picker");
  $$(".book-grid button").find((b) => b.textContent === "Psalms").click();
  await until(() => $(".chapter-grid"), "chapters");
  $$(".chapter-grid button").find((b) => b.textContent === "22").click();
  await until(() => $("#ref-label").textContent === "Psalm 22" && $("#reader").getAttribute("aria-busy") === "false", "Psalm 22");
  $("#v1").click();
  await until(() => [...(section("Augustine")?.querySelectorAll("summary") ?? [])].some((x) => x.textContent.startsWith("Psalm XXIII. Lat. XXII. · Psalm 23:1-6")), "Augustine on Psalm 23");
  // Back to the KJV for the tests that follow
  $("#translation-button").click();
  await until(() => $("#translations").open, "translation picker again");
  $$(".translation-item").find((b) => b.querySelector(".translation-abbr").textContent === "KJV").click();
  await until(() => $("#translation-label").textContent === "KJV", "back to the KJV");
`);

const XREF_HELPERS = `
  const panel = $("#panel");
  const where = () => panel.querySelector(".notes-where")?.textContent;
  const collection = (id) => panel.querySelector('[data-collection="' + id + '"]');
  const places = (id) => [...(collection(id)?.querySelectorAll(".xref") ?? [])];
  const label = (x) => x.querySelector(".xref-ref").textContent;
`;

await test("Cross-references: the Treasury's keywords and OpenBible's ranked list, with their words", "book=John&chapter=3&tr=kjv&verse=16&select=1&panel=xrefs", {}, `${XREF_HELPERS}
  await until(() => collection("tsk") && collection("openbible"), "both collections");
  assert(where() === "From John 3:16" && !$(".notes-kjv"), "from John 3:16: " + where());
  const first = collection("tsk").querySelector(".xref-line");
  assert(first.querySelector(".xref-words").textContent === "God.", "the Treasury's first keyword");
  assert(label(first.querySelector(".xref")) === "Luke 2:14", "under it, Luke 2:14");
  assert(first.querySelector(".xref-text").textContent.startsWith("Glory to God in the highest"), "with its words");
  const range = places("tsk").find((x) => label(x) === "2 Corinthians 5:19-21");
  assert(range && [...range.querySelectorAll(".xref-num")].map((n) => n.textContent).join() === "19,20,21", "a range, verse by verse");
  // OpenBible: most helpful first, twenty at first
  assert(label(places("openbible")[0]) === "Romans 5:8", "OpenBible's first: " + label(places("openbible")[0]));
  assert(places("openbible").length === 20, "twenty at first");
  collection("openbible").querySelector(".xref-all").click();
  await until(() => places("openbible").length === 23 && !collection("openbible").querySelector(".xref-all"), "all 23");
  // A place opens, and the panel follows
  places("tsk")[0].querySelector(".xref-ref").click();
  await until(() => $("#ref-label").textContent === "Luke 2" && $("#reader").getAttribute("aria-busy") === "false", "Luke 2");
  await until(() => $(".verse[aria-current='true']")?.id === "v14", "Luke 2:14 selected");
  await until(() => where() === "From Luke 2:14", "cross-references from Luke 2:14");
`);

await test("Cross-references: in the translation being read, with the KJV's words where it hasn't the place", "book=Psalms&chapter=22&tr=brenton&verse=1&select=1&panel=xrefs", {}, `${XREF_HELPERS}
  await until(() => collection("tsk") && collection("openbible"), "both collections");
  assert($(".notes-kjv")?.textContent.includes("Psalm 23:1"), "numbered as the KJV's Psalm 23:1");
  // Brenton numbers the Psalms as the Septuagint does: the KJV's 79:13 is its 78:13
  assert(places("tsk").some((x) => label(x) === "Psalm 78:13"), "Psalm 78:13 in Brenton's numbering");
  // Brenton has no New Testament: the KJV's words, marked
  const john = places("openbible").find((x) => label(x) === "John 10:11");
  assert(/^KJV · not in /.test(john?.querySelector(".xref-note")?.textContent ?? ""), "John 10:11 marked as the KJV's");
  assert(john.querySelector(".xref-text").textContent.startsWith("I am the good shepherd"), "with the KJV's words");
  assert(!john.querySelector("button"), "not a link: Brenton can't open it");
  // Choosing collections
  const chip = (name) => $$("#panel .chip").find((c) => c.textContent === name);
  chip("Treasury").click();
  await until(() => !collection("tsk") && collection("openbible"), "the Treasury turned off");
  chip("Treasury").click();
  await until(() => collection("tsk"), "the Treasury back");
`);

// Every verse of every translation on screen, compared with the data (whose text is
// checked against eBible's own editions in crates/library/tests). A full sweep of
// everything runs weekly (FULL_SWEEP=1); every push sweeps a representative set in
// full and the opening of every other translation.
{
  const sweep = readFileSync(new URL("./every_library_verse.js", import.meta.url), "utf8");
  const bibles = await (await fetch(`${BASE}/api/bibles`, { method: "POST", body: "{}" })).json();
  const full = new Set(process.env.FULL_SWEEP ? bibles.map((b) => b.id) : ["web", "dra", "brenton", "kjvcpb", "jps", "ojb"]);
  let chapters = 0;
  let verses = 0;
  const problems = [];
  for (const b of bibles) {
    // The KJV's own reader draws its 66 books; the library draws its Apocrypha
    const only = b.id === "kjv" ? b.books.filter((x) => x.section === "apocrypha").map((x) => x.name) : null;
    let start = null;
    for (const book of b.books.filter((x) => !only || only.includes(x.name))) {
      start = { book: book.name, chapter: book.numbers[0] };
      if (start) break;
    }
    try {
      await open(`tr=${b.id}&book=${encodeURIComponent(start.book)}&chapter=${start.chapter}`);
      const r = await run(`async () => (${sweep})(${JSON.stringify(b.id)}, ${full.has(b.id) ? 100000 : 25}, ${JSON.stringify(only)})`);
      chapters += r.chapters;
      verses += r.verses;
      problems.push(...r.problems.map((p) => `${b.id}: ${p}`));
    } catch (error) {
      problems.push(`${b.id}: ${error.message}`);
    }
  }
  results.push([
    `Every library verse on screen matches its text (${verses} verses, ${chapters} chapters; full: ${[...full].join(", ")})`,
    problems.length ? `FAIL: ${problems.length} problems\n      ${problems.slice(0, 40).join("\n      ")}` : "ok",
  ]);
}

results.push(["No console errors", consoleErrors.length ? `FAIL: ${consoleErrors.join(" | ")}` : "ok"]);

// ------------------------------------------------------------------ report

ws.close();
const exited = new Promise((resolve) => chrome.once("exit", resolve));
chrome.kill();
await Promise.race([exited, sleep(5000)]);
// Chrome's helpers can hold files for a moment after it exits
rmSync(profile, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
let failed = 0;
for (const [name, outcome] of results) {
  if (outcome !== "ok") failed++;
  console.log(`${outcome === "ok" ? "ok  " : "FAIL"}  ${name}${outcome === "ok" ? "" : "\n      " + outcome.slice(6)}`);
}
console.log(`\n${results.length - failed} passed, ${failed} failed`);
process.exit(failed ? 1 : 0);
