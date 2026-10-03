// What the study assistant reads: passages the reader picks (or that follow their
// reading), each in the chosen translations, with commentaries' notes, cross-
// references, and the KJV's Hebrew and Greek. Rust writes the text
// (crates/core/src/context.rs); this is the editor, a dialog, and what turns the
// saved choices into what Rust takes.
//
// A context, as settings keep it:
//   { passages, translations, commentaries, crossrefs, crossrefLimit, crossrefText,
//     original, definitions }
// where a passage is { follow: "verse" | "chapter" | "book" } (wherever the reader
// is), { whole: "bible" | "old" | "new" }, { books: [app book names] }, or
// { bible, refs, label } (refs as Rust's reference::Range::osis writes them, in
// translation `bible`'s numbering), each with optional `translations`,
// `commentaries`, `crossrefs`, and `original` of its own. "reading" in a translation
// list means the translation being read.

import { call, copyText } from "./backend.js";
import { h, icon, plural, replace } from "./dom.js";
import { openTranslations } from "./translations.js";

export const FOLLOW = { verse: "This verse", chapter: "This chapter", book: "This book" };
export const WHOLE = { bible: "The whole Bible", old: "The Old Testament", new: "The New Testament" };
const WHOLE_CHIPS = { bible: "Whole Bible", old: "Old Testament", new: "New Testament" };
const LIMITS = [
  [5, "5"],
  [10, "10"],
  [25, "25"],
  [0, "All"],
];
/** Characters of the preview drawn at once; Copy always takes everything. */
const PREVIEW_CHARS = 200_000;

/** 1,085,845 -> "1.09M", 13,939 -> "14k". Limits round down (never overstate the room). */
export const compact = (n, round = Math.round) =>
  n >= 1e6 ? `${(round(n / 1e4) / 100).toFixed(2).replace(/\.?0+$/, "")}M` : n >= 1e3 ? `${round(n / 1e3)}k` : String(n);
export const compactLimit = (n) => compact(n, Math.floor);

let catalogues = null; // Promise of { commentaries, crossrefs }

/** The commentaries and cross-reference collections, loaded once. */
export function loadCatalogues() {
  catalogues ??= Promise.all([call("commentaries"), call("crossref_collections")])
    .then(([commentaries, crossrefs]) => ({ commentaries, crossrefs }))
    .catch((error) => {
      catalogues = null;
      throw error;
    });
  return catalogues;
}

const shortName = (c) => c.short ?? c.name;

function bible(ctx, id) {
  return ctx.state.bibles.find((b) => b.id === id) ?? null;
}

function codeIn(ctx, id, name) {
  return bible(ctx, id)?.books.find((b) => b.name === name)?.code ?? null;
}

/** "The one you're reading (WEB)", "KJV" */
function translationName(ctx, id) {
  if (id === "reading") return `The one you’re reading (${bible(ctx, ctx.settings.translation)?.abbr ?? "KJV"})`;
  return bible(ctx, id)?.abbr ?? id;
}

/** A passage as Rust takes it ({ bible, refs }), or null where it can't be given now
 * (following the reader before a chapter is open). */
function placeOf(ctx, p) {
  if (p.follow) {
    const view = ctx.state.chapter;
    if (!view) return null;
    const id = ctx.settings.translation;
    const code = codeIn(ctx, id, view.book);
    if (!code) return null;
    if (p.follow === "book") return { bible: id, refs: code };
    if (p.follow === "chapter") return { bible: id, refs: `${code}.${view.chapter}` };
    const verse = ctx.state.selectedVerse || ctx.settings.position.verse || 1;
    return { bible: id, refs: `${code}.${view.chapter}.${verse}` };
  }
  const kjv = bible(ctx, "kjv")?.books ?? [];
  if (p.whole) {
    const books = kjv.filter((b) => (p.whole === "bible" ? b.section !== "apocrypha" : b.section === p.whole));
    return books.length ? { bible: "kjv", refs: books.map((b) => b.code).join(" ") } : null;
  }
  if (p.books) {
    const codes = p.books.map((name) => kjv.find((b) => b.name === name)?.code).filter(Boolean);
    return codes.length ? { bible: "kjv", refs: codes.join(" ") } : null;
  }
  return p.refs ? { bible: p.bible, refs: p.refs } : null;
}

