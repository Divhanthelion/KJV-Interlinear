// App controller: navigation, selection, panels, keyboard, and persistence.

import { call, copyText } from "./backend.js";
import { h, icon, replace } from "./dom.js";
import { renderSaved, renderSearch, renderSettings, renderStrongs } from "./panels.js";
import { closePicker, initPicker, isPickerOpen, openPicker } from "./picker.js";
import { markSelected, renderChapter } from "./reader.js";
import * as prefs from "./settings.js";

const $ = (id) => document.getElementById(id);
const app = $("app");
const reader = $("reader");
const panel = $("panel");
const panelBody = $("panel-body");
const actions = $("verse-actions");
const desktop = window.matchMedia("(min-width: 900px)");

const PANELS = {
  search: { title: "Search", render: renderSearch },
  strongs: { title: "Strong's & Lexicon", render: renderStrongs },
  saved: { title: "Saved", render: renderSaved },
  settings: { title: "Settings", render: renderSettings },
};

const state = {
  books: [],
  bookMap: new Map(),
  chapter: null,
  selectedVerse: null,
  highlight: null,
  panel: null,
  search: { query: "", scope: "all", results: null },
  strongs: { query: "", results: null, pending: false },
};
let settings = prefs.sanitize(null);

// ------------------------------------------------------------------ names

const bookName = (book) => state.bookMap.get(book)?.display ?? book;
const heading = (book, chapter) => (book === "Psalms" ? `Psalm ${chapter}` : `${bookName(book)} ${chapter}`);
const reference = (book, chapter, verse) =>
  verse > 0 ? `${heading(book, chapter)}:${verse}` : `${heading(book, chapter)} (title)`;

// ------------------------------------------------------------------ chapters

const cache = new Map();
const CACHE_SIZE = 16;

async function fetchChapter(book, chapter) {
  const key = `${book}|${chapter}|${state.highlight ?? ""}`;
  if (cache.has(key)) {
    const hit = cache.get(key);
    cache.delete(key);
    cache.set(key, hit); // most recently used last
    return hit;
  }
  const view = await call("chapter", {
    book,
    chapter,
    options: { red_letter: true, original: true, query: state.highlight },
  });
  cache.set(key, view);
  if (cache.size > CACHE_SIZE) cache.delete(cache.keys().next().value);
  return view;
}

let navSeq = 0;

/**
 * Show `book` `chapter`. With a verse > 0, select it and scroll to it.
 * opts: { highlight, fromPanel, top, keepScroll, select = true }
 */
async function goTo(book, chapter, verse = 0, opts = {}) {
  if (!state.bookMap.has(book)) {
    book = "Genesis";
    chapter = 1;
    verse = 0;
  }
  chapter = Math.min(Math.max(1, chapter), state.bookMap.get(book).chapters);
  if (opts.highlight !== undefined) state.highlight = opts.highlight || null;

  const seq = ++navSeq;
  reader.setAttribute("aria-busy", "true");
  let view;
  try {
    view = await fetchChapter(book, chapter);
  } catch (error) {
    if (seq === navSeq) {
      replace(reader, h("p", { class: "status" }, `Could not open ${heading(book, chapter)}: ${error.message ?? error}`));
      reader.setAttribute("aria-busy", "false");
    }
    return;
  }
  if (seq !== navSeq) return; // a later navigation won

  const changedChapter = state.chapter?.book !== book || state.chapter?.chapter !== chapter;
  const anchor = opts.keepScroll ? firstVisibleVerse() : null;
  state.chapter = view;
  const target = verse > 0 && verse <= view.verses.length ? verse : null;
  state.selectedVerse = opts.select === false ? null : target;
  render();

  if (target && !opts.top) scrollToVerse(target);
  else if (anchor !== null) scrollToVerse(anchor);
  else if (changedChapter || opts.top) reader.scrollTop = 0;

  settings.position = { book, chapter, verse: target ?? 1 };
  if (changedChapter) prefs.addHistory(settings, book, chapter);
  prefs.save(settings);
  reader.setAttribute("aria-busy", "false");

  if (opts.fromPanel && !desktop.matches) closePanel();
}

function render() {
  const view = state.chapter;
  if (!view) return;
  renderChapter(reader, view, {
    view: settings.view,
    selectedVerse: state.selectedVerse,
    nav: {
      prevLabel: view.prev ? heading(view.prev.book, view.prev.chapter) : null,
      nextLabel: view.next ? heading(view.next.book, view.next.chapter) : null,
      onPrev: () => step(-1),
      onNext: () => step(1),
    },
  });
  $("ref-label").textContent = view.heading;
  $("prev-chapter").disabled = !view.prev;
  $("next-chapter").disabled = !view.next;
  document.title = `${view.heading} · KJV Interlinear`;
  updateActions();
}

