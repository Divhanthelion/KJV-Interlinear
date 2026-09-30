// Renders the Markdown that language models write (headings, lists, emphasis, code,
// quotes, tables) as DOM nodes. Text only ever goes in as text nodes, so nothing a
// model writes can run as HTML. Bible references become links.

import { h } from "./dom.js";

/**
 * Build a function that finds references like "John 3:16", "1 Cor. 13:4–7",
 * "Ps 23:1" in text. `books` is the app's book list ({ name, display, abbr }).
 * Returns (text) => [{ start, end, book, chapter, verse }].
 */
export function referenceFinder(books) {
  const alias = new Map();
  const add = (label, book) => alias.set(label.toLowerCase(), book);
  for (const b of books) {
    add(b.name, b.name);
    add(b.display, b.name);
    add(b.abbr, b.name);
    // "1 John" is also written "1John", "I John", "First John"
    const m = /^([123]) (.+)$/.exec(b.display);
    if (m) {
      add(`${m[1]}${m[2]}`, b.name);
      add(`${"I".repeat(Number(m[1]))} ${m[2]}`, b.name);
    }
  }
  add("Psalm", "Psalms");
  add("Pss", "Psalms");
  add("Song of Songs", "Song of Solomon");
  add("Revelations", "Revelation");
  const labels = [...alias.keys()].sort((a, b) => b.length - a.length).map((l) => l.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
  const pattern = new RegExp(
    `(?<![\\p{L}\\d])(${labels.join("|")})\\.?\\s+(\\d{1,3}):(\\d{1,3})(?:\\s*[-–]\\s*(\\d{1,3})(?!:))?`,
    "giu",
  );
  return (text) => {
    const found = [];
    for (const m of text.matchAll(pattern)) {
      found.push({
        start: m.index,
        end: m.index + m[0].length,
        book: alias.get(m[1].toLowerCase()),
        chapter: Number(m[2]),
        verse: Number(m[3]),
      });
    }
    return found;
  };
}

/**
 * Render `source` into a DocumentFragment.
 * opts: { findReferences(text), onReference(ref) }
 */
export function renderMarkdown(source, opts = {}) {
  const frag = document.createDocumentFragment();
  const lines = source.replace(/\r\n?/g, "\n").split("\n");
  let i = 0;

  const isBlank = (l) => /^\s*$/.test(l);
  const fence = /^\s*(```|~~~)/;
  const heading = /^\s{0,3}(#{1,6})\s+(.*?)\s*#*\s*$/;
  const rule = /^\s{0,3}([-*_])(\s*\1){2,}\s*$/;
  const quote = /^\s{0,3}>\s?/;
  const item = /^(\s*)([-*+•]|\d{1,3}[.)])\s+(.*)$/;
  const tableRow = /^\s*\|.*\|\s*$/;
  const tableRule = /^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$/;

  while (i < lines.length) {
    const line = lines[i];
    if (isBlank(line)) {
      i++;
      continue;
    }
    const f = fence.exec(line);
    if (f) {
      const body = [];
      i++;
      while (i < lines.length && !lines[i].trim().startsWith(f[1])) body.push(lines[i++]);
      i++;
      frag.append(h("pre", { class: "md-code" }, h("code", {}, body.join("\n"))));
      continue;
    }
    const hd = heading.exec(line);
    if (hd) {
      // Keep headings modest inside a chat bubble
      const level = Math.min(6, hd[1].length + 3);
      frag.append(h(`h${level}`, { class: "md-heading" }, inline(hd[2], opts)));
      i++;
      continue;
    }
    if (rule.test(line)) {
      frag.append(h("hr", {}));
      i++;
      continue;
    }
    if (quote.test(line)) {
      const body = [];
      while (i < lines.length && quote.test(lines[i])) body.push(lines[i++].replace(quote, ""));
      frag.append(h("blockquote", {}, renderMarkdown(body.join("\n"), opts)));
      continue;
    }
    if (tableRow.test(line) && i + 1 < lines.length && tableRule.test(lines[i + 1])) {
      const cells = (l) => l.trim().replace(/^\||\|$/g, "").split("|").map((c) => c.trim());
      const head = cells(line);
      i += 2;
      const rows = [];
      while (i < lines.length && tableRow.test(lines[i])) rows.push(cells(lines[i++]));
      frag.append(
        h(
          "div",
          { class: "md-table" },
          h(
            "table",
            {},
            h("thead", {}, h("tr", {}, head.map((c) => h("th", {}, inline(c, opts))))),
            h("tbody", {}, rows.map((r) => h("tr", {}, r.map((c) => h("td", {}, inline(c, opts)))))),
          ),
        ),
      );
      continue;
    }
    if (item.test(line)) {
      const block = [];
      while (i < lines.length && (item.test(lines[i]) || (/^\s+\S/.test(lines[i]) && block.length))) {
        block.push(lines[i++]);
        // A blank line inside a list continues it if the next line is indented or an item
        if (i + 1 < lines.length && isBlank(lines[i]) && (item.test(lines[i + 1]) || /^\s+\S/.test(lines[i + 1]))) i++;
      }
      frag.append(list(block, opts));
      continue;
    }
    const para = [];
    while (
      i < lines.length &&
      !isBlank(lines[i]) &&
      !fence.test(lines[i]) &&
      !heading.test(lines[i]) &&
      !quote.test(lines[i]) &&
      !item.test(lines[i]) &&
      !rule.test(lines[i])
    ) {
      para.push(lines[i++].trim());
    }
    frag.append(h("p", {}, inline(para.join("\n"), opts)));
  }
  return frag;
}

/** Nested lists from indented "- item" / "1. item" lines. */
function list(lines, opts) {
  const item = /^(\s*)([-*+•]|\d{1,3}[.)])\s+(.*)$/;
  const first = item.exec(lines[0]);
  const indent = first[1].length;
  const ordered = /\d/.test(first[2]);
  const root = h(ordered ? "ol" : "ul", {});
  if (ordered) {
    const start = parseInt(first[2], 10);
    if (start !== 1) root.setAttribute("start", String(start));
  }
  let current = null;
  let nested = [];
  const flush = () => {
    if (current && nested.length) current.append(list(nested, opts));
    nested = [];
  };
  for (const line of lines) {
    const m = item.exec(line);
    if (m && m[1].length <= indent) {
      flush();
      current = h("li", {}, inline(m[3], opts));
      root.append(current);
    } else if (m) {
      nested.push(line);
    } else if (current) {
      if (nested.length) nested.push(line);
      else current.append(" ", inline(line.trim(), opts));
    }
  }
  flush();
  return root;
}

/** Emphasis, code, and links inside a line; references become links. */
function inline(text, opts) {
  const out = [];
  const token = /(`+)([^`]+?)\1|\*\*([^*]+?)\*\*|__([^_]+?)__|(?<![\w*])\*(?!\s)([^*\n]+?)\*(?!\w)|(?<![\w_])_(?!\s)([^_\n]+?)_(?!\w)|\[([^\]]+)\]\(([^)\s]+)\)|\n/g;
  let last = 0;
  for (const m of text.matchAll(token)) {
    if (m.index > last) out.push(...linkRefs(text.slice(last, m.index), opts));
    if (m[2] !== undefined) out.push(h("code", {}, m[2]));
    else if (m[3] !== undefined || m[4] !== undefined) out.push(h("strong", {}, inline(m[3] ?? m[4], opts)));
    else if (m[5] !== undefined || m[6] !== undefined) out.push(h("em", {}, inline(m[5] ?? m[6], opts)));
    // External links are shown, not followed: the app opens only its own known sites
    else if (m[7] !== undefined) out.push(...linkRefs(m[7], opts), h("span", { class: "md-url" }, ` (${m[8]})`));
    else out.push(h("br", {}));
    last = m.index + m[0].length;
  }
  if (last < text.length) out.push(...linkRefs(text.slice(last), opts));
  return out;
}

function linkRefs(text, opts) {
  if (!opts.findReferences) return [text];
  const out = [];
  let last = 0;
  for (const ref of opts.findReferences(text)) {
    if (ref.start > last) out.push(text.slice(last, ref.start));
    out.push(
      h(
        "button",
        {
          type: "button",
          class: "ref-link",
          onclick: () => opts.onReference?.(ref),
        },
        text.slice(ref.start, ref.end),
      ),
    );
    last = ref.end;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}
