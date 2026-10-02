// The Cross-references panel: the places the chosen collections (the Treasury,
// OpenBible.info) give for the selected verse, each with its words in the translation
// being read. References are keyed to the KJV; Rust maps the verse and every place.

import { call } from "./backend.js";
import { h, replace } from "./dom.js";

let catalogue = null; // [{ id, name, short, credit, about, books }]
let seq = 0;
let shown = null; // the place the panel shows (see place())
let unlimited = new Set(); // lists shown in full at this place

/** A list gives this many places at first, most helpful first. */
const FIRST = 20;

async function collections() {
  catalogue ??= await call("crossref_collections");
  return catalogue;
}

/** The collections to show: the reader's choice, or all of them. */
function chosen(ctx, all) {
  const picked = ctx.settings.crossrefs;
  return picked ? all.filter((c) => picked.includes(c.id)) : all;
}

function place(ctx) {
  const view = ctx.state.chapter;
  return view ? [ctx.settings.translation, view.book, view.chapter, ctx.state.selectedVerse ?? 0].join("/") : null;
}

/** Whether the reader has moved since the panel was drawn. */
export function xrefsStale(ctx) {
  return place(ctx) !== shown;
}

function chooser(ctx, all) {
  const picked = new Set(chosen(ctx, all).map((c) => c.id));
  return h(
    "div",
    { class: "chips", role: "group", "aria-label": "Cross-references to show" },
    all.map((c) =>
      h(
        "button",
        {
          type: "button",
          class: "chip",
          "aria-pressed": String(picked.has(c.id)),
          title: c.name,
          onclick: () => {
            const next = new Set(picked);
            next.has(c.id) ? next.delete(c.id) : next.add(c.id);
            ctx.changeSettings((s) => {
              s.crossrefs = all.filter((x) => next.has(x.id)).map((x) => x.id);
            });
            ctx.refreshPanel();
          },
        },
        c.short,
      ),
    ),
  );
}

/** One place: where it is (a link that opens it) and its words. */
function passage(p, ctx) {
  const abbr = ctx.state.bibles.find((b) => b.id === ctx.settings.translation)?.abbr ?? "this translation";
  const label = p.book
    ? h(
        "button",
        {
          type: "button",
          class: "text-link note-link xref-ref",
          title: `Open ${p.label}`,
          onclick: () => ctx.goTo(p.book, p.chapter, p.verse, { fromPanel: true }),
        },
        p.label,
      )
    : h("span", { class: "xref-ref" }, p.label);
  const several = p.verses.length > 1;
  const words = p.verses.length
    ? h(
        "p",
        { class: "xref-text" },
        p.verses.map(([n, t], i) => [i ? " " : null, several ? h("sup", { class: "xref-num" }, n) : null, t]),
        p.more ? h("span", { class: "muted" }, " …") : null,
      )
    : null;
  return h(
    "div",
    { class: "xref" },
    h("div", { class: "xref-head" }, label, p.from_kjv ? h("span", { class: "xref-note" }, `KJV · not in ${abbr}`) : null),
    words,
  );
}

function line(l, ctx) {
  // The Treasury's keywords are short ("God."); its remarks run on
  const words = l.text ? h("p", { class: l.text.length > 80 ? "xref-remark" : "xref-words" }, l.text) : null;
  return h("div", { class: "xref-line" }, words, l.refs.map((p) => passage(p, ctx)));
}

function collection(c, ctx, redraw) {
  const given = c.lines.reduce((n, l) => n + l.refs.length, 0);
  return h(
    "section",
    { class: "commentary xref-collection", "data-collection": c.id },
    h("h3", { class: "commentary-name" }, c.name, h("span", { class: "commentary-meta" }, ` · ${c.total} ${c.total === 1 ? "place" : "places"}`)),
    c.lines.map((l) => line(l, ctx)),
    given < c.total
      ? h(
          "button",
          {
            type: "button",
            class: "button xref-all",
            onclick: () => {
              unlimited.add(c.id);
              redraw();
            },
          },
          `Show all ${c.total}`,
        )
      : null,
    h("p", { class: "commentary-credit" }, c.credit),
  );
}

export async function renderXrefs(body, ctx) {
  const view = ctx.state.chapter;
  if (!view) return;
  const verse = ctx.state.selectedVerse ?? 0;
  const here = place(ctx);
  const moved = here !== shown;
  if (moved) unlimited = new Set();
  shown = here;
  const mine = ++seq;
  const where = verse ? ctx.reference(view.book, view.chapter, verse) : ctx.heading(view.book, view.chapter);
  const redraw = () => renderXrefs(body, ctx);
  // References usually arrive at once; only say they're loading when they don't (and
  // not when showing more of the same place, which would lose the reader's scroll)
  const loading = setTimeout(() => {
    if (mine === seq && moved) replace(body, h("p", { class: "status" }, `Loading cross-references for ${where}…`));
  }, 150);
  let all;
  let found = null; // { kjv, same, collections }
  try {
    all = await collections();
    const ids = chosen(ctx, all).map((c) => c.id);
    if (verse && ids.length) {
      // Lists come most helpful first; ask for the rest only when wanted
      const full = ids.filter((id) => unlimited.has(id));
      const short = ids.filter((id) => !unlimited.has(id));
      const args = { bible: ctx.settings.translation, book: view.book, chapter: view.chapter, verse };
      const [a, b] = await Promise.all([
        short.length ? call("crossrefs", { ...args, collections: short, limit: FIRST }) : null,
        full.length ? call("crossrefs", { ...args, collections: full }) : null,
      ]);
      const got = [...(a?.collections ?? []), ...(b?.collections ?? [])];
      found = { ...(a ?? b), collections: ids.map((id) => got.find((c) => c.id === id)) };
    }
  } catch (error) {
    clearTimeout(loading);
    if (mine === seq) replace(body, h("p", { class: "chat-error" }, `Couldn't load the cross-references: ${error.message ?? error}`));
    return;
  }
  clearTimeout(loading);
  if (mine !== seq) return; // the reader moved on
  if (!verse) {
    replace(body, h("p", { class: "notes-where" }, `${where}. Select a verse for its cross-references.`), chooser(ctx, all));
    return;
  }
  const results = found?.collections ?? [];
  let kjv = null;
  if (results.length && !found.same) {
    kjv = found.kjv
      ? h("p", { class: "notes-kjv" }, `The references number it as the KJV does: ${found.kjv}.`)
      : h("p", { class: "notes-kjv" }, "The KJV, whose numbering the references follow, has nothing that matches this verse.");
  }
  const sections = results.filter((c) => c.total).map((c) => collection(c, ctx, redraw));
  const silent = results.filter((c) => !c.total).map((c) => c.short);
  replace(
    body,
    h("p", { class: "notes-where" }, `From ${where}`),
    kjv,
    chooser(ctx, all),
    results.length
      ? [sections, silent.length ? h("div", { class: "notes-silent" }, h("p", { class: "empty" }, `No cross-references here from ${silent.join(" or ")}.`)) : null]
      : h("p", { class: "empty" }, "Choose a collection above."),
  );
}