/**
 * The context as Rust takes it (crates/core/src/context.rs, `Spec`), with `index`:
 * for each of its passages, which of `c.passages` it is. Ids no longer in the
 * library are dropped.
 */
export function resolve(ctx, c, known = null) {
  const reading = ctx.settings.translation;
  const translations = (list) => [...new Set(list.map((t) => (t === "reading" ? reading : t)))].filter((t) => bible(ctx, t));
  const keep = (list, all) => (all ? list.filter((id) => all.some((x) => x.id === id)) : list);
  const passages = [];
  const index = [];
  c.passages.forEach((p, i) => {
    const place = placeOf(ctx, p);
    if (!place) return;
    const spec = { ...place };
    if (p.translations) spec.translations = translations(p.translations);
    if (p.commentaries) spec.commentaries = keep(p.commentaries, known?.commentaries);
    if (p.crossrefs) spec.crossrefs = keep(p.crossrefs, known?.crossrefs);
    if (typeof p.original === "boolean") spec.original = p.original;
    passages.push(spec);
    index.push(i);
  });
  const spec = {
    passages,
    translations: translations(c.translations),
    commentaries: keep(c.commentaries, known?.commentaries),
    crossrefs: keep(c.crossrefs, known?.crossrefs),
    crossrefLimit: c.crossrefLimit,
    crossrefText: c.crossrefText,
    original: c.original,
    definitions: c.original && c.definitions,
  };
  return { spec, index };
}

/** The context with every passage fixed where it is now (what a question was asked
 * with, to keep with the conversation), labels from `size` where known. */
export function fixed(ctx, c, known = null, size = null) {
  const { spec, index } = resolve(ctx, c, known);
  const passages = spec.passages.map((p, k) => {
    const original = c.passages[index[k]];
    const out = { bible: p.bible, refs: p.refs, label: size?.passages?.[k]?.label ?? original.label ?? "" };
    for (const key of ["translations", "commentaries", "crossrefs", "original"]) if (key in p) out[key] = p[key];
    return out;
  });
  return { ...structuredClone(c), passages, translations: spec.translations };
}

/** The size of context `c` ({ label, tokens, verses, capped, passages }), keyed so
 * callers can tell whether it changed. */
export async function sizeOf(ctx, c, previous = null) {
  let known = null;
  try {
    known = await loadCatalogues();
  } catch {
    // Sized without checking ids; Rust names any that are unknown
  }
  const { spec, index } = resolve(ctx, c, known);
  const key = JSON.stringify(spec);
  if (previous && previous.key === key && !previous.error) return previous;
  if (!spec.passages.length) return { key, spec, index, label: "", tokens: 0, verses: 0, capped: false, passages: [] };
  const size = await call("context_size", { context: spec });
  return { key, spec, index, ...size };
}

// ------------------------------------------------------------------ the editor

const editor = {
  dialog: null,
  body: null,
  known: null, // the catalogues, once loaded
  opts: null, // { get, set, conversation, budget, onChange }
  size: null,
  sizing: null, // the key being sized
  timer: null,
  expanded: new Set(), // passage indexes whose options are open
  adding: "",
  addError: "",
  naming: null, // the name being typed for a saved context
  preview: null, // { text, tokens } | { loading } | { error }
};

function dialog() {
  if (editor.dialog) return editor.dialog;
  const close = h("button", { class: "icon-btn", type: "button", "aria-label": "Close" }, icon("close"));
  editor.body = h("div", { class: "picker-body context-body" });
  editor.dialog = h(
    "dialog",
    { class: "picker context-dialog", id: "context-editor", "aria-labelledby": "context-title" },
    h("div", { class: "picker-header" }, h("h2", { class: "picker-title", id: "context-title" }, "What the assistant reads"), close),
    editor.body,
  );
  close.addEventListener("click", () => editor.dialog.close());
  editor.dialog.addEventListener("click", (event) => {
    if (event.target === editor.dialog) editor.dialog.close();
  });
  editor.dialog.addEventListener("close", () => {
    clearTimeout(editor.timer);
    editor.preview = null;
    editor.opts?.onChange();
  });
  document.getElementById("app").append(editor.dialog);
  return editor.dialog;
}

