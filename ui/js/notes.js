// The Commentary panel: the chosen commentaries' notes on the selected verse (or the
// chapter's introductions), in whatever translation is being read. Notes are keyed to
// the KJV; Rust maps the verse first.

import { call } from "./backend.js";
import { h, replace } from "./dom.js";

let catalogue = null; // [{ id, name, author, year, tradition, coverage, credit, about }]
let seq = 0;
let shown = null; // the place the panel shows (see place())

/** Notes longer than this many characters start folded. */
const FOLD = 2400;

async function commentaries() {
  catalogue ??= await call("commentaries");
  return catalogue;
}

/** The commentaries to show: the reader's choice, or all of them. */
export function chosen(ctx, all) {
  const picked = ctx.settings.commentaries;
  return picked ? all.filter((c) => picked.includes(c.id)) : all;
}

// ------------------------------------------------------------------ note markup

/** USFM code -> the app's book key, from the KJV's book list. */
function bookByCode(ctx) {
  const kjv = ctx.state.bibles.find((b) => b.id === "kjv");
  return new Map((kjv?.books ?? []).map((b) => [b.code, b.name]));
}

function codeOf(ctx, book) {
  for (const [code, name] of bookByCode(ctx)) if (name === book) return code;
  return null;
}

/** "Matthew Henry's Complete Commentary" -> "Matthew Henry" */
function shortName(c) {
  return c.name.replace(/'s (Complete )?Commentary.*$|'s Exposition.*$|'s Notes.*$| Commentary.*$/, "");
}

function list(names) {
  return names.length < 3 ? names.join(" and ") : `${names.slice(0, -1).join(", ")}, and ${names.at(-1)}`;
}

/** Open a reference ("JHN.3.16-JHN.3.18 ROM.5.8": the first place), in the
 * translation being read (notes cite KJV numbering). */
async function follow(ctx, to) {
  const first = to.split(/\s+/)[0].split("-")[0];
  const [code, chapter, verse] = first.split(".");
  const book = bookByCode(ctx).get(code);
  if (!book) return;
  const c = Number(chapter) || 1;
  const v = Number(verse) || 0;
  const translation = ctx.settings.translation;
  if (translation !== "kjv") {
    try {
      const at = await call("bible_map", { from: "kjv", to: translation, book, chapter: c, verse: v });
      if (at) return ctx.goTo(at.book, at.chapter, parseInt(at.verse, 10) || 0, { fromPanel: true });
    } catch {
      // fall through to the same numbers
    }
  }
  ctx.goTo(book, c, v, { fromPanel: true });
}

/** "1JN.4.10" -> "1 John 4:10", "2CO.5.19-2CO.5.21" -> "2 Corinthians 5:19-21" */
function describe(ctx, target) {
  const [first, last] = target.split("-");
  const [code, chapter, verse] = first.split(".");
  const book = bookByCode(ctx).get(code);
  if (!book) return target;
  let text = verse ? ctx.reference(book, Number(chapter), verse) : ctx.heading(book, Number(chapter));
  if (last) {
    const [code2, chapter2, verse2] = last.split(".");
    text += code2 === code && chapter2 === chapter ? `-${verse2}` : `-${chapter2}:${verse2}`;
  }
  return text;
}

function link(ctx, target, label) {
  return h("button", { type: "button", class: "text-link note-link", title: `Open ${describe(ctx, target)}`, onclick: () => follow(ctx, target) }, label);
}

/** "Numb 1:22", "2Chron 35:15", "26:14" */
const PLACE = /(?:(?:[1-3]\s?)?[A-Z][a-z]*\.?\s+)?\d+:\d+(?:-\d+(?::\d+)?)?/g;

/** A reference to one place or several. The Treasury lists several under one
 * reference ("Lu 2:14; Ro 5:8; 1Jo 4:9,10,19"), as Wesley does ("Numb 1:22 26:14");
 * when the text has a part for each place, in order, each part opens its own. */
function references(node, ctx, kids) {
  const targets = node.getAttribute("to").split(/\s+/).filter(Boolean);
  const onlyText = [...node.childNodes].every((n) => n.nodeType === Node.TEXT_NODE);
  if (targets.length > 1 && onlyText) {
    const text = node.textContent;
    // Parts between ";" and ",", any annotation ("*title", "&c.") after them
    const pieces = text.split(/([;,]\s*)/); // part, separator, part, ...
    const placed = pieces.map((t, i) => i % 2 === 0 && /\d/.test(t));
    const count = placed.filter(Boolean).length;
    const lastPlaced = placed.lastIndexOf(true);
    if (count === targets.length && pieces.slice(0, lastPlaced + 1).every((t, i) => i % 2 === 1 || placed[i])) {
      let k = 0;
      return pieces.map((t, i) => (placed[i] ? link(ctx, targets[k++], t) : t));
    }
    // Places separated by spaces
    const found = [...text.matchAll(PLACE)];
    if (found.length === targets.length && /^[\s;,.]*$/.test(text.replace(PLACE, ""))) {
      const out = [];
      let at = 0;
      found.forEach((m, k) => {
        out.push(text.slice(at, m.index), link(ctx, targets[k], m[0]));
        at = m.index + m[0].length;
      });
      out.push(text.slice(at));
      return out;
    }
  }
  return link(ctx, targets[0], kids());
}

/** The library's note markup (docs/LIBRARY.md) as page elements. Only the known
 * elements are drawn; nothing in a note is ever treated as HTML. */
export function renderMarkup(body, ctx) {
  const doc = new DOMParser().parseFromString(`<n>${body}</n>`, "application/xml");
  if (doc.querySelector("parsererror")) {
    // Never expected (the importer checks every note); show the words, not an error
    const words = body.replace(/<[^>]*>/g, " ").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&amp;/g, "&");
    return [h("p", null, words.replace(/\s+/g, " ").trim())];
  }
  const convert = (node) => {
    if (node.nodeType === Node.TEXT_NODE) return node.nodeValue;
    if (node.nodeType !== Node.ELEMENT_NODE) return null;
    const kids = () => [...node.childNodes].map(convert);
    switch (node.nodeName) {
      case "p":
        return h("p", null, kids());
      case "h":
        return h("h4", { class: "note-heading" }, kids());
      case "l":
        return h("div", { class: "note-line" }, kids());
      case "li":
        return h("div", { class: "note-item" }, kids());
      case "tr":
        return h("div", { class: "note-row" }, kids());
      case "td":
        return h("span", { class: "note-cell" }, kids());
      case "i":
        return h("em", null, kids());
      case "b":
        return h("strong", null, kids());
      case "sup":
        return h("sup", null, kids());
      case "sc":
        return h("span", { class: "sc" }, kids());
      case "lang":
        return h("span", { lang: node.getAttribute("code") }, kids());
      case "fn":
        return h("span", { class: "note-fn" }, " [", kids(), "]");
      case "br":
        return h("br");
      case "ref":
        return node.getAttribute("to")?.trim() ? references(node, ctx, kids) : h("span", null, kids());
      default:
        return h("span", null, kids());
    }
  };
  return [...doc.documentElement.childNodes].map(convert);
}

