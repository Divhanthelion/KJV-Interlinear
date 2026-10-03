// The Search panel: the translation being read, or chosen translations and
// commentaries, or everything. Each source is searched on its own
// (crates/core/src/search.rs) and its results shown as they arrive.

import { call } from "./backend.js";
import { loadCatalogues } from "./context.js";
import { h, plural, replace } from "./dom.js";
import { openNote } from "./notes.js";

/** Results shown for each source when searching several (all of them on request). */
const SOME = 20;
const ALL = 500;
/** Sources searched at once (each also reads its books on every core). */
const AT_ONCE = 3;

let seq = 0;
let timer = null;
// Draws the results in the panel on screen (a search outlives a redraw of the panel)
let redraw = () => {};
let known = null; // { commentaries, crossrefs }

const shortName = (c) => c.short ?? c.name;

/** The sources to search, in order: the translation being read first. */
function sources(ctx) {
  const s = ctx.settings.search;
  const reading = ctx.settings.translation;
  const bible = (id) => ctx.state.bibles.find((b) => b.id === id);
  const label = (id) => bible(id)?.abbr ?? id;
  if (s.in === "reading" || !known) return [{ kind: "bible", id: reading, short: label(reading), name: bible(reading)?.name ?? reading }];
  const everything = s.in === "everything";
  const ids = everything ? ctx.state.bibles.map((b) => b.id) : ctx.state.bibles.map((b) => b.id).filter((id) => s.translations.includes(id));
  const ordered = ids.includes(reading) ? [reading, ...ids.filter((id) => id !== reading)] : ids;
  const out = ordered.map((id) => ({ kind: "bible", id, short: label(id), name: bible(id)?.name ?? id }));
  for (const c of known.commentaries) {
    if (everything || s.commentaries.includes(c.id)) out.push({ kind: "commentary", id: c.id, short: shortName(c), name: c.name });
  }
  return out;
}

function hitSegments(nodes) {
  return nodes.map((s) => (s.hit ? h("mark", { class: "hit" }, s.text) : s.text));
}

function open(ctx, group, hit, query) {
  if (group.kind === "commentary") {
    openNote(ctx, { id: group.id, label: hit.reference, book: hit.book, chapter: hit.chapter, verse: hit.verse });
  } else {
    ctx.openIn(group.id, hit.book, hit.chapter, hit.verse, { highlight: query, fromPanel: true });
  }
}

function hits(ctx, group, query) {
  return h(
    "ul",
    { class: "result-list" },
    group.data.hits.map((hit) =>
      h(
        "li",
        {},
        h(
          "button",
          { class: `result${group.kind === "commentary" ? " is-note" : ""}`, type: "button", onclick: () => open(ctx, group, hit, query) },
          h("span", { class: "result-ref" }, hit.reference),
          h("span", { class: "result-text" }, hitSegments(hit.segments)),
        ),
      ),
    ),
  );
}

/** "verse", "note" */
const unit = (group) => (group.kind === "commentary" ? "note" : "verse");

