// Side panels (docked on desktop, full screen on phones): search, Strong's, saved, settings.

import { call, openExternal } from "./backend.js";
import { h, icon, plural, replace, timeAgo } from "./dom.js";
import { renderAiSettings } from "./chat.js";
import { APP } from "./brand.js";
import { openLicences } from "./licences.js";
import { ORIG_SCALES, TEXT_SCALES } from "./settings.js";

/**
 * ctx = {
 *   state, settings, bookName(book) -> display name, reference(book, ch, v),
 *   goTo(book, chapter, verse, { highlight }), changeSettings(mutator), toast(msg),
 *   openPanel(name), refreshPanel()
 * }
 */

// ------------------------------------------------------------------ shared

function segments(nodes) {
  return nodes.map((s) => (s.hit ? h("mark", { class: "hit" }, s.text) : s.text));
}

function hitList(hits, ctx, highlight) {
  return h(
    "ul",
    { class: "result-list" },
    hits.map((hit) =>
      h(
        "li",
        {},
        h(
          "button",
          {
            class: "result",
            type: "button",
            onclick: () => ctx.goTo(hit.book, hit.chapter, hit.verse, { highlight, fromPanel: true }),
          },
          h("span", { class: "result-ref" }, hit.reference),
          h("span", { class: "result-text" }, segments(hit.segments)),
        ),
      ),
    ),
  );
}

function segmented(label, options, value, onChange) {
  return h(
    "div",
    { class: "segmented", role: "radiogroup", "aria-label": label },
    options.map(([id, text]) =>
      h(
        "button",
        {
          type: "button",
          role: "radio",
          "aria-checked": String(id === value),
          onclick: () => onChange(id),
        },
        text,
      ),
    ),
  );
}

// ------------------------------------------------------------------ Strong's and lexicon

let strongsSeq = 0;

export function renderStrongs(body, ctx) {
  const st = ctx.state.strongs;
  const input = h("input", {
    type: "search",
    id: "strongs-input",
    value: st.query,
    placeholder: "H430 or G2316",
    "aria-label": "Strong's number",
    autocomplete: "off",
    autocapitalize: "characters",
    spellcheck: "false",
    enterkeyhint: "search",
  });
  const output = h("div", { "aria-live": "polite" });

  const run = async () => {
    st.query = input.value;
    const query = input.value.trim();
    const seq = ++strongsSeq;
    if (!query) {
      st.results = null;
      return draw();
    }
    try {
      const r = await call("strongs", { query });
      if (seq !== strongsSeq) return;
      st.results = r;
      draw();
    } catch (error) {
      replace(output, h("p", { class: "empty" }, `Lookup failed: ${error.message ?? error}`));
    }
  };

  const draw = () => {
    const r = st.results;
    if (!r) return replace(output, h("p", { class: "empty" }, "Tap a word in Interlinear view, or enter a Strong's number."));
    if (!r.key) return replace(output, h("p", { class: "empty" }, "Enter a number like H430 (Hebrew) or G2316 (Greek)."));
    const lex = r.lexicon;
    replace(
      output,
      lex
        ? [
            h(
              "div",
              { class: "lexicon-head" },
              h("span", { class: "lexicon-word", lang: lex.lang, dir: lex.lang === "he" ? "rtl" : "ltr" }, lex.word),
              h("span", { class: "lexicon-translit" }, lex.translit),
            ),
            h("p", { class: "lexicon-meta" }, [lex.strongs, lex.morph].filter(Boolean).join(" · ")),
            h("p", { class: "lexicon-gloss" }, lex.gloss),
            h("p", { class: "lexicon-def", dir: "auto" }, lex.definition),
          ]
        : h("p", { class: "empty" }, `No lexicon entry for ${r.strongs}.`),
      h("h3", { class: "section-title" }, r.total ? `${plural(r.total, "verse")} with ${r.strongs}` : `No verses with ${r.strongs}`),
      r.total > r.hits.length
        ? h("p", { class: "result-summary" }, `Showing the first ${r.hits.length.toLocaleString()}`)
        : null,
      hitList(r.hits, ctx, null),
    );
  };

  input.addEventListener("keydown", (event) => {
    if (event.key === "Enter") run();
  });
  input.addEventListener("change", run);

  replace(body, h("div", { class: "field" }, input), output);
  draw();
  if (st.pending) {
    st.pending = false;
    run();
  }
  return input;
}

// ------------------------------------------------------------------ saved

