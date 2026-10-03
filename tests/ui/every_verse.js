// Runs inside the page (smoke.mjs injects it): walks every chapter with the Next
// button, as a reader would, and compares what is drawn with the text files,
// character for character. Returns a list of problems (empty when all is well).
//
//   chapters: [{ book, chapter, label, lines: [[verse, text], ...] }] in canonical order
//   view:     "kjv" or "interlinear" (which also checks every Hebrew/Greek word card)
async (chapters, view) => {
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const $ = (s) => document.querySelector(s);
  const $$ = (s) => [...document.querySelectorAll(s)];
  const problems = [];
  const report = (where, what) => problems.push(`${where}: ${what}`);
  const codepoint = (s, i) =>
    i >= s.length ? "end of verse" : `${JSON.stringify(String.fromCodePoint(s.codePointAt(i)))} (U+${s.codePointAt(i).toString(16).toUpperCase().padStart(4, "0")})`;
  const difference = (want, got) => {
    let i = 0;
    while (i < want.length && want[i] === got[i]) i++;
    return `character ${i + 1}: the file has ${codepoint(want, i)}, the screen has ${codepoint(got, i)}: …${got.slice(Math.max(0, i - 25), i + 25)}…`;
  };

  for (let i = 0; i < chapters.length && problems.length < 100; i++) {
    const want = chapters[i];
    if (i > 0) {
      const before = $("#ref-label").textContent;
      $("#next-chapter").click();
      for (const end = Date.now() + 10000; ; await wait(5)) {
        if ($("#ref-label").textContent !== before && $("#reader").getAttribute("aria-busy") === "false") break;
        if (Date.now() > end) {
          report(want.label, "never appeared after pressing Next");
          return problems;
        }
      }
    }
    const heading = $("#ref-label").textContent;
    if (!heading.endsWith(` ${want.chapter}`)) report(want.label, `the heading reads ${JSON.stringify(heading)}`);

    const verses = $$("#reader .verse");
    if (verses.length !== want.lines.length) report(want.label, `${verses.length} verses on screen, ${want.lines.length} in the file`);

    // The words the app was given for this chapter, to check every word card against
    const data =
      view === "interlinear"
        ? await (
            await fetch("/api/chapter", {
              method: "POST",
              headers: { "Content-Type": "application/json" },
              body: JSON.stringify({ book: want.book, chapter: want.chapter, options: { original: true } }),
            })
          ).json()
        : null;

    verses.forEach((el, j) => {
      const [v, text] = want.lines[j] ?? [NaN, ""];
      const where = `${want.label}:${v}`;
      const p = el.querySelector(".verse-text");
      if (!p) return report(where, "no KJV text on screen");
      const num = p.querySelector(".vnum");
      if (Number(el.dataset.verse) !== v || (num ? num.textContent.trim() : "0") !== String(v)) {
        report(where, `drawn as verse ${num?.textContent.trim() ?? "(title)"}`);
      }

      // The characters in the page
      const chars = [...p.childNodes].filter((n) => n !== num).map((n) => n.textContent).join("");
      if (chars !== text) report(where, difference(text, chars));

      // What the eye sees once CSS is applied: hidden text, text-transform, small caps
      if (!p.getClientRects().length) report(where, "not visible");
      const seen = p.innerText.replace(/\s+/g, " ").trim();
      const expected = num ? `${v} ${text}` : text;
      if (seen !== expected) report(where, `rendered differently: ${difference(expected, seen)}`);
      for (const node of [p, ...p.querySelectorAll("*")]) {
        const style = getComputedStyle(node);
        if (style.fontVariantCaps !== "normal" || style.textTransform !== "none" || style.visibility !== "visible") {
          report(where, `<${node.localName} class="${node.className}"> is styled ${style.fontVariantCaps} / ${style.textTransform} / ${style.visibility}`);
        }
      }

      // Every Hebrew/Greek word card
      if (data) {
        const verse = v === 0 ? data.title : data.verses.find((x) => x.number === v);
        const words = verse?.original?.words ?? [];
        const cards = [...el.querySelectorAll(".word")];
        if (cards.length !== words.length) report(where, `${cards.length} word cards on screen, ${words.length} words in the data`);
        cards.forEach((card, k) => {
          const w = words[k];
          if (!w) return;
          const part = (cls) => card.querySelector(cls)?.textContent ?? "";
          const shown = [part(".word-orig"), part(".word-translit"), part(".word-strongs"), part(".word-morph"), part(".word-gloss")];
          const given = [w.text, w.translit, w.strongs ?? "", w.morph ?? "", w.gloss || " "];
          shown.forEach((s, n) => {
            if (s !== given[n]) report(`${where} word ${k + 1}`, difference(given[n], s));
          });
        });
      }
    });
  }
  return problems;
}