function step(direction) {
  const target = direction < 0 ? state.chapter?.prev : state.chapter?.next;
  if (target) goTo(target.book, target.chapter, 0, { top: true });
}

function scrollToVerse(n) {
  reader.querySelector(`#v${n}`)?.scrollIntoView({ block: "start" });
}

/** The verse at the top of the reading pane, to keep the place when the layout changes. */
function firstVisibleVerse() {
  const top = reader.getBoundingClientRect().top;
  for (const el of reader.querySelectorAll(".verse")) {
    if (el.getBoundingClientRect().bottom > top + 8) return Number(el.dataset.verse);
  }
  return null;
}

/** Re-render in place (view change), keeping the reader on the same verse. */
function rerender() {
  const anchor = firstVisibleVerse();
  render();
  if (anchor !== null) scrollToVerse(anchor);
}

async function setHighlight(query) {
  if ((query || null) === state.highlight || !state.chapter) {
    state.highlight = query || null;
    return;
  }
  state.highlight = query || null;
  await goTo(state.chapter.book, state.chapter.chapter, state.selectedVerse ?? 0, { keepScroll: true });
}

// ------------------------------------------------------------------ verse selection and actions

function selectVerse(n) {
  state.selectedVerse = n;
  markSelected(reader, n);
  if (n) {
    settings.position = { ...settings.position, verse: n };
    prefs.save(settings);
  }
  updateActions();
}

function updateActions() {
  const n = state.selectedVerse;
  actions.hidden = !n;
  if (!n) return;
  const { book, chapter } = state.chapter;
  $("verse-actions-ref").textContent = reference(book, chapter, n);
  const saved = prefs.isBookmarked(settings, book, chapter, n);
  const bookmark = actions.querySelector('[data-action="bookmark"]');
  bookmark.setAttribute("aria-pressed", String(saved));
  replace(bookmark, icon(saved ? "bookmarkFilled" : "bookmark"), h("span", { class: "action-label" }, saved ? "Saved" : "Bookmark"));
  bookmark.title = saved ? "Remove bookmark (Ctrl+B)" : "Bookmark (Ctrl+B)";
}

async function copyVerse(n = state.selectedVerse) {
  if (!state.chapter || !n) return;
  const { book, chapter } = state.chapter;
  await copy({ book, chapter, verse: n }, `Copied ${reference(book, chapter, n)}`);
}

async function copyChapter() {
  if (!state.chapter) return;
  const { book, chapter } = state.chapter;
  await copy({ book, chapter }, `Copied ${heading(book, chapter)}`);
}

async function copy(args, message) {
  try {
    await copyText(await call("copy_text", args));
    toast(message);
  } catch (error) {
    toast("Could not copy to the clipboard");
    console.error(error);
  }
}

function toggleBookmark() {
  const n = state.selectedVerse;
  if (!state.chapter || !n) return;
  const { book, chapter } = state.chapter;
  const saved = prefs.toggleBookmark(settings, book, chapter, n);
  prefs.save(settings);
  updateActions();
  toast(saved ? `Bookmarked ${reference(book, chapter, n)}` : "Bookmark removed");
  if (state.panel === "saved") refreshPanel();
}

let toastTimer = null;
function toast(message) {
  const el = $("toast");
  el.textContent = message;
  el.classList.add("show");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => el.classList.remove("show"), 1800);
}

// ------------------------------------------------------------------ panels

function openPanel(name, { focus = true } = {}) {
  const wasOpen = state.panel !== null;
  state.panel = name;
  panel.hidden = false;
  app.dataset.panelOpen = "true";
  $("panel-title").textContent = PANELS[name].title;
  const input = PANELS[name].render(panelBody, ctx);
  panelBody.scrollTop = 0;
  syncNavState();
  // On phones the panel covers the reader: let the system back gesture close it
  if (!desktop.matches && !wasOpen) history.pushState({ panel: name }, "");
  if (focus) (input ?? $("panel-close")).focus();
}

function refreshPanel() {
  if (!state.panel) return;
  const scroll = panelBody.scrollTop;
  const active = document.activeElement?.id;
  PANELS[state.panel].render(panelBody, ctx);
  panelBody.scrollTop = scroll;
  if (active) $(active)?.focus();
}

function closePanel({ fromHistory = false } = {}) {
  if (!state.panel) return;
  state.panel = null;
  panel.hidden = true;
  app.dataset.panelOpen = "false";
  syncNavState();
  if (!fromHistory && !desktop.matches && history.state?.panel) history.back();
  reader.focus({ preventScroll: true });
}