export function renderSaved(body, ctx) {
  const settings = ctx.settings;
  const bookmarks = settings.bookmarks.length
    ? h(
        "ul",
        { class: "result-list" },
        settings.bookmarks.map((b) =>
          h(
            "li",
            { class: "bookmark-row" },
            h(
              "button",
              {
                class: "row-button",
                type: "button",
                onclick: () => ctx.goTo(b.book, b.chapter, b.verse, { fromPanel: true }),
              },
              h(
                "span",
                { class: "grow" },
                h("span", { class: "row-main" }, ctx.reference(b.book, b.chapter, b.verse)),
                h("span", { class: "row-sub" }, `Saved ${timeAgo(b.created)}`),
              ),
            ),
            h(
              "button",
              {
                class: "icon-btn",
                type: "button",
                "aria-label": `Remove bookmark ${ctx.reference(b.book, b.chapter, b.verse)}`,
                onclick: () => {
                  ctx.changeSettings((s) => {
                    s.bookmarks = s.bookmarks.filter((x) => x !== b);
                  });
                  ctx.refreshPanel();
                },
              },
              icon("trash"),
            ),
          ),
        ),
      )
    : h("p", { class: "empty" }, "No bookmarks yet. Select a verse and choose Bookmark.");

  const history = settings.history.length
    ? [
        h(
          "ul",
          { class: "result-list" },
          settings.history.map((entry) =>
            h(
              "li",
              {},
              h(
                "button",
                {
                  class: "row-button",
                  type: "button",
                  onclick: () => ctx.goTo(entry.book, entry.chapter, 1, { fromPanel: true, top: true }),
                },
                h(
                  "span",
                  { class: "grow" },
                  h("span", { class: "row-main" }, ctx.heading(entry.book, entry.chapter)),
                  h("span", { class: "row-sub" }, timeAgo(entry.time)),
                ),
              ),
            ),
          ),
        ),
        h(
          "button",
          {
            class: "text-button",
            type: "button",
            onclick: () => {
              ctx.changeSettings((s) => {
                s.history = [];
              });
              ctx.refreshPanel();
            },
          },
          "Clear history",
        ),
      ]
    : h("p", { class: "empty" }, "Chapters you read appear here.");

  replace(
    body,
    h("h3", { class: "section-title" }, "Bookmarks"),
    bookmarks,
    h("h3", { class: "section-title" }, "Recently read"),
    history,
  );
}

// ------------------------------------------------------------------ settings

function switchRow(label, hint, value, onChange) {
  const id = `set-${label.replace(/\W+/g, "-").toLowerCase()}`;
  return h(
    "div",
    { class: "setting" },
    h(
      "span",
      { class: "setting-label", id },
      label,
      hint ? h("span", { class: "setting-hint" }, hint) : null,
    ),
    h("button", {
      class: "switch",
      type: "button",
      role: "switch",
      "aria-checked": String(value),
      "aria-labelledby": id,
      onclick: () => onChange(!value),
    }),
  );
}

function stepperRow(label, values, value, format, onChange) {
  const i = values.indexOf(value);
  return h(
    "div",
    { class: "setting" },
    h("span", { class: "setting-label" }, label),
    h(
      "div",
      { class: "stepper" },
      h("button", { type: "button", "aria-label": `Smaller ${label.toLowerCase()}`, disabled: i <= 0, onclick: () => onChange(values[i - 1]) }, "−"),
      h("output", { "aria-live": "polite" }, format(value)),
      h("button", { type: "button", "aria-label": `Larger ${label.toLowerCase()}`, disabled: i >= values.length - 1, onclick: () => onChange(values[i + 1]) }, "+"),
    ),
  );
}


export function renderSettings(body, ctx) {
  const s = ctx.settings;
  const set = (mutator) => {
    ctx.changeSettings(mutator);
    ctx.refreshPanel();
  };
  const percent = (v) => `${Math.round(v * 100)}%`;

  replace(
    body,
    h("h3", { class: "section-title" }, "Appearance"),
    h("div", { class: "field" }, h("span", { class: "field-label" }, "Theme"),
      segmented("Theme", [["system", "Automatic"], ["light", "Light"], ["dark", "Dark"]], s.theme, (v) => set((x) => { x.theme = v; }))),
    h("div", { class: "field" }, h("span", { class: "field-label" }, "Text font"),
      segmented("Text font", [["serif", "Serif"], ["sans", "Sans serif"]], s.textFont, (v) => set((x) => { x.textFont = v; }))),
    stepperRow("Text size", TEXT_SCALES, s.textScale, percent, (v) => set((x) => { x.textScale = v; })),
    h("h3", { class: "section-title" }, "Reading"),
    switchRow("Verse numbers", null, s.verseNumbers, (v) => set((x) => { x.verseNumbers = v; })),
    switchRow("Red letter", "Words of Christ in red", s.redLetter, (v) => set((x) => { x.redLetter = v; })),
    h("h3", { class: "section-title" }, "Hebrew & Greek"),
    stepperRow("Hebrew & Greek size", ORIG_SCALES, s.origScale, percent, (v) => set((x) => { x.origScale = v; })),
    switchRow("Transliteration", null, s.translit, (v) => set((x) => { x.translit = v; })),
    switchRow("Strong's numbers", null, s.strongs, (v) => set((x) => { x.strongs = v; })),
    switchRow("Grammar codes", "Morphology under each word", s.morph, (v) => set((x) => { x.morph = v; })),
    h("h3", { class: "section-title", "data-section": "ai" }, "AI assistant"),
    renderAiSettings(ctx),
    h("h3", { class: "section-title" }, "About"),
    h(
      "div",
      { class: "about" },
      h("p", {}, h("strong", {}, APP.name), ` ${ctx.version ?? ""}`),
      h(
        "p",
        {},
        `A library for reading and studying the Bible: ${ctx.state.bibles.length || "many"} English translations, commentaries from the Church Fathers to the Reformation and after, cross-references, and the Hebrew and Greek, all on this device.`,
      ),
      h("p", {}, "Free, and always will be: no ads, no account, nothing to buy. Nothing is collected; your settings, bookmarks, and conversations stay on this device. If you set up an AI provider, your questions and what you attach go only to that provider."),
      h(
        "div",
        { class: "about-actions" },
        h("button", { type: "button", class: "button", onclick: () => openLicences(ctx) }, "Licences"),
        APP.source ? h("button", { type: "button", class: "button", onclick: () => openExternal(`${APP.source}/blob/main/PRIVACY.md`) }, "Privacy policy") : null,
        APP.source ? h("button", { type: "button", class: "button", onclick: () => openExternal(APP.source) }, "Source code") : null,
      ),
    ),
  );
}