/**
 * Open the editor on the context `opts.get()` returns; changes go through
 * `opts.set(mutator)`. `opts.budget(tokens)` gives { limit, fits, reserve } for the
 * model in use (limit null when unknown); `opts.conversation` says the context is a
 * saved conversation's; `opts.onSize(size)` gets each new size, and `opts.onChange()`
 * runs when the dialog closes.
 */
export function openContextEditor(ctx, opts) {
  editor.opts = opts;
  editor.expanded.clear();
  editor.addError = "";
  editor.naming = null;
  editor.preview = null;
  editor.size = null;
  const d = dialog();
  draw(ctx);
  refresh(ctx, 0);
  if (!d.open) d.showModal();
  loadCatalogues().then(
    (known) => {
      editor.known = known;
      if (contextEditorOpen()) draw(ctx);
    },
    () => {},
  );
}

export function contextEditorOpen() {
  return editor.dialog?.open ?? false;
}

/** The reader moved: passages that follow them have changed. */
export function contextEditorMoved(ctx) {
  if (contextEditorOpen()) refresh(ctx);
}

function change(ctx, mutator) {
  editor.opts.set(mutator);
  editor.preview = null;
  draw(ctx);
  refresh(ctx);
}

/** Size the context again, soon (typing and clicking settle first). */
function refresh(ctx, delay = 150) {
  clearTimeout(editor.timer);
  editor.timer = setTimeout(async () => {
    const c = editor.opts.get();
    try {
      const size = await sizeOf(ctx, c, editor.size);
      editor.size = size;
      editor.opts.onSize?.(size);
    } catch (error) {
      editor.size = { error: String(error.message ?? error) };
    }
    if (contextEditorOpen()) draw(ctx);
  }, delay);
}

function section(title, ...children) {
  return h("section", { class: "ctx-section" }, h("h3", { class: "section-title" }, title), children);
}

function toggle(label, hint, on, flip, id) {
  return h(
    "div",
    { class: "setting" },
    h("span", { class: "setting-label", id }, label, hint ? h("span", { class: "setting-hint" }, hint) : null),
    h("button", { class: "switch", type: "button", role: "switch", "aria-checked": String(on), "aria-labelledby": id, onclick: flip }),
  );
}

function draw(ctx) {
  if (!editor.body) return;
  const c = editor.opts.get();
  const scroll = editor.body.scrollTop;
  const focused = document.activeElement?.dataset?.focusKey;
  replace(
    editor.body,
    sizeBar(ctx, c),
    editor.opts.conversation
      ? h("p", { class: "setting-note" }, "This conversation’s context, as its last question was asked. Changes apply to this conversation; a new conversation starts from your usual context.")
      : null,
    section("Passages", passages(ctx, c), addRow(ctx, c), quickRow(ctx, c)),
    section("Translations", translations(ctx, c.translations, (m) => change(ctx, (x) => m(x.translations)))),
    section("Commentaries", commentaries(ctx, c.commentaries, (id) => change(ctx, (x) => flip(x.commentaries, id)))),
    section(
      "Cross-references",
      collections(ctx, c.crossrefs, (id) => change(ctx, (x) => flip(x.crossrefs, id))),
      c.crossrefs.length || c.passages.some((p) => p.crossrefs?.length)
        ? [
            h(
              "div",
              { class: "setting" },
              h("span", { class: "setting-label", id: "ctx-limit-label" }, "Places per verse", h("span", { class: "setting-hint" }, "From OpenBible.info, the most helpful first; from the Treasury, for each word")),
              h(
                "div",
                { class: "segmented", role: "radiogroup", "aria-labelledby": "ctx-limit-label" },
                LIMITS.map(([n, label]) =>
                  h("button", { type: "button", role: "radio", "aria-checked": String(c.crossrefLimit === n), onclick: () => change(ctx, (x) => { x.crossrefLimit = n; }) }, label),
                ),
              ),
            ),
            toggle("Include their words", "Each place’s text, in the passage’s first translation", c.crossrefText, () => change(ctx, (x) => { x.crossrefText = !x.crossrefText; }), "ctx-xref-text"),
          ]
        : null,
    ),
    section(
      "Hebrew and Greek",
      toggle("Original words", "Each verse’s Hebrew or Greek words with Strong’s numbers and glosses, from the KJV’s interlinear. About 3× larger.", c.original, () => change(ctx, (x) => { x.original = !x.original; }), "ctx-original"),
      c.original
        ? toggle("Full Strong’s definitions", "The whole lexicon entry for every Strong’s number, once each. Much larger.", c.definitions, () => change(ctx, (x) => { x.definitions = !x.definitions; }), "ctx-definitions")
        : null,
    ),
    section("Saved contexts", saved(ctx, c)),
    previewSection(ctx, c),
  );
  editor.body.scrollTop = scroll;
  if (focused) editor.body.querySelector(`[data-focus-key="${focused}"]`)?.focus();
}

