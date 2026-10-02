// Runs inside the page (smoke.mjs injects it): pages through translation `bible` with
// the Next button, from the chapter already open, and checks that every verse and
// heading drawn reads exactly as the `bible_chapter` command gives it. That command's
// text is itself checked against eBible's own edition (crates/library/tests).
// `only`: book names to stay within (the KJV's Apocrypha), or null for all.
// Returns { chapters, verses, problems }.
async (bible, limit, only = null) => {
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const $ = (s) => document.querySelector(s);
  const $$ = (s) => [...document.querySelectorAll(s)];
  const problems = [];
  const collapse = (s) => s.replace(/\s+/g, " ").trim();
  const isLabel = (p) => p.styles.includes("vp") || p.styles.includes("va");
  const expected = (v) => collapse(v.parts.map((p) => (p.t === "text" ? (isLabel(p) ? " " : p.text) : p.t === "break" ? " " : "")).join(""));
  const drawn = (el) => {
    const p = el.querySelector(".library-text").cloneNode(true);
    // Verse numbers and printed labels stand apart; note markers sit against the text
    for (const x of p.querySelectorAll(".vnum, .vlabel")) x.replaceWith(" ");
    for (const x of p.querySelectorAll(".note-ref")) x.remove();
    return collapse([...p.querySelectorAll(".line")].map((l) => l.textContent).join(" "));
  };
  const api = async (book, chapter) =>
    (await fetch("/api/bible_chapter", { method: "POST", body: JSON.stringify({ bible, book, chapter }) })).json();

  // Where the app is now: find it from the heading by asking for the same chapter
  let chapters = 0;
  let verses = 0;
  let want = null;
  for (let i = 0; i < limit && problems.length < 50; i++) {
    if (i > 0) {
      if (!want.next || (only && !only.includes(want.next.book))) break;
      const before = $("#ref-label").textContent;
      $("#next-chapter").click();
      for (const end = Date.now() + 10000; ; await wait(5)) {
        if ($("#ref-label").textContent !== before && $("#reader").getAttribute("aria-busy") === "false") break;
        if (Date.now() > end) return { chapters, verses, problems: [...problems, `${want.heading}: Next never arrived`] };
      }
      want = await api(want.next.book, want.next.chapter);
    } else {
      // The first chapter: the page was opened at it
      const heading = $("#ref-label").textContent;
      const params = new URLSearchParams(location.search);
      want = await api(params.get("book"), Number(params.get("chapter")));
      if (want.heading !== heading) problems.push(`opened at ${heading}, expected ${want.heading}`);
    }
    const where = `${bible} ${want.heading}`;
    if ($("#ref-label").textContent !== want.heading) problems.push(`${where}: heading reads ${$("#ref-label").textContent}`);
    const els = $$("#reader .verse");
    const all = [...(want.title ? [want.title] : []), ...want.verses];
    if (els.length !== all.length) problems.push(`${where}: ${els.length} verses drawn, ${all.length} in the data`);
    els.forEach((el, j) => {
      const v = all[j];
      if (!v) return;
      if (el.dataset.label !== v.number) problems.push(`${where}:${v.number} drawn as ${el.dataset.label}`);
      const a = expected(v);
      const b = drawn(el);
      if (a !== b) {
        let k = 0;
        while (k < a.length && a[k] === b[k]) k++;
        problems.push(`${where}:${v.number} at ${k + 1}\n    data:  …${a.slice(Math.max(0, k - 30), k + 30)}\n    drawn: …${b.slice(Math.max(0, k - 30), k + 30)}`);
      }
      // Headings before the verse
      const headings = [];
      for (let prev = el.previousElementSibling; prev && !prev.classList.contains("verse") && !prev.classList.contains("chapter-heading"); prev = prev.previousElementSibling) headings.unshift(collapse(prev.textContent));
      const wantHeadings = v.before.map((x) => collapse(x.text));
      if (headings.join(" | ") !== wantHeadings.join(" | ")) problems.push(`${where}:${v.number} headings drawn ${JSON.stringify(headings)}, data ${JSON.stringify(wantHeadings)}`);
      verses++;
    });
    chapters++;
  }
  return { chapters, verses, problems };
}