export function renderSearch(body, ctx) {
  const s = ctx.state.search;
  const where = ctx.settings.search;
  const reading = ctx.state.bibles.find((b) => b.id === ctx.settings.translation);
  const several = where.in !== "reading";
  const label = where.in === "everything" ? "Search everything" : several ? "Search the chosen sources" : `Search the ${reading?.abbr ?? "KJV"}`;
  const input = h("input", {
    type: "search",
    id: "search-input",
    value: s.query,
    placeholder: label,
    "aria-label": label,
    autocomplete: "off",
    autocapitalize: "off",
    spellcheck: "false",
    enterkeyhint: "search",
  });
  const results = h("div", { class: "search-results", "aria-live": "polite" });
  const choices = h("div", { class: "search-choices" });

  const run = async () => {
    const query = input.value.trim();
    s.query = input.value;
    const mine = ++seq;
    if (!query) {
      s.results = null;
      replace(results);
      ctx.setHighlight(null);
      return;
    }
    if (ctx.settings.search.in !== "reading" && !known) {
      try {
        known = await loadCatalogues();
      } catch {
        // searched as the translation being read
      }
    }
    const list = sources(ctx);
    const many = list.length > 1;
    const book = s.scope === "book" ? ctx.state.chapter?.book : null;
    s.results = { query, many, groups: list.map((g) => ({ ...g, status: "pending" })) };
    drawResults();
    let next = 0;
    const worker = async () => {
      while (next < s.results.groups.length && mine === seq) {
        const g = s.results.groups[next++];
        try {
          g.data = await call("search_source", { query, kind: g.kind, source: g.id, scope: s.scope, book, limit: many ? SOME : ALL });
          g.status = "done";
        } catch (error) {
          g.status = "error";
          g.error = String(error.message ?? error);
        }
        if (mine === seq) redraw();
      }
    };
    await Promise.all(Array.from({ length: Math.min(AT_ONCE, list.length) }, worker));
    if (mine === seq) ctx.setHighlight(query);
  };

  /** All of one source's results, in place of its first few. */
  const showAll = async (g) => {
    const r = s.results;
    g.loadingAll = true;
    redraw();
    try {
      const book = s.scope === "book" ? ctx.state.chapter?.book : null;
      g.data = await call("search_source", { query: r.query, kind: g.kind, source: g.id, scope: s.scope, book, limit: ALL });
    } catch (error) {
      ctx.toast(String(error.message ?? error));
    }
    g.loadingAll = false;
    if (s.results === r) redraw();
  };

  const drawResults = () => {
    const r = s.results;
    if (!r) return replace(results);
    if (!r.many) {
      const g = r.groups[0];
      if (g.status === "pending") return replace(results, h("p", { class: "search-status muted" }, "Searching…"));
      if (g.status === "error") return replace(results, h("p", { class: "empty" }, `Search failed: ${g.error}`));
      if (g.data.total === 0) return replace(results, h("p", { class: "empty" }, `No verses in the ${g.short} contain “${r.query}”.`));
      return replace(
        results,
        h(
          "p",
          { class: "result-summary" },
          g.data.total > g.data.hits.length
            ? `${plural(g.data.total, "verse")} · showing the first ${g.data.hits.length.toLocaleString()}`
            : plural(g.data.total, "verse"),
        ),
        hits(ctx, g, r.query),
      );
    }
    const done = r.groups.filter((g) => g.status !== "pending");
    const found = done.filter((g) => g.status === "done" && g.data.total > 0);
    const none = done.filter((g) => g.status === "done" && g.data.total === 0);
    const failed = done.filter((g) => g.status === "error");
    const total = found.reduce((n, g) => n + g.data.total, 0);
    const pending = r.groups.length - done.length;
    const sections = r.groups.filter((g) => g.status === "done" && g.data.total > 0);
    replace(
      results,
      h(
        "p",
        { class: "result-summary" },
        `${plural(total, "result")} in ${plural(found.length, "source")}`,
        pending ? h("span", { class: "muted" }, ` · searching ${pending} more…`) : null,
      ),
      found.length > 1
        ? h(
            "div",
            { class: "chips search-jump", role: "navigation", "aria-label": "Jump to a source" },
            found.map((g) =>
              h(
                "button",
                {
                  type: "button",
                  class: "chip",
                  onclick: () => results.querySelector(`[data-group="${g.kind}:${CSS.escape(g.id)}"]`)?.scrollIntoView({ block: "start" }),
                },
                g.short,
                h("span", { class: "search-count" }, ` ${g.data.total.toLocaleString()}`),
              ),
            ),
          )
        : null,
      sections.map((g) =>
        h(
          "section",
          { class: "search-group", "data-group": `${g.kind}:${g.id}` },
          h(
            "h3",
            { class: "search-group-name" },
            g.short,
            g.short !== g.name ? h("span", { class: "muted" }, ` · ${g.name}`) : null,
            h("span", { class: "search-count" }, ` · ${plural(g.data.total, unit(g))}`),
          ),
          hits(ctx, g, r.query),
          g.data.total > g.data.hits.length
            ? h(
                "button",
                { type: "button", class: "text-button search-more", disabled: !!g.loadingAll, onclick: () => showAll(g) },
                g.loadingAll ? "Loading…" : g.data.total > ALL ? `Show the first ${ALL}` : `Show all ${g.data.total.toLocaleString()}`,
              )
            : null,
        ),
      ),
      none.length ? h("p", { class: "empty search-none" }, `Nothing in ${none.map((g) => g.short).join(", ")}.`) : null,
      failed.length ? failed.map((g) => h("p", { class: "chat-error small" }, `${g.short}: ${g.error}`)) : null,
      !pending && !found.length ? h("p", { class: "empty" }, `Nothing contains “${r.query}”.`) : null,
    );
  };

  /** Where to look, and (for chosen sources) which. */
  const drawChoices = () => {
    const w = ctx.settings.search;
    const set = (mutator) => {
      ctx.changeSettings((x) => mutator(x.search));
      ctx.refreshPanel();
      if (input.value.trim()) run();
    };
    const toggle = (key, id) =>
      set((x) => {
        x[key] = x[key].includes(id) ? x[key].filter((y) => y !== id) : [...x[key], id];
      });
    replace(
      choices,
      segmented(
        "Search in",
        [
          ["reading", reading?.abbr ?? "KJV"],
          ["chosen", "Choose…"],
          ["everything", "Everything"],
        ],
        w.in,
        (id) => set((x) => { x.in = id; }),
      ),
      w.in === "chosen"
        ? h(
            "div",
            { class: "search-sources" },
            h("p", { class: "field-label" }, "Translations"),
            h(
              "div",
              { class: "chips", role: "group", "aria-label": "Translations to search" },
              ctx.state.bibles.map((b) =>
                h("button", { type: "button", class: "chip", "aria-pressed": String(w.translations.includes(b.id)), title: b.name, onclick: () => toggle("translations", b.id) }, b.abbr),
              ),
            ),
            h("p", { class: "field-label" }, "Commentaries"),
            known
              ? h(
                  "div",
                  { class: "chips", role: "group", "aria-label": "Commentaries to search" },
                  known.commentaries.map((c) =>
                    h("button", { type: "button", class: "chip", "aria-pressed": String(w.commentaries.includes(c.id)), title: c.name, onclick: () => toggle("commentaries", c.id) }, shortName(c)),
                  ),
                )
              : h("p", { class: "muted small" }, "Loading…"),
            !w.translations.length && !w.commentaries.length ? h("p", { class: "muted small" }, "Choose what to search.") : null,
          )
        : null,
    );
  };

  input.addEventListener("input", () => {
    clearTimeout(timer);
    timer = setTimeout(run, 250);
  });
  input.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      clearTimeout(timer);
      run();
    }
  });

  const bookName = ctx.state.chapter ? ctx.bookName(ctx.state.chapter.book) : "This book";
  const scope = segmented(
    "Which books",
    [
      ["all", "All"],
      ["old", "Old Test."],
      ["new", "New Test."],
      ["book", bookName],
    ],
    s.scope,
    (id) => {
      s.scope = id;
      ctx.refreshPanel();
      if (input.value.trim()) run();
    },
  );

  if (ctx.settings.search.in !== "reading" && !known) {
    loadCatalogues().then((k) => {
      known = k;
      drawChoices();
    }, () => {});
  }
  drawChoices();
  redraw = drawResults;
  replace(body, h("div", { class: "field" }, input), h("div", { class: "field" }, choices), h("div", { class: "field" }, scope), results);
  drawResults();
  return input;
}

function segmented(label, options, value, onChange) {
  return h(
    "div",
    { class: "segmented", role: "radiogroup", "aria-label": label },
    options.map(([id, text]) => h("button", { type: "button", role: "radio", "aria-checked": String(id === value), onclick: () => onChange(id) }, text)),
  );
}