// ------------------------------------------------------------------ panel

function place(ctx) {
  const view = ctx.state.chapter;
  return view ? [ctx.settings.translation, view.book, view.chapter, ctx.state.selectedVerse ?? 0].join("/") : null;
}

/** Whether the reader has moved since the panel was drawn. */
export function notesStale(ctx) {
  return place(ctx) !== shown;
}

function chooser(ctx, all) {
  const picked = new Set(chosen(ctx, all).map((c) => c.id));
  return h(
    "div",
    { class: "chips", role: "group", "aria-label": "Commentaries to show" },
    all.map((c) =>
      h(
        "button",
        {
          type: "button",
          class: "chip",
          "aria-pressed": String(picked.has(c.id)),
          title: `${c.name} (${c.tradition})`,
          onclick: () => {
            const next = new Set(picked);
            next.has(c.id) ? next.delete(c.id) : next.add(c.id);
            ctx.changeSettings((s) => {
              s.commentaries = all.filter((x) => next.has(x.id)).map((x) => x.id);
            });
            ctx.refreshPanel();
          },
        },
        shortName(c),
      ),
    ),
  );
}

function note(n, ctx) {
  const long = n.body.length > FOLD;
  const content = h("div", { class: "note-body" }, renderMarkup(n.body, ctx));
  return long
    ? h("details", { class: "note" }, h("summary", { class: "note-label" }, n.label, h("span", { class: "muted" }, ` · ${Math.round(n.body.length / 1000)}k characters`)), content)
    : h("section", { class: "note" }, h("h4", { class: "note-label" }, n.label), content);
}