function syncNavState() {
  for (const b of document.querySelectorAll("[data-open-panel]")) {
    b.setAttribute("aria-pressed", String(b.dataset.openPanel === state.panel));
  }
  for (const tab of document.querySelectorAll("[data-tab]")) {
    const current = tab.dataset.tab === (state.panel ?? "read") || (tab.dataset.tab === "search" && state.panel === "strongs");
    if (current) tab.setAttribute("aria-current", "page");
    else tab.removeAttribute("aria-current");
  }
}

function openStrongs(key) {
  state.strongs.query = key.replace(/^([HG])0+(?=\d)/, "$1");
  state.strongs.pending = true;
  openPanel("strongs", { focus: desktop.matches });
}

const ctx = {
  state,
  get settings() {
    return settings;
  },
  version: null,
  bookName,
  heading,
  reference,
  goTo,
  setHighlight,
  refreshPanel,
  openPanel,
  toast,
  changeSettings(mutator) {
    const before = settings.view;
    mutator(settings);
    prefs.apply(settings);
    prefs.save(settings);
    if (settings.view !== before) rerender();
    syncViewSwitch();
  },
};

// ------------------------------------------------------------------ view switch

function buildViewSwitches() {
  for (const container of document.querySelectorAll("[data-view-switch]")) {
    replace(
      container,
      prefs.VIEWS.map((v) =>
        h(
          "button",
          {
            type: "button",
            role: "radio",
            "data-view": v.id,
            onclick: () => ctx.changeSettings((s) => {
              s.view = v.id;
            }),
          },
          v.label,
        ),
      ),
    );
  }
  syncViewSwitch();
}

function syncViewSwitch() {
  for (const b of document.querySelectorAll("[data-view-switch] button")) {
    b.setAttribute("aria-checked", String(b.dataset.view === settings.view));
    b.tabIndex = b.dataset.view === settings.view ? 0 : -1;
  }
}

// ------------------------------------------------------------------ input

function isTyping(target) {
  return target instanceof HTMLElement && (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName));
}

function onKeydown(event) {
  const mod = event.ctrlKey || event.metaKey;
  const key = event.key.toLowerCase();

  if (mod && key === "f") {
    event.preventDefault();
    openPanel("search");
    return;
  }
  if (isPickerOpen() || isTyping(event.target)) {
    if (event.key === "Escape" && isTyping(event.target) && state.panel) {
      event.preventDefault();
      closePanel();
    }
    return;
  }
  if (mod && key === "c" && state.selectedVerse && !window.getSelection()?.toString()) {
    event.preventDefault();
    if (event.shiftKey) copyChapter();
    else copyVerse();
    return;
  }
  if (mod && key === "c" && event.shiftKey) {
    event.preventDefault();
    copyChapter();
    return;
  }
  if (mod && key === "b") {
    event.preventDefault();
    toggleBookmark();
    return;
  }
  if (mod || event.altKey) return;

  if (event.key === "ArrowLeft" && !event.target.closest?.("[role=radiogroup]")) {
    event.preventDefault();
    step(-1);
  } else if (event.key === "ArrowRight" && !event.target.closest?.("[role=radiogroup]")) {
    event.preventDefault();
    step(1);
  } else if (event.key === "Escape") {
    if (state.selectedVerse) selectVerse(null);
    else if (state.panel) closePanel();
    else if (state.highlight) setHighlight(null);
  }
}

function onReaderClick(event) {
  const word = event.target.closest(".word");
  if (word?.dataset.key) {
    openStrongs(word.dataset.key);
    return;
  }
  if (event.target.closest("button, a")) return;
  const verse = event.target.closest(".verse");
  if (!verse || verse.classList.contains("is-title")) return;
  // Let people select text without toggling the verse
  if (window.getSelection()?.toString()) return;
  const n = Number(verse.dataset.verse);
  selectVerse(state.selectedVerse === n ? null : n);
}

// ------------------------------------------------------------------ startup