function flip(list, id) {
  const at = list.indexOf(id);
  if (at >= 0) list.splice(at, 1);
  else list.push(id);
}

function sizeBar(ctx, c) {
  const s = editor.size;
  if (!c.passages.length) return h("p", { class: "ctx-size-line" }, "Nothing attached: the assistant answers from what it knows.");
  if (!s) return h("p", { class: "ctx-size-line muted" }, "Counting…");
  if (s.error) return h("p", { class: "chat-error" }, s.error);
  const { limit, fits, reserve } = editor.opts.budget(s.tokens);
  const fraction = limit ? Math.min(1, s.tokens / Math.max(1, limit - reserve)) : 0;
  const fill = h("span", {});
  fill.style.width = `${(fraction * 100).toFixed(1)}%`;
  return h(
    "div",
    { class: "ctx-size" },
    h(
      "p",
      { class: "ctx-size-line" },
      h("strong", {}, `${s.capped ? "More than " : "≈"}${compact(s.tokens)} tokens`),
      limit ? ` of ${compactLimit(limit)} this model reads` : "",
      h("span", { class: "muted" }, ` · ${plural(s.verses, "verse")}`),
    ),
    limit ? h("div", { class: `meter${fits ? "" : " over"}`, role: "presentation" }, fill) : null,
    fits ? null : h("p", { class: "chat-error small" }, `Too large for this model: ${compact(reserve)} tokens are kept free for the answer. Remove something, or choose a model that reads more.`),
  );
}

function passageLabel(p, size) {
  if (size?.label) return size.label;
  if (p.follow) return FOLLOW[p.follow];
  if (p.whole) return WHOLE[p.whole];
  if (p.books) return p.books.join(", ");
  return p.label || p.refs;
}

function partName(ctx, part, known) {
  if (part.kind === "bible") return bible(ctx, part.id)?.abbr ?? part.id;
  if (part.kind === "original") return "Hebrew/Greek";
  const list = part.kind === "commentary" ? known?.commentaries : known?.crossrefs;
  const c = list?.find((x) => x.id === part.id);
  return c ? shortName(c) : part.id;
}

function passages(ctx, c) {
  if (!c.passages.length) return h("p", { class: "muted small" }, "No passages yet. Add one below, or let it follow your reading.");
  const s = editor.size;
  const known = knownNow();
  return h(
    "ol",
    { class: "ctx-passages" },
    c.passages.map((p, i) => {
      const k = s?.index?.indexOf(i) ?? -1;
      const size = k >= 0 ? s.passages[k] : null;
      const open = editor.expanded.has(i);
      const follow = p.follow ? h("span", { class: "ctx-follow" }, `${FOLLOW[p.follow]}, as you read`) : null;
      return h(
        "li",
        { class: "ctx-passage" },
        h(
          "div",
          { class: "ctx-passage-head" },
          h("span", { class: "ctx-passage-label" }, size?.label ?? (p.follow ? "" : passageLabel(p, size)), follow),
          size ? h("span", { class: "ctx-passage-size" }, `≈${compact(size.tokens)}`) : null,
          h(
            "button",
            {
              type: "button",
              class: "text-button ctx-options-button",
              "aria-expanded": String(open),
              onclick: () => {
                if (open) editor.expanded.delete(i);
                else editor.expanded.add(i);
                draw(ctx);
              },
            },
            own(p) ? "Options •" : "Options",
          ),
          h(
            "button",
            {
              type: "button",
              class: "icon-btn",
              "aria-label": `Remove ${passageLabel(p, size)}`,
              title: "Remove",
              onclick: () => {
                editor.expanded.clear();
                change(ctx, (x) => x.passages.splice(i, 1));
              },
            },
            icon("close"),
          ),
        ),
        size?.problem ? h("p", { class: "chat-error small" }, size.problem) : null,
        size && size.parts.length > 1
          ? h(
              "p",
              { class: "ctx-parts" },
              size.parts.map((part) => `${partName(ctx, part, known)} ${part.empty ? "–" : compact(part.tokens)}`).join(" · "),
            )
          : null,
        open ? options(ctx, p, i) : null,
      );
    }),
  );
}