export async function renderNotes(body, ctx) {
  const view = ctx.state.chapter;
  if (!view) return;
  const verse = ctx.state.selectedVerse ?? 0;
  const where = verse ? ctx.reference(view.book, view.chapter, verse) : `${ctx.heading(view.book, view.chapter)} (introductions)`;
  const mine = ++seq;
  shown = place(ctx);
  // Notes usually arrive at once; only say they're loading when they don't
  const loading = setTimeout(() => {
    if (mine === seq) replace(body, h("p", { class: "status" }, `Loading notes on ${where}…`));
  }, 150);
  let all;
  let found; // { kjv, same, commentaries }
  try {
    all = await commentaries();
    const ids = chosen(ctx, all).map((c) => c.id);
    found = ids.length
      ? await call("notes", { commentaries: ids, bible: ctx.settings.translation, book: view.book, chapter: view.chapter, verse })
      : { kjv: "", same: true, commentaries: [] };
  } catch (error) {
    clearTimeout(loading);
    if (mine === seq) replace(body, h("p", { class: "chat-error" }, `Couldn't load the notes: ${error.message ?? error}`));
    return;
  }
  clearTimeout(loading);
  if (mine !== seq) return; // the reader moved on
  const results = found.commentaries;
  const sections = results
    .filter((c) => c.notes.length)
    .map((c) =>
      h(
        "section",
        { class: "commentary" },
        h("h3", { class: "commentary-name" }, c.name, h("span", { class: "commentary-meta" }, ` · ${c.author} · ${c.tradition}`)),
        c.notes.map((n) => note(n, ctx)),
        h("p", { class: "commentary-credit" }, c.credit),
      ),
    );
  // The rest in a line each: silent here, or not on this book at all
  const code = codeOf(ctx, view.book);
  const info = new Map(all.map((c) => [c.id, c]));
  const silent = results.filter((c) => !c.notes.length).map((c) => info.get(c.id));
  const elsewhere = silent.filter((c) => code && !c.books.includes(code));
  const quiet = silent.filter((c) => !elsewhere.includes(c));
  const nothing = [];
  if (quiet.length) {
    const what = verse ? "No note on this verse" : "No introduction to this chapter";
    nothing.push(h("p", { class: "empty" }, `${what} from ${list(quiet.map(shortName))}.`));
  }
  if (elsewhere.length) {
    nothing.push(h("p", { class: "empty" }, `Nothing on ${ctx.bookName(view.book)} from ${list(elsewhere.map(shortName))}.`));
  }
  // Every commentary follows the KJV's numbering; say so where this translation's differs
  let kjv = null;
  if (results.length && !found.same) {
    kjv = found.kjv
      ? h("p", { class: "notes-kjv" }, `The commentaries number it as the KJV does: ${found.kjv}.`)
      : h("p", { class: "notes-kjv" }, `The KJV, whose numbering the commentaries follow, has nothing that matches this ${verse ? "verse" : "chapter"}.`);
  }
  replace(
    body,
    h("p", { class: "notes-where" }, verse ? `On ${where}` : `${where}. Select a verse for its notes.`),
    kjv,
    chooser(ctx, all),
    results.length ? [sections, nothing.length ? h("div", { class: "notes-silent" }, nothing) : null] : h("p", { class: "empty" }, "Choose a commentary above."),
  );
}
