// Draws a chapter in the chosen view. Everything here is presentation: the text,
// red-letter spans, search hits, and glosses all arrive ready-made from Rust.

import { h, icon } from "./dom.js";

const RTL = new Set(["he", "arc"]);

function segmentsToNodes(segments) {
  return segments.map((s) => {
    if (s.hit) return h("mark", { class: s.red ? "hit red" : "hit" }, s.text);
    if (s.red) return h("span", { class: "red" }, s.text);
    return s.text;
  });
}

function verseNumber(verse) {
  return verse.number > 0 ? h("span", { class: "vnum" }, verse.number, " ") : null;
}

function kjvText(verse) {
  return h("p", { class: "verse-text" }, verseNumber(verse), segmentsToNodes(verse.segments));
}

function originalParagraph(original, verse) {
  const rtl = RTL.has(original.lang);
  return h(
    "p",
    { class: "orig-text", lang: original.lang, dir: rtl ? "rtl" : "ltr" },
    verse ? verseNumber(verse) : null,
    original.words.map((w) => w.text).join(" "),
  );
}

function noOriginal() {
  return h("p", { class: "no-original" }, "No Hebrew or Greek for this verse.");
}

function wordCard(word, lang) {
  const rtl = RTL.has(lang);
  const label = [word.text, word.translit, word.gloss, word.strongs && `Strong's ${word.strongs}`]
    .filter(Boolean)
    .join(", ");
  return h(
    "button",
    {
      class: "word",
      type: "button",
      dir: "ltr",
      "data-key": word.key,
      "aria-label": label,
      disabled: word.key ? null : true,
    },
    h("span", { class: "word-orig", lang, dir: rtl ? "rtl" : "ltr" }, word.text),
    word.translit ? h("span", { class: "word-translit" }, word.translit) : null,
    word.strongs ? h("span", { class: "word-strongs" }, word.strongs) : null,
    word.morph ? h("span", { class: "word-morph" }, word.morph) : null,
    h("span", { class: "word-gloss" }, word.gloss || " "),
  );
}

function verseBody(verse, view) {
  const original = verse.original;
  switch (view) {
    case "interlinear":
      return [
        kjvText(verse),
        original
          ? h(
              "div",
              { class: "words", dir: RTL.has(original.lang) ? "rtl" : "ltr" },
              original.words.map((w) => wordCard(w, original.lang)),
            )
          : noOriginal(),
      ];
    case "original":
      return original ? originalParagraph(original, verse) : kjvText(verse);
    default:
      return kjvText(verse);
  }
}

function verseElement(verse, view, selected) {
  const isTitle = verse.number === 0;
  return h(
    "div",
    {
      class: isTitle ? "verse is-title" : "verse",
      id: `v${verse.number}`,
      "data-verse": verse.number,
      "aria-current": selected ? "true" : null,
    },
    verseBody(verse, view),
  );
}

/**
 * Render `chapter` into `container`.
 * `nav` = { prevLabel, nextLabel, onPrev, onNext }.
 */
export function renderChapter(container, chapter, { view, selectedVerse, nav }) {
  const article = h(
    "article",
    { class: "chapter", lang: "en" },
    h("h1", { class: "chapter-heading" }, chapter.heading),
    chapter.title ? verseElement(chapter.title, view, false) : null,
    chapter.verses.map((v) => verseElement(v, view, v.number === selectedVerse)),
    h(
      "nav",
      { class: "chapter-nav", "aria-label": "Chapters" },
      nav.prevLabel
        ? h("button", { type: "button", onclick: nav.onPrev, title: nav.prevLabel }, icon("chevronLeft"), h("span", { class: "nav-label" }, nav.prevLabel))
        : h("span"),
      nav.nextLabel
        ? h("button", { type: "button", onclick: nav.onNext, title: nav.nextLabel }, h("span", { class: "nav-label" }, nav.nextLabel), icon("chevronRight"))
        : h("span"),
    ),
  );
  container.replaceChildren(article);
}

