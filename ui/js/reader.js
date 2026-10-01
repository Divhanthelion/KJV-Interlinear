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
    case "parallel":
      return h(
        "div",
        { class: "parallel" },
        kjvText(verse),
        original ? originalParagraph(original, null) : noOriginal(),
      );
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
