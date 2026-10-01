// UI smoke test: drives the real interface (served by kjv-devserver) in headless Chrome
// over the DevTools protocol. No npm dependencies; needs Node 22+ (global WebSocket).
//
//   cargo run --release -p kjv-devserver &      # serves http://localhost:1420
//   python3 tests/ui/mock_llm.py &              # a stand-in model at :8765
//   node tests/ui/smoke.mjs [path-to-chrome]

import { spawn } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const BASE = "http://localhost:1420";
const PORT = 9333;
const chromePath = process.argv[2] || process.env.CHROME || "google-chrome";

const chrome = spawn(chromePath, [
  "--headless=new", "--disable-gpu", "--no-first-run", "--no-default-browser-check",
  `--remote-debugging-port=${PORT}`, `--user-data-dir=${mkdtempSync(join(tmpdir(), "kjv-ui-"))}`, "about:blank",
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
  assert($("#verse-actions-ref").textContent === "Romans 8:28", "action bar reference");
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
  assert($(".chat-scope-label").textContent === "Attached: John 11", "chapter attached: " + $(".chat-scope-label").textContent);
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
  await until(() => $(".chat-scope-label").textContent === "Attached: Romans 12", "scope follows the reader");
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
  $$(".chat-scope [role=radio]").find((b) => b.textContent === "Whole Bible").click();
  await until(() => $(".chat-scope-size.over"), "over budget");
  assert($(".chat-scope-size").textContent === "≈1.12M of 32k tokens", "whole Bible size: " + $(".chat-scope-size").textContent);
  const before = $$(".msg").length;
  await ask("anything");
  await wait(400);
  assert($$(".msg").length === before, "nothing sent when it can't fit");
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
  await ask("Where else did Jesus weep?");
  await until(() => finished() && $$(".msg-tools").length === 2, "continued");
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

results.push(["No console errors", consoleErrors.length ? `FAIL: ${consoleErrors.join(" | ")}` : "ok"]);

// ------------------------------------------------------------------ report

ws.close();
chrome.kill();
let failed = 0;
for (const [name, outcome] of results) {
  if (outcome !== "ok") failed++;
  console.log(`${outcome === "ok" ? "ok  " : "FAIL"}  ${name}${outcome === "ok" ? "" : "\n      " + outcome.slice(6)}`);
}
console.log(`\n${results.length - failed} passed, ${failed} failed`);
process.exit(failed ? 1 : 0);