/** Mark verse `n` as selected (or none) without re-rendering. */
export function markSelected(container, n) {
  for (const el of container.querySelectorAll(".verse[aria-current]")) el.removeAttribute("aria-current");
  if (n) container.querySelector(`#v${n}`)?.setAttribute("aria-current", "true");
}

// ------------------------------------------------------------------ library translations

// Character styles from the source (USFM) and how they look
const STYLE_CLASSES = {
  wj: "red", // words of Jesus
  add: "it", // words the translators supplied
  it: "it",
  em: "it",
  tl: "it", // transliterated words
  bk: "it", // book titles
  sls: "it",
  qt: "it",
  nd: "sc", // the divine name, LORD
  sc: "sc",
  bd: "b",
  bdit: "b it",
  qs: "selah it", // "Selah"
  sup: "sup",
  vp: "vlabel", // a verse number printed in the text
  va: "vlabel",
};

const HEADING_CLASSES = {
  ms: "major-heading", // "BOOK 1" in the Psalms
  mr: "heading-refs",
  r: "heading-refs", // parallel passages
  sp: "speaker", // "Beloved", "Friends" in the Song of Songs
  d: "acrostic", // Psalm 119's letters, set apart
  qa: "acrostic",
};

/** A verse's text: its words only, line breaks as spaces, notes and labels left out. */
export function libraryVerseText(verse) {
  return verse.parts
    .map((p) => (p.t === "text" ? (p.styles.includes("vp") || p.styles.includes("va") ? " " : p.text) : p.t === "break" ? " " : ""))
    .join("")
    .replace(/\s+/g, " ")
    .trim();
}

function styled(part) {
  const classes = [...new Set(part.styles.flatMap((s) => (STYLE_CLASSES[s] ?? "").split(" ")).filter(Boolean))];
  return classes.length ? h("span", { class: classes.join(" ") }, part.text) : part.text;
}

function headingElement(heading) {
  const base = heading.marker.replace(/\d+$/, "");
  const cls = HEADING_CLASSES[base] ?? (base === "s" && heading.marker !== "s1" ? "section-heading minor" : "section-heading");
  return h(cls.includes("section-heading") || cls === "major-heading" ? "h2" : "p", { class: cls }, heading.text);
}

function noteButton(part, onToggle) {
  const text = part.parts.map((p) => p.text).join("").replace(/\s+/g, " ").trim();
  const isCrossRef = part.marker === "x" || part.marker === "ex";
  return h(
    "button",
    {
      type: "button",
      class: "note-ref",
      "aria-expanded": "false",
      "aria-label": isCrossRef ? `Cross references: ${text}` : `Note: ${text}`,
      title: text,
      onclick: (event) => onToggle(event.currentTarget, part, text),
    },
    isCrossRef ? "†" : "*",
  );
}

/** Show or hide a note's text under its verse. */
function toggleNote(button, part, text) {
  const verse = button.closest(".verse");
  const open = button.getAttribute("aria-expanded") === "true";
  for (const b of verse.querySelectorAll(".note-ref[aria-expanded='true']")) b.setAttribute("aria-expanded", "false");
  verse.querySelector(".verse-note")?.remove();
  if (open) return;
  button.setAttribute("aria-expanded", "true");
  const label = part.parts.find((p) => p.marker === "fr" || p.marker === "xo")?.text.trim();
  const body = part.parts.filter((p) => p.marker !== "fr" && p.marker !== "xo");
  verse.append(
    h(
      "p",
      { class: "verse-note", role: "note" },
      label ? h("span", { class: "verse-note-ref" }, label, " ") : null,
      body.map((p) => (p.marker === "fq" || p.marker === "fqa" ? h("span", { class: "it" }, p.text) : p.text)),
    ),
  );
  if (!text) verse.querySelector(".verse-note")?.remove();
}

/** The verse's text as lines: poetry and paragraph breaks start new lines. */
function libraryLines(verse, number) {
  const lines = [];
  let line = h("span", { class: "line", "data-kind": verse.starts ?? "p" }, number);
  for (const part of verse.parts) {
    if (part.t === "break") {
      lines.push(line);
      line = h("span", { class: "line", "data-kind": part.kind });
    } else if (part.t === "text") {
      line.append(styled(part));
    } else if (part.t === "note") {
      line.append(noteButton(part, toggleNote));
    }
  }
  lines.push(line);
  return lines;
}