/** Whether a passage has choices of its own */
function own(p) {
  return ["translations", "commentaries", "crossrefs", "original"].some((k) => k in p);
}

function knownNow() {
  return editor.known ?? null;
}

/** A passage's own choices, each either the same as the rest's or its own. */
function options(ctx, p, i) {
  const set = (mutator) => change(ctx, (x) => mutator(x.passages[i], x));
  const row = (title, key, body) =>
    h(
      "div",
      { class: "ctx-override" },
      h(
        "div",
        { class: "ctx-override-head" },
        h("span", { class: "ctx-override-title" }, title),
        h(
          "div",
          { class: "segmented", role: "radiogroup", "aria-label": title },
          h("button", { type: "button", role: "radio", "aria-checked": String(!(key in p)), onclick: () => set((q) => { delete q[key]; }) }, "Same as the rest"),
          h(
            "button",
            {
              type: "button",
              role: "radio",
              "aria-checked": String(key in p),
              onclick: () =>
                set((q, x) => {
                  if (!(key in q)) q[key] = key === "original" ? !x.original : structuredClone(x[key]);
                }),
            },
            "Its own",
          ),
        ),
      ),
      key in p ? body() : null,
    );
  const count = editor.opts.get().passages.length;
  const move = (by) => {
    editor.expanded.clear();
    editor.expanded.add(i + by);
    change(ctx, (x) => x.passages.splice(i + by, 0, ...x.passages.splice(i, 1)));
  };
  return h(
    "div",
    { class: "ctx-options" },
    count > 1
      ? h(
          "div",
          { class: "ctx-override-head" },
          h("span", { class: "ctx-override-title" }, "Order"),
          h(
            "div",
            { class: "ctx-move" },
            h("button", { type: "button", class: "button", disabled: i === 0, onclick: () => move(-1) }, "Move up"),
            h("button", { type: "button", class: "button", disabled: i === count - 1, onclick: () => move(1) }, "Move down"),
          ),
        )
      : null,
    row("Translations", "translations", () => translations(ctx, p.translations, (m) => set((q) => m(q.translations)))),
    row("Commentaries", "commentaries", () => commentaries(ctx, p.commentaries, (id) => set((q) => flip(q.commentaries, id)))),
    row("Cross-references", "crossrefs", () => collections(ctx, p.crossrefs, (id) => set((q) => flip(q.crossrefs, id)))),
    row("Hebrew and Greek", "original", () =>
      h(
        "div",
        { class: "segmented", role: "radiogroup", "aria-label": "Hebrew and Greek for this passage" },
        h("button", { type: "button", role: "radio", "aria-checked": String(p.original === true), onclick: () => set((q) => { q.original = true; }) }, "Include"),
        h("button", { type: "button", role: "radio", "aria-checked": String(p.original === false), onclick: () => set((q) => { q.original = false; }) }, "Leave out"),
      ),
    ),
  );
}

function addRow(ctx, c) {
  const input = h("input", {
    type: "text",
    placeholder: "Luke 2:14; Rom 5:1-2; Micah 6",
    "aria-label": "Add passages",
    autocomplete: "off",
    spellcheck: "false",
    enterkeyhint: "done",
    "data-focus-key": "add",
    value: editor.adding,
  });
  const add = async () => {
    const text = input.value.trim();
    if (!text) return;
    try {
      const found = await call("context_parse", { text, bible: ctx.settings.translation });
      editor.adding = "";
      editor.addError = "";
      change(ctx, (x) => {
        for (const p of found) x.passages.push({ bible: p.bible, refs: p.refs, label: p.label });
      });
    } catch (error) {
      editor.adding = text;
      editor.addError = String(error.message ?? error);
      draw(ctx);
    }
  };
  input.addEventListener("input", () => {
    editor.adding = input.value;
  });
  input.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && !event.isComposing) {
      event.preventDefault();
      add();
    }
  });
  return [
    h("div", { class: "ctx-add" }, input, h("button", { type: "button", class: "button", onclick: add }, icon("plus"), "Add")),
    editor.addError ? h("p", { class: "chat-error small" }, editor.addError) : null,
  ];
}

