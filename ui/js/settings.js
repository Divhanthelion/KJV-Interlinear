// User settings: defaults, validation of whatever was saved, and applying them to the page.

import { loadSettings, saveSettings } from "./backend.js";

export const VIEWS = [
  { id: "kjv", label: "KJV" },
  { id: "parallel", label: "Parallel" },
  { id: "interlinear", label: "Interlinear" },
  { id: "original", label: "Original" },
];

export const TEXT_SCALES = [0.85, 0.92, 1, 1.1, 1.2, 1.35, 1.5, 1.7];
export const ORIG_SCALES = [1, 1.15, 1.3, 1.45, 1.6];

export const DEFAULTS = {
  theme: "system", // system | light | dark
  textScale: 1,
  textFont: "serif", // serif | sans
  view: "kjv",
  verseNumbers: true,
  redLetter: true,
  translit: true,
  strongs: true,
  morph: false,
  origScale: 1.3,
  position: { book: "Genesis", chapter: 1, verse: 1 },
  bookmarks: [], // { book, chapter, verse, created }
  history: [], // { book, chapter, time }
  ai: {
    providers: [], // { id, preset, name, kind, baseUrl, contextWindow }
    providerId: null,
    model: null,
    scope: "chapter", // none | verse | chapter | book | books | bible
    books: [], // for scope "books"
    original: false, // attach Hebrew/Greek words
    think: true, // let a local reasoning model think before answering
    // Providers the reader agreed to send questions to (keyed by id)
    consent: {},
    // Real ÷ estimated prompt tokens, per "provider|model", learned from replies
    calibration: {},
  },
};

const HISTORY_LIMIT = 50;
export const AI_KINDS = ["openai", "anthropic", "gemini"];
export const AI_SCOPES = ["none", "verse", "chapter", "book", "books", "bible"];

const isObject = (v) => v !== null && typeof v === "object" && !Array.isArray(v);
const isRef = (v) =>
  isObject(v) && typeof v.book === "string" && Number.isInteger(v.chapter) && v.chapter > 0;

function oneOf(value, allowed, fallback) {
  return allowed.includes(value) ? value : fallback;
}

function sanitizeAi(raw) {
  const a = isObject(raw) ? raw : {};
  const text = (v, max = 500) => (typeof v === "string" ? v.slice(0, max) : "");
  const providers = (Array.isArray(a.providers) ? a.providers : [])
    .filter((p) => isObject(p) && typeof p.id === "string" && p.id && AI_KINDS.includes(p.kind))
    .map((p) => ({
      id: p.id,
      preset: text(p.preset, 40) || "custom",
      name: text(p.name, 80) || "AI provider",
      kind: p.kind,
      baseUrl: text(p.baseUrl),
      contextWindow: Number.isInteger(p.contextWindow) && p.contextWindow > 0 ? p.contextWindow : null,
    }));
  const ids = new Set(providers.map((p) => p.id));
  const flags = (v) =>
    Object.fromEntries(Object.entries(isObject(v) ? v : {}).filter(([k, x]) => ids.has(k) && x === true));
  return {
    providers,
    providerId: ids.has(a.providerId) ? a.providerId : (providers[0]?.id ?? null),
    model: typeof a.model === "string" && a.model ? a.model.slice(0, 200) : null,
    scope: oneOf(a.scope, AI_SCOPES, DEFAULTS.ai.scope),
    books: (Array.isArray(a.books) ? a.books : []).filter((b) => typeof b === "string").slice(0, 66),
    original: typeof a.original === "boolean" ? a.original : false,
    think: typeof a.think === "boolean" ? a.think : true,
    consent: flags(a.consent),
    calibration: Object.fromEntries(
      Object.entries(isObject(a.calibration) ? a.calibration : {})
        .filter(([, v]) => Number.isFinite(v) && v > 0.3 && v < 4)
        .slice(0, 100),
    ),
  };
}