function libraryVerse(verse, selected) {
  const isTitle = verse.number === "0";
  const first = parseInt(verse.number, 10) || 0;
  const label = verse.published ?? verse.number;
  const number = isTitle ? null : h("span", { class: "vnum" }, label, verse.alternate ? h("span", { class: "valt" }, ` (${verse.alternate})`) : null, " ");
  return [
    verse.before.map(headingElement),
    h(
      "div",
      {
        class: isTitle ? "verse is-title" : "verse",
        id: `v${first}`,
        "data-verse": first,
        "data-label": verse.number,
        "aria-current": selected ? "true" : null,
      },
      h("p", { class: "verse-text library-text" }, libraryLines(verse, number)),
    ),
  ];
}

/** One character folded as the search folds it (crates/library/src/text.rs): curly
 * quotes straight, dashes plain, "æ" as "ae", spaces as spaces, lower case. */
function foldChar(c) {
  if (c === "\u2018" || c === "\u2019" || c === "\u201B" || c === "\u02BC") return "'";
  if (c === "\u201C" || c === "\u201D") return '"';
  const code = c.codePointAt(0);
  if (code >= 0x2010 && code <= 0x2014) return "-";
  if (c === "æ" || c === "Æ") return "ae";
  if (/\s/u.test(c)) return " ";
  return c.toLowerCase();
}

/**
 * Mark every match of `query` in a library chapter's verses, as the search finds them
 * (the KJV's own chapters come from Rust with their matches marked). A verse's lines
 * are joined with a space, as the search reads them; verse numbers and note markers
 * aren't part of the text.
 */
function markMatches(container, query) {
  let needle = "";
  for (const c of query.trim()) needle += foldChar(c);
  if (!needle) return;
  for (const verse of container.querySelectorAll(".verse-text")) {
    const nodes = [];
    let text = "";
    let lastLine = null;
    const walker = document.createTreeWalker(verse, NodeFilter.SHOW_TEXT, {
      acceptNode: (n) => (n.parentElement.closest(".vnum, .note-ref, .verse-note") ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT),
    });
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      const line = n.parentElement.closest(".line");
      if (lastLine && line !== lastLine) text += " ";
      lastLine = line;
      nodes.push({ node: n, start: text.length });
      text += n.nodeValue;
    }
    // The folded text, and for each of its code units where it came from in `text`
    let folded = "";
    const from = [];
    for (let i = 0; i < text.length; ) {
      const ch = String.fromCodePoint(text.codePointAt(i));
      const f = foldChar(ch);
      for (let k = 0; k < f.length; k++) from.push(i);
      folded += f;
      i += ch.length;
    }
    from.push(text.length);
    const ranges = [];
    for (let at = folded.indexOf(needle); at >= 0; at = folded.indexOf(needle, at + needle.length)) {
      const end = at + needle.length;
      // To the end of the last character matched
      let e = from[end];
      if (e === from[end - 1]) e = from.find((x, k) => k > end && x > from[end - 1]) ?? text.length;
      ranges.push([from[at], e]);
    }
    // Last first, so the earlier text nodes and offsets stay as they were
    for (const [s, e] of ranges.reverse()) {
      for (const { node, start } of [...nodes].reverse()) {
        const a = Math.max(s, start);
        const b = Math.min(e, start + node.nodeValue.length);
        if (a >= b) continue;
        const range = document.createRange();
        range.setStart(node, a - start);
        range.setEnd(node, b - start);
        range.surroundContents(h("mark", { class: "hit" }));
      }
    }
  }
}