function quickRow(ctx, c) {
  const has = (key, value) => c.passages.some((p) => p[key] === value);
  const chip = (label, key, value, hint) =>
    h(
      "button",
      { type: "button", class: "chip", disabled: has(key, value), title: hint, onclick: () => change(ctx, (x) => x.passages.push({ [key]: value })) },
      icon("plus"),
      label,
    );
  return h(
    "div",
    { class: "ctx-quick" },
    h("span", { class: "muted small" }, "Follow your reading:"),
    Object.entries(FOLLOW).map(([k, label]) => chip(label, "follow", k, `${label}, wherever you are reading`)),
    h("span", { class: "muted small" }, "Or:"),
    Object.entries(WHOLE_CHIPS).map(([k, label]) => chip(label, "whole", k, WHOLE[k])),
  );
}

function translations(ctx, list, mutate) {
  const pick = (id) => mutate((l) => { if (!l.includes(id)) l.push(id); });
  return h(
    "div",
    { class: "chips ctx-chips" },
    list.map((id, i) =>
      h(
        "span",
        { class: "chip is-on ctx-chip-removable" },
        translationName(ctx, id),
        h(
          "button",
          { type: "button", class: "ctx-chip-remove", "aria-label": `Remove ${translationName(ctx, id)}`, disabled: list.length === 1, onclick: () => mutate((l) => l.splice(i, 1)) },
          icon("close"),
        ),
      ),
    ),
    list.includes("reading")
      ? null
      : h("button", { type: "button", class: "chip", onclick: () => pick("reading") }, icon("plus"), "The one you’re reading"),
    h(
      "button",
      {
        type: "button",
        class: "chip",
        onclick: () => openTranslations(ctx.state.bibles, null, { pick, title: "Add a translation" }),
      },
      icon("plus"),
      "Add a translation",
    ),
  );
}

function commentaries(ctx, list, flipOne) {
  const known = knownNow();
  if (!known) return h("p", { class: "muted small" }, "Loading…");
  // The Treasury is offered as cross-references (with their words), not as its notes
  const lists = new Set(known.crossrefs.map((x) => x.commentary).filter(Boolean));
  return h(
    "div",
    { class: "chips ctx-chips", role: "group", "aria-label": "Commentaries" },
    known.commentaries.filter((c) => !lists.has(c.id) || list.includes(c.id)).map((c) =>
      h("button", { type: "button", class: "chip", "aria-pressed": String(list.includes(c.id)), title: `${c.name} (${c.year})`, onclick: () => flipOne(c.id) }, shortName(c)),
    ),
  );
}

function collections(ctx, list, flipOne) {
  const known = knownNow();
  if (!known) return h("p", { class: "muted small" }, "Loading…");
  return h(
    "div",
    { class: "chips ctx-chips", role: "group", "aria-label": "Cross-references" },
    known.crossrefs.map((c) =>
      h("button", { type: "button", class: "chip", "aria-pressed": String(list.includes(c.id)), title: c.name, onclick: () => flipOne(c.id) }, c.short ?? c.name),
    ),
  );
}

