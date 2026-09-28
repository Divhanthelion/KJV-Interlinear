// UI smoke test: drives the real interface (served by kjv-devserver) in headless Chrome
// over the DevTools protocol. No npm dependencies; needs Node 22+ (global WebSocket).
//
//   cargo run --release -p kjv-devserver &      # serves http://localhost:1420
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
  const until = async (fn, what) => {
    for (let i = 0; i < 200; i++) { const v = fn(); if (v) return v; await wait(25); }
    throw new Error("timed out waiting for " + what);
  };
  const $ = (s) => document.querySelector(s);
  const $$ = (s) => [...document.querySelectorAll(s)];
  const assert = (cond, msg) => { if (!cond) throw new Error(msg); };
  const visible = (el) => !!el && el.offsetParent !== null;
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