/** Merge saved settings over the defaults, dropping anything malformed. */
export function sanitize(raw) {
  const s = isObject(raw) ? raw : {};
  const bool = (key) => (typeof s[key] === "boolean" ? s[key] : DEFAULTS[key]);
  const position = isRef(s.position)
    ? {
        book: s.position.book,
        chapter: s.position.chapter,
        verse: Number.isInteger(s.position.verse) && s.position.verse >= 0 ? s.position.verse : 1,
      }
    : { ...DEFAULTS.position };
  return {
    theme: oneOf(s.theme, ["system", "light", "dark"], DEFAULTS.theme),
    textScale: oneOf(s.textScale, TEXT_SCALES, DEFAULTS.textScale),
    textFont: oneOf(s.textFont, ["serif", "sans"], DEFAULTS.textFont),
    view: oneOf(s.view, VIEWS.map((v) => v.id), DEFAULTS.view),
    verseNumbers: bool("verseNumbers"),
    redLetter: bool("redLetter"),
    translit: bool("translit"),
    strongs: bool("strongs"),
    morph: bool("morph"),
    origScale: oneOf(s.origScale, ORIG_SCALES, DEFAULTS.origScale),
    position,
    bookmarks: (Array.isArray(s.bookmarks) ? s.bookmarks : [])
      .filter((b) => isRef(b) && Number.isInteger(b.verse) && b.verse > 0)
      .map((b) => ({
        book: b.book,
        chapter: b.chapter,
        verse: b.verse,
        created: Number.isFinite(b.created) ? b.created : Date.now(),
      })),
    history: (Array.isArray(s.history) ? s.history : [])
      .filter(isRef)
      .slice(0, HISTORY_LIMIT)
      .map((h) => ({ book: h.book, chapter: h.chapter, time: Number.isFinite(h.time) ? h.time : Date.now() })),
    ai: sanitizeAi(s.ai),
  };
}

// Nothing is written until the saved settings have been read: saving the defaults
// before then (the window closing early) or after a failed read would replace the
// reader's file. "No settings yet" (null) is a successful read; an error is not.
let loaded = false;

/** The saved settings, or the defaults on first run or when they can't be read. */
export async function load() {
  let raw;
  try {
    raw = await loadSettings();
  } catch (error) {
    console.error("Could not load settings; changes won't be saved", error);
    return sanitize(null);
  }
  loaded = true;
  return sanitize(raw);
}

/** False until settings have been read successfully (saving is off until then). */
export function isLoaded() {
  return loaded;
}

let saveTimer = null;

/** Save soon; repeated changes within half a second are written once. */
export function save(settings) {
  if (!loaded) return;
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    saveSettings(settings).catch((error) => console.error("Could not save settings", error));
  }, 500);
}

/** Write immediately (e.g. when the window is closing). */
export function flush(settings) {
  if (!loaded) return Promise.resolve();
  clearTimeout(saveTimer);
  return saveSettings(settings);
}

/** Reflect settings in CSS: theme, sizes, and which parts of the text show. */
export function apply(settings) {
  const root = document.documentElement;
  if (settings.theme === "system") delete root.dataset.theme;
  else root.dataset.theme = settings.theme;
  root.dataset.textFont = settings.textFont;
  root.style.setProperty("--text-scale", settings.textScale);
  root.style.setProperty("--orig-scale", settings.origScale);

  const app = document.getElementById("app");
  app.dataset.view = settings.view;
  app.dataset.verseNumbers = settings.verseNumbers;
  app.dataset.redLetter = settings.redLetter;
  app.dataset.translit = settings.translit;
  app.dataset.strongs = settings.strongs;
  app.dataset.morph = settings.morph;
}

export function addHistory(settings, book, chapter) {
  settings.history = [
    { book, chapter, time: Date.now() },
    ...settings.history.filter((h) => !(h.book === book && h.chapter === chapter)),
  ].slice(0, HISTORY_LIMIT);
}

export function isBookmarked(settings, book, chapter, verse) {
  return settings.bookmarks.some((b) => b.book === book && b.chapter === chapter && b.verse === verse);
}

/** Add or remove a bookmark; returns true if the verse is now bookmarked. */
export function toggleBookmark(settings, book, chapter, verse) {
  if (isBookmarked(settings, book, chapter, verse)) {
    settings.bookmarks = settings.bookmarks.filter(
      (b) => !(b.book === book && b.chapter === chapter && b.verse === verse),
    );
    return false;
  }
  settings.bookmarks = [{ book, chapter, verse, created: Date.now() }, ...settings.bookmarks];
  return true;
}