function wireStaticControls() {
  $("prev-chapter").append(icon("chevronLeft"));
  $("next-chapter").append(icon("chevronRight"));
  $("prev-chapter").addEventListener("click", () => step(-1));
  $("next-chapter").addEventListener("click", () => step(1));
  $("ref-button").addEventListener("click", () => {
    if (state.chapter) openPicker(state.chapter.book, state.chapter.chapter);
  });
  $("panel-close").append(icon("close"));
  $("panel-close").addEventListener("click", () => closePanel());

  const toolIcons = { search: "search", saved: "bookmark", settings: "settings" };
  for (const b of document.querySelectorAll("[data-open-panel]")) {
    b.append(icon(toolIcons[b.dataset.openPanel]));
    b.addEventListener("click", () =>
      state.panel === b.dataset.openPanel ? closePanel() : openPanel(b.dataset.openPanel),
    );
  }

  const tabs = { read: ["book", "Read"], search: ["search", "Search"], saved: ["bookmark", "Saved"], settings: ["settings", "Settings"] };
  for (const tab of document.querySelectorAll("[data-tab]")) {
    const [iconName, label] = tabs[tab.dataset.tab];
    tab.append(icon(iconName), h("span", {}, label));
    tab.addEventListener("click", () => (tab.dataset.tab === "read" ? closePanel() : openPanel(tab.dataset.tab, { focus: false })));
  }

  const actionIcons = { "copy-verse": ["copy", "Copy"], "copy-chapter": ["chapter", "Copy chapter"] };
  for (const [action, [iconName, label]] of Object.entries(actionIcons)) {
    const b = actions.querySelector(`[data-action="${action}"]`);
    b.append(icon(iconName), h("span", { class: "action-label" }, label));
    b.title = action === "copy-verse" ? "Copy verse (Ctrl+C)" : "Copy chapter (Ctrl+Shift+C)";
  }
  actions.querySelector('[data-action="deselect"]').append(icon("close"));
  actions.addEventListener("click", (event) => {
    const action = event.target.closest("[data-action]")?.dataset.action;
    if (action === "bookmark") toggleBookmark();
    else if (action === "copy-verse") copyVerse();
    else if (action === "copy-chapter") copyChapter();
    else if (action === "deselect") selectVerse(null);
  });

  reader.addEventListener("click", onReaderClick);
  document.addEventListener("keydown", onKeydown);
  window.addEventListener("popstate", () => {
    if (isPickerOpen()) closePicker();
    else if (state.panel) closePanel({ fromHistory: true });
  });
  desktop.addEventListener("change", syncNavState);
  const persist = () => prefs.flush(settings).catch(() => {});
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "hidden") persist();
  });
  window.addEventListener("pagehide", persist);
}

async function start() {
  wireStaticControls();
  try {
    const [loaded, books, version] = await Promise.all([
      prefs.load(),
      call("books"),
      window.__TAURI__?.app?.getVersion?.().catch(() => null) ?? null,
    ]);
    settings = loaded;
    ctx.version = version;
    state.books = books;
    state.bookMap = new Map(books.map((b) => [b.name, b]));
  } catch (error) {
    replace(reader, h("p", { class: "status" }, `Could not load the Bible text: ${error.message ?? error}`));
    reader.setAttribute("aria-busy", "false");
    return;
  }
  const preview = previewParams();
  if (preview) applyPreviewSettings(preview);
  prefs.apply(settings);
  buildViewSwitches();
  syncNavState();
  initPicker(state.books, (book, chapter) => goTo(book, chapter, 0, { top: true }));
  const p = settings.position;
  if (preview) await applyPreviewState(preview);
  else await goTo(p.book, p.chapter, p.verse > 1 ? p.verse : 0, { select: false });
}

// Browser preview only (never in the app): put the UI in a given state from the URL,
// for screenshots and review. ?book=Psalms&chapter=23&verse=1&select=1&view=parallel
// &theme=dark&scale=1.2&panel=search&q=wept&strongs=H430&picker=1
function previewParams() {
  if (window.__TAURI__ || !location.search) return null;
  return new URLSearchParams(location.search);
}

function applyPreviewSettings(p) {
  const raw = { ...settings };
  if (p.has("view")) raw.view = p.get("view");
  if (p.has("theme")) raw.theme = p.get("theme");
  if (p.has("scale")) raw.textScale = Number(p.get("scale"));
  if (p.has("font")) raw.textFont = p.get("font");
  if (p.has("morph")) raw.morph = p.get("morph") === "1";
  settings = prefs.sanitize(raw);
}

async function applyPreviewState(p) {
  const verse = Number(p.get("verse") ?? 0);
  if (p.has("q")) {
    state.search.query = p.get("q");
    state.highlight = p.get("q");
  }
  await goTo(p.get("book") ?? "Genesis", Number(p.get("chapter") ?? 1), verse, {
    select: p.get("select") === "1",
  });
  if (p.has("strongs")) openStrongs(p.get("strongs"));
  else if (p.has("panel")) openPanel(p.get("panel"), { focus: false });
  if (state.panel === "search" && state.search.query) $("search-input")?.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
  if (p.get("picker") === "1") openPicker(state.chapter.book, state.chapter.chapter);
  if (p.has("pickbook")) {
    openPicker(state.chapter.book, state.chapter.chapter);
    [...document.querySelectorAll(".book-grid button")].find((b) => b.textContent === p.get("pickbook"))?.click();
  }
}

start();