function saved(ctx, c) {
  const sets = ctx.settings.ai.sets;
  const naming = editor.naming;
  const nameInput = h("input", {
    type: "text",
    placeholder: "Name, like “Advent study”",
    "aria-label": "Name for this context",
    maxlength: "80",
    "data-focus-key": "name",
    value: naming ?? "",
  });
  const save = () => {
    const name = nameInput.value.trim();
    if (!name) return;
    const now = Date.now();
    ctx.changeSettings((s) => {
      const existing = s.ai.sets.find((x) => x.name === name);
      if (existing) existing.context = structuredClone(c);
      else s.ai.sets.push({ id: `s${now.toString(36)}${Math.random().toString(36).slice(2, 6)}`, name, context: structuredClone(c) });
    });
    editor.naming = null;
    draw(ctx);
    ctx.toast(`Saved “${name}”`);
  };
  nameInput.addEventListener("input", () => {
    editor.naming = nameInput.value;
  });
  nameInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && !event.isComposing) {
      event.preventDefault();
      save();
    } else if (event.key === "Escape") {
      event.preventDefault();
      editor.naming = null;
      draw(ctx);
    }
  });
  return [
    sets.length
      ? h(
          "ul",
          { class: "ctx-sets" },
          sets.map((set) =>
            h(
              "li",
              { class: "ctx-set" },
              h("span", { class: "ctx-set-name" }, set.name),
              h(
                "button",
                {
                  type: "button",
                  class: "button",
                  onclick: () => {
                    editor.expanded.clear();
                    change(ctx, (x) => {
                      for (const key of Object.keys(x)) delete x[key];
                      Object.assign(x, structuredClone(set.context));
                    });
                    ctx.toast(`Using “${set.name}”`);
                  },
                },
                "Use",
              ),
              h(
                "button",
                {
                  type: "button",
                  class: "icon-btn",
                  "aria-label": `Delete ${set.name}`,
                  title: "Delete",
                  onclick: () => {
                    ctx.changeSettings((s) => { s.ai.sets = s.ai.sets.filter((x) => x.id !== set.id); });
                    draw(ctx);
                  },
                },
                icon("trash"),
              ),
            ),
          ),
        )
      : h("p", { class: "muted small" }, "Save this context to use it again later."),
    naming === null
      ? h("button", { type: "button", class: "button", onclick: () => { editor.naming = ""; draw(ctx); editor.body.querySelector('[data-focus-key="name"]')?.focus(); } }, "Save this context…")
      : h(
          "div",
          { class: "ctx-add" },
          nameInput,
          h("button", { type: "button", class: "button primary", onclick: save }, "Save"),
          h("button", { type: "button", class: "button", onclick: () => { editor.naming = null; draw(ctx); } }, "Cancel"),
        ),
  ];
}

function previewSection(ctx, c) {
  const p = editor.preview;
  const show = async () => {
    editor.preview = { loading: true };
    draw(ctx);
    try {
      const known = await loadCatalogues().catch(() => null);
      const { spec } = resolve(ctx, c, known);
      const t = await call("context_text", { context: spec });
      editor.preview = { full: t.text ? `${t.instructions}\n\n${t.text}` : t.instructions, tokens: t.tokens };
    } catch (error) {
      editor.preview = { error: String(error.message ?? error) };
    }
    if (contextEditorOpen()) draw(ctx);
  };
  const copy = async () => {
    try {
      await copyText(p.full);
      ctx.toast("Copied what the assistant reads");
    } catch (error) {
      ctx.toast(`Couldn’t copy: ${error.message ?? error}`);
    }
  };
  return section(
    "What’s sent",
    h("p", { class: "setting-note" }, "The instructions and text sent with each question, exactly as the model gets them. Copy them to use with any assistant."),
    !p
      ? h("button", { type: "button", class: "button", onclick: show }, "Show what’s sent")
      : p.loading
        ? h("p", { class: "muted" }, "Writing it out…")
        : p.error
          ? h("p", { class: "chat-error" }, p.error)
          : [
              h(
                "div",
                { class: "ctx-preview-tools" },
                h("span", { class: "muted small" }, `≈${compact(p.tokens)} tokens · ${plural(p.full.length, "character")}`),
                h("button", { type: "button", class: "button", onclick: copy }, icon("copy"), "Copy all"),
                h("button", { type: "button", class: "button", onclick: () => { editor.preview = null; draw(ctx); } }, "Hide"),
              ),
              h("pre", { class: "ctx-preview", tabindex: "0" }, p.full.length > PREVIEW_CHARS ? p.full.slice(0, PREVIEW_CHARS) : p.full),
              p.full.length > PREVIEW_CHARS
                ? h("p", { class: "muted small" }, `Showing the first ${PREVIEW_CHARS.toLocaleString()} characters. Copy all takes everything.`)
                : null,
            ],
  );
}