/** Render a chapter of a library translation (from the `bible_chapter` command). */
export function renderLibraryChapter(container, chapter, { selectedVerse, nav, highlight = null }) {
  const article = h(
    "article",
    { class: "chapter library", lang: "en" },
    h("h1", { class: "chapter-heading" }, chapter.heading, h("span", { class: "chapter-translation" }, chapter.abbr)),
    chapter.title ? libraryVerse(chapter.title, false) : null,
    chapter.verses.map((v) => libraryVerse(v, (parseInt(v.number, 10) || 0) === selectedVerse)),
    chapter.after.map(headingElement),
    h(
      "nav",
      { class: "chapter-nav", "aria-label": "Chapters" },
      nav.prevLabel
        ? h("button", { type: "button", onclick: nav.onPrev, title: nav.prevLabel }, icon("chevronLeft"), h("span", { class: "nav-label" }, nav.prevLabel))
        : h("span"),
      nav.nextLabel
        ? h("button", { type: "button", onclick: nav.onNext, title: nav.nextLabel }, h("span", { class: "nav-label" }, nav.nextLabel), icon("chevronRight"))
        : h("span"),
    ),
  );
  if (highlight) markMatches(article, highlight);
  container.replaceChildren(article);
}

// ------------------------------------------------------------------ translations side by side

/** One column's verses for one row: its text as the reader draws it, numbered as that
 * translation numbers it (with the chapter where it differs from the row's). */
function parallelCell(column, cell) {
  // Named on phones, where the columns stack (drawn from data-name, so it isn't text)
  const attrs = (cls) => ({ class: cls, "data-name": column.abbr });
  if (cell.above) return h("div", attrs("pr-cell is-above"), h("p", { class: "pr-note" }, "With the verse above"));
  if (!cell.verses.length) return h("div", attrs("pr-cell is-empty"), h("p", { class: "pr-note" }, "Not in this translation"));
  return h(
    "div",
    attrs("pr-cell"),
    cell.verses.map((v) => {
      const number = v.label === "title" ? null : h("span", { class: "vnum" }, v.label, " ");
      if (v.original) {
        const rtl = RTL.has(v.original.lang);
        return h("p", { class: "orig-text", lang: v.original.lang, dir: rtl ? "rtl" : "ltr" }, number, v.original.words.map((w) => w.text).join(" "));
      }
      return h("p", { class: `verse-text library-text${v.label === "title" ? " is-title" : ""}` }, libraryLines({ parts: v.parts ?? [], starts: null }, number));
    }),
  );
}

/**
 * Render translations side by side (from the `parallel` command): a row for each verse
 * of the leading translation, a column for each translation (and the Hebrew or Greek).
 * `bar` is the strip for choosing the columns.
 */
export function renderParallel(container, chapter, { selectedVerse, nav, highlight = null, bar = null }) {
  const rows = [];
  for (const row of chapter.rows) {
    rows.push(row.before.map(headingElement));
    const first = parseInt(row.number, 10) || 0;
    const isTitle = row.number === "0";
    rows.push(
      h(
        "div",
        {
          class: isTitle ? "verse pr-row is-title" : "verse pr-row",
          id: `v${first}`,
          "data-verse": first,
          "data-label": row.number,
          "aria-current": !isTitle && first === selectedVerse ? "true" : null,
        },
        row.cells.map((cell, i) => parallelCell(chapter.columns[i], cell)),
      ),
    );
  }
  const article = h(
    "article",
    { class: "chapter parallel-reading", lang: "en" },
    h("h1", { class: "chapter-heading" }, chapter.heading),
    bar,
    h("div", { class: "pr-columns", "aria-hidden": "true" }, chapter.columns.map((c) => h("div", { class: "pr-column-name", title: c.name }, c.abbr))),
    rows,
    chapter.after.map(headingElement),
    h(
      "nav",
      { class: "chapter-nav", "aria-label": "Chapters" },
      nav.prevLabel
        ? h("button", { type: "button", onclick: nav.onPrev, title: nav.prevLabel }, icon("chevronLeft"), h("span", { class: "nav-label" }, nav.prevLabel))
        : h("span"),
      nav.nextLabel
        ? h("button", { type: "button", onclick: nav.onNext, title: nav.nextLabel }, h("span", { class: "nav-label" }, nav.nextLabel), icon("chevronRight"))
        : h("span"),
    ),
  );
  article.style.setProperty("--columns", String(chapter.columns.length));
  if (highlight) markMatches(article, highlight);
  container.replaceChildren(article);
}
