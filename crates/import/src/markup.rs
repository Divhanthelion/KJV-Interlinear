//! Source markup (OSIS, ThML) -> the library's note markup (docs/LIBRARY.md, "Note
//! markup"), with a proof that no text was lost.
//!
//! # The target markup
//!
//! Blocks `<p>` `<h>` `<l>` `<li>` and `<tr><td>…</td></tr>`; inline `<i>` `<b>` `<sup>`
//! `<sc>` `<lang code="…">` `<ref to="…">` `<br/>` and `<fn>`. Text is plain Unicode with
//! `&`, `<`, `>` escaped as `&amp;`, `&lt;`, `&gt;` and nothing else escaped.
//!
//! `<fn>…</fn>` is an addition to the documented markup: a note inside a note (Catena
//! Aurea's editor's notes, Keil & Delitzsch's and Matthew Henry's footnotes). It stays
//! exactly where the source put it, inside its paragraph, so the reading order and the
//! text are unchanged and a reader can show it as a footnote, a bracketed aside, or
//! inline. A `<ref>`'s `to` is one OSIS-style range or several separated by spaces
//! (`to="PRO.8.22-PRO.8.24 PRO.16.4"`) when the source cites a list in one element.
//!
//! # Mapping
//!
//! | OSIS | ThML | target |
//! |---|---|---|
//! | `div type=x-p` (sID/eID milestones, or a container) | `p` | paragraph boundary; text outside any paragraph gets an implicit `<p>` |
//! | `title` | | `<h>` |
//! | `l` (sID/eID milestones, or a container) | | `<l>` |
//! | `item` | | `<li>` |
//! | `row` / `cell` | | `<tr>` / `<td>` |
//! | `hi type=italic\|underline` | `i` | `<i>` |
//! | `hi type=bold` | `b` | `<b>` |
//! | `hi type=super` | `sup` | `<sup>` |
//! | `hi type=small-caps` | | `<sc>` |
//! | `foreign xml:lang` | | `<lang code>` |
//! | `transChange` | | `<i>` |
//! | `q` | | its text (a `marker` holding quotation marks the text lacks is an error) |
//! | `lb`, `milestone type=x-p` | `br` | `<br/>` / paragraph boundary |
//! | `note` | | `<fn>` |
//! | `reference osisRef` | `scripRef passage` (or its text) | `<ref to>` |
//! | `div` (book, introduction, section, x-milestone), `chapter`, `lg`, `list`, `table`, `milestone x-usfm-toc*` | | structure only: dropped and counted |
//! | | `sync type=Strongs` | dropped and counted (a Strong's number, not text) |
//!
//! Anything else is an error naming the element, never silently dropped.
//!
//! # References
//!
//! A reference's text is always kept. Its `to` is the source's own reading (`osisRef`, or the
//! `passage` of a ThML `scripRef`, or its text) when that can be read and names verses the KJV
//! has. Where a `to` would be wrong it is left off, and the build counts and lists the case:
//!
//! * `refs_unparsed`: the source's reading cannot be read (a backwards range, a book name cut
//!   short, a `scripRef` that wraps a few words of prose);
//! * `refs_corrected` / `refs_withheld`: an OSIS `osisRef` that the text the reader sees
//!   contradicts (see `Conv::osis_link`);
//! * `refs_impossible`: a chapter or verse the KJV does not have ("Mic.35").
//!
//! The Treasury of Scripture Knowledge and Wesley's Notes write references with no book
//! ("22; 8:17"): the book and chapter of the note they stand in supply it ([`Options::context`]).
//!
//! Output is tidied: whitespace is collapsed, a space at the inside edge of an inline element
//! is moved outside it, a line break does not begin or end a block or a footnote, and an
//! element with nothing in it is removed.
//!
//! # The proof
//!
//! [`prove`] reads the source and the converted body independently and checks that the
//! text content (entities decoded, markup removed) is identical character for
//! character. Whitespace is compared as "a gap or no gap" between neighbouring
//! characters: collapsing whitespace is allowed, joining or splitting words is not. The
//! converted body may have a gap where the source had none only at a block boundary or a
//! `<br/>` (those separate lines; the count is reported as `structural_gaps`).

use std::collections::{BTreeMap, BTreeSet};

use kjv_library::reference::{self, Context, Options as RefOptions};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// Well-formed XML: anything that is not a tag or an entity is an error.
    Osis,
    /// Wesley's and TSK's ThML is not well-formed: a bare `&` ("&c.") and stray `<` / `>`
    /// are text. They are kept as text and counted.
    Thml,
}

#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub dialect: Dialect,
    /// "Jud" in this module's scripture references means Judges (not Jude).
    pub jud_is_judges: bool,
    /// ThML only: a scripture reference with no book is in this book (and, when the
    /// chapter is not 0, a bare number is a verse of this chapter).
    pub context: Option<(&'static str, u32)>,
}

/// What a conversion noticed, for the build report.
#[derive(Debug, Default, Clone)]
pub struct Stats {
    /// References converted with a `to`, and the texts of those that could not be (kept
    /// as plain `<ref>` without `to`).
    pub refs_resolved: u64,
    pub unparsed: Vec<String>,
    /// OSIS references whose `osisRef` contradicts what the reader sees, as `osisRef (shown)`:
    /// those given the text's reading (`refs_corrected`) and those left with no `to`
    /// (`refs_withheld`). See `Conv::osis_link`.
    pub refs_corrected: Vec<String>,
    pub refs_withheld: Vec<String>,
    /// References to a chapter or verse the KJV does not have ("Mic.35"), as `source (shown)`:
    /// typos in the source, left with no `to`.
    pub refs_impossible: Vec<String>,
    /// Structural elements dropped, by description.
    pub ignored: BTreeMap<String, u64>,
    /// Attributes that carried data but no text, dropped.
    pub dropped_data: BTreeMap<String, u64>,
    /// ThML text that looked like markup but was not (kept as text), with context.
    pub literal_markup: Vec<String>,
    /// Bare `&` in ThML kept as text.
    pub bare_ampersands: u64,
    /// Block boundaries and `<br/>` where the converted text has a gap the source did not.
    pub structural_gaps: u64,
    /// Footnotes (`<fn>`) emitted.
    pub footnotes: u64,
}

impl Stats {
    pub fn add(&mut self, other: Stats) {
        self.refs_resolved += other.refs_resolved;
        self.unparsed.extend(other.unparsed);
        self.refs_corrected.extend(other.refs_corrected);
        self.refs_withheld.extend(other.refs_withheld);
        self.refs_impossible.extend(other.refs_impossible);
        for (k, v) in other.ignored {
            *self.ignored.entry(k).or_default() += v;
        }
        for (k, v) in other.dropped_data {
            *self.dropped_data.entry(k).or_default() += v;
        }
        self.literal_markup.extend(other.literal_markup);
        self.bare_ampersands += other.bare_ampersands;
        self.structural_gaps += other.structural_gaps;
        self.footnotes += other.footnotes;
    }
}

// ------------------------------------------------------------------------------------
// Tokens
// ------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    Text(String),
    Open(String, Vec<(String, String)>),
    Close(String),
    Empty(String, Vec<(String, String)>),
}

fn is_name_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_name_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b':' | b'.' | b'-')
}

fn is_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r')
}

/// A tag at the start of `s` (which begins with `<`): its token and byte length, or `None`
/// when what follows `<` is not a well-formed tag.
fn scan_tag(s: &str) -> Option<(Tok, usize)> {
    let b = s.as_bytes();
    let mut i = 1;
    let closing = b.get(i) == Some(&b'/');
    if closing {
        i += 1;
    }
    let start = i;
    if !b.get(i).copied().is_some_and(is_name_start) {
        return None;
    }
    while b.get(i).copied().is_some_and(is_name_char) {
        i += 1;
    }
    let name = s[start..i].to_string();
    if closing {
        while b.get(i).copied().is_some_and(is_ws) {
            i += 1;
        }
        return (b.get(i) == Some(&b'>')).then(|| (Tok::Close(name), i + 1));
    }
    let mut attrs = Vec::new();
    loop {
        let before = i;
        while b.get(i).copied().is_some_and(is_ws) {
            i += 1;
        }
        let spaced = i > before;
        match b.get(i) {
            Some(b'>') => return Some((Tok::Open(name, attrs), i + 1)),
            Some(b'/') if b.get(i + 1) == Some(&b'>') => return Some((Tok::Empty(name, attrs), i + 2)),
            Some(&c) if spaced && is_name_start(c) => {
                let a0 = i;
                while b.get(i).copied().is_some_and(is_name_char) {
                    i += 1;
                }
                let aname = s[a0..i].to_string();
                while b.get(i).copied().is_some_and(is_ws) {
                    i += 1;
                }
                if b.get(i) != Some(&b'=') {
                    return None;
                }
                i += 1;
                while b.get(i).copied().is_some_and(is_ws) {
                    i += 1;
                }
                let q = *b.get(i)?;
                if q != b'"' && q != b'\'' {
                    return None;
                }
                i += 1;
                let v0 = i;
                while *b.get(i)? != q {
                    if b[i] == b'<' {
                        return None;
                    }
                    i += 1;
                }
                let value = decode_entities_strict(&s[v0..i])?;
                i += 1;
                attrs.push((aname, value));
            }
            _ => return None,
        }
    }
}

/// `&amp;` `&lt;` `&gt;` `&quot;` `&apos;` `&#N;` `&#xH;` at the start of `s`.
fn scan_entity(s: &str) -> Option<(char, usize)> {
    let end = s.as_bytes().iter().take(12).position(|&c| c == b';')?;
    let name = &s[1..end];
    let c = match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        _ => {
            let digits = name.strip_prefix('#')?;
            let n = match digits.strip_prefix(['x', 'X']) {
                Some(h) if !h.is_empty() && h.bytes().all(|c| c.is_ascii_hexdigit()) => u32::from_str_radix(h, 16).ok()?,
                None if !digits.is_empty() && digits.bytes().all(|c| c.is_ascii_digit()) => digits.parse().ok()?,
                _ => return None,
            };
            char::from_u32(n).filter(|&c| c != '\0')?
        }
    };
    Some((c, end + 1))
}

fn decode_entities_strict(s: &str) -> Option<String> {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let (c, n) = scan_entity(&rest[at..])?;
        out.push(c);
        rest = &rest[at + n..];
    }
    out.push_str(rest);
    Some(out)
}

fn context(raw: &str, at: usize) -> String {
    let a = raw[..at].char_indices().rev().nth(30).map_or(0, |(i, _)| i);
    let b = raw[at..].char_indices().nth(40).map_or(raw.len(), |(i, _)| at + i);
    raw[a..b].replace('\n', "\\n")
}

/// Splits markup into tokens. Text has its entities decoded.
pub fn tokenize(raw: &str, dialect: Dialect, stats: &mut Stats) -> Result<Vec<Tok>, String> {
    let mut out: Vec<Tok> = Vec::new();
    let mut text = String::new();
    let b = raw.as_bytes();
    let mut i = 0;
    let flush = |text: &mut String, out: &mut Vec<Tok>| {
        if !text.is_empty() {
            out.push(Tok::Text(std::mem::take(text)));
        }
    };
    while i < b.len() {
        match b[i] {
            b'<' => match scan_tag(&raw[i..]) {
                Some((tok, n)) => {
                    flush(&mut text, &mut out);
                    out.push(tok);
                    i += n;
                }
                None if dialect == Dialect::Thml => {
                    stats.literal_markup.push(format!("<  in: {}", context(raw, i)));
                    text.push('<');
                    i += 1;
                }
                None => return Err(format!("not a well-formed tag at: {}", context(raw, i))),
            },
            b'&' => match scan_entity(&raw[i..]) {
                Some((c, n)) => {
                    text.push(c);
                    i += n;
                }
                None if dialect == Dialect::Thml => {
                    stats.bare_ampersands += 1;
                    text.push('&');
                    i += 1;
                }
                None => return Err(format!("unknown entity or bare & at: {}", context(raw, i))),
            },
            _ => {
                // copy up to the next special byte (all of them ASCII, so this is a char boundary)
                let j = raw[i..].find(['<', '&']).map_or(raw.len(), |n| i + n);
                text.push_str(&raw[i..j]);
                i = j;
            }
        }
    }
    flush(&mut text, &mut out);
    Ok(out)
}

// ------------------------------------------------------------------------------------
// Tree
// ------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct El {
    name: String,
    attrs: Vec<(String, String)>,
    kids: Vec<Node>,
}

#[derive(Debug, Clone)]
enum Node {
    Text(String),
    El(El),
}

impl El {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    fn describe(&self) -> String {
        let mut s = format!("<{}", self.name);
        for (k, v) in &self.attrs {
            s.push_str(&format!(" {k}=\"{v}\""));
        }
        s.push('>');
        s
    }

    fn text(&self) -> String {
        fn go(kids: &[Node], out: &mut String) {
            for k in kids {
                match k {
                    Node::Text(t) => out.push_str(t),
                    Node::El(e) => go(&e.kids, out),
                }
            }
        }
        let mut s = String::new();
        go(&self.kids, &mut s);
        s
    }
}

fn build_tree(toks: Vec<Tok>) -> Result<Vec<Node>, String> {
    let mut stack: Vec<El> = Vec::new();
    let mut root: Vec<Node> = Vec::new();
    fn push(stack: &mut [El], root: &mut Vec<Node>, n: Node) {
        match stack.last_mut() {
            Some(top) => top.kids.push(n),
            None => root.push(n),
        }
    }
    for t in toks {
        match t {
            Tok::Text(s) => push(&mut stack, &mut root, Node::Text(s)),
            Tok::Empty(name, attrs) => push(&mut stack, &mut root, Node::El(El { name, attrs, kids: Vec::new() })),
            Tok::Open(name, attrs) => stack.push(El { name, attrs, kids: Vec::new() }),
            Tok::Close(name) => {
                let Some(top) = stack.pop() else {
                    return Err(format!("</{name}> closes nothing"));
                };
                if top.name != name {
                    return Err(format!("</{name}> closes {}", top.describe()));
                }
                push(&mut stack, &mut root, Node::El(top));
            }
        }
    }
    if let Some(top) = stack.last() {
        return Err(format!("{} is never closed", top.describe()));
    }
    Ok(root)
}

// ------------------------------------------------------------------------------------
// Output model
// ------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tag {
    I,
    B,
    Sup,
    Sc,
    Lang(String),
    Ref(Option<String>),
    Fn,
}

impl Tag {
    fn name(&self) -> &'static str {
        match self {
            Tag::I => "i",
            Tag::B => "b",
            Tag::Sup => "sup",
            Tag::Sc => "sc",
            Tag::Lang(_) => "lang",
            Tag::Ref(_) => "ref",
            Tag::Fn => "fn",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Item {
    Text(String),
    Open(Tag),
    Close(Tag),
    Br,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    P,
    H,
    L,
    Li,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::P => "p",
            Kind::H => "h",
            Kind::L => "l",
            Kind::Li => "li",
        }
    }
}

#[derive(Debug, Clone)]
enum Block {
    Flow(Kind, Vec<Item>),
    Row(Vec<Vec<Item>>),
}

/// Removes the space at the end of the last text, if only tags (no `<br/>`) follow it.
fn strip_trailing_space(out: &mut Vec<Item>) {
    let Some(i) = out.iter().rposition(|x| matches!(x, Item::Text(_))) else { return };
    if out[i + 1..].iter().any(|x| matches!(x, Item::Br)) {
        return;
    }
    if let Item::Text(t) = &mut out[i] {
        if t.ends_with(' ') {
            t.pop();
        }
        if t.is_empty() {
            out.remove(i);
        }
    }
}

/// Collapses whitespace, trims block edges and `<br/>` edges, and removes elements that
/// ended up empty. Returns an empty vector for a block with no text.
fn tidy(items: Vec<Item>) -> Vec<Item> {
    // 0. A footnote does not begin or end with a line break (they stand where its own
    //    paragraphs were).
    let mut items = items;
    loop {
        let at = (1..items.len()).find(|&i| {
            matches!((&items[i - 1], &items[i]), (Item::Open(Tag::Fn), Item::Br))
                || matches!((&items[i - 1], items.get(i)), (Item::Br, Some(Item::Close(Tag::Fn))))
        });
        let Some(i) = at else { break };
        // the Br is the second of the pair when it follows the opening, else the first
        items.remove(if matches!(items[i], Item::Br) { i } else { i - 1 });
    }
    // 1. Whitespace runs become one space; none at the start of the block, after a
    //    `<br/>`, or after a space already written; none before a `<br/>` or at the end.
    let mut out: Vec<Item> = Vec::with_capacity(items.len());
    let mut prev_space = true;
    for item in items {
        match item {
            Item::Text(t) => {
                let mut s = String::with_capacity(t.len());
                for c in t.chars() {
                    if c.is_ascii() && is_ws(c as u8) {
                        if !prev_space {
                            s.push(' ');
                            prev_space = true;
                        }
                    } else {
                        s.push(c);
                        prev_space = false;
                    }
                }
                if !s.is_empty() {
                    out.push(Item::Text(s));
                }
            }
            Item::Br => {
                strip_trailing_space(&mut out);
                out.push(Item::Br);
                prev_space = true;
            }
            other => out.push(other),
        }
    }
    strip_trailing_space(&mut out);
    // 2. No `<br/>` before the first text or after the last.
    let (Some(first), Some(last)) = (
        out.iter().position(|x| matches!(x, Item::Text(_))),
        out.iter().rposition(|x| matches!(x, Item::Text(_))),
    ) else {
        return Vec::new();
    };
    let mut kept: Vec<Item> = Vec::with_capacity(out.len());
    for (i, item) in out.into_iter().enumerate() {
        if !(matches!(item, Item::Br) && (i < first || i > last)) {
            kept.push(item);
        }
    }
    // 3. Remove elements with nothing inside.
    let mut out: Vec<Item> = Vec::with_capacity(kept.len());
    for item in kept {
        match item {
            Item::Close(tag) if matches!(out.last(), Some(Item::Open(t)) if *t == tag) => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    hoist_spaces(out)
}

/// Moves the space at the inside edge of an element to its outside ("a<i> b </i>c" becomes
/// "a <i>b</i> c"), so that an element's content is exactly the words it marks. The text and
/// its gaps are unchanged (step 1 left no two gaps side by side).
fn hoist_spaces(mut items: Vec<Item>) -> Vec<Item> {
    loop {
        let mut changed = false;
        let mut i = 0;
        while i < items.len() {
            match (&items[i], items.get(i + 1)) {
                // <x> text ...   ->   " " <x> text ...
                (Item::Open(_), Some(Item::Text(t))) if t.starts_with(' ') => {
                    let rest = t[1..].to_string();
                    if rest.is_empty() {
                        items.remove(i + 1);
                    } else {
                        items[i + 1] = Item::Text(rest);
                    }
                    match i.checked_sub(1).map(|j| &mut items[j]) {
                        Some(Item::Text(prev)) => prev.push(' '),
                        _ => items.insert(i, Item::Text(" ".into())),
                    }
                    changed = true;
                }
                // ... text </x>   ->   ... text </x> " "
                (Item::Text(t), Some(Item::Close(_))) if t.ends_with(' ') => {
                    let rest = t[..t.len() - 1].to_string();
                    let had_text = !rest.is_empty();
                    if had_text {
                        items[i] = Item::Text(rest);
                    } else {
                        items.remove(i);
                    }
                    // the Close is now at `at`; put the space after it
                    let at = if had_text { i + 1 } else { i };
                    match items.get_mut(at + 1) {
                        Some(Item::Text(next)) => next.insert(0, ' '),
                        _ => items.insert(at + 1, Item::Text(" ".into())),
                    }
                    changed = true;
                }
                _ => {}
            }
            i += 1;
        }
        if !changed {
            return items;
        }
    }
}

fn escape(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
}

fn write_items(items: &[Item], out: &mut String) {
    for item in items {
        match item {
            Item::Text(t) => escape(t, out),
            Item::Br => out.push_str("<br/>"),
            Item::Open(tag) => {
                out.push('<');
                out.push_str(tag.name());
                match tag {
                    Tag::Lang(code) => {
                        out.push_str(" code=\"");
                        out.push_str(code);
                        out.push('"');
                    }
                    Tag::Ref(Some(to)) => {
                        out.push_str(" to=\"");
                        out.push_str(to);
                        out.push('"');
                    }
                    _ => {}
                }
                out.push('>');
            }
            Item::Close(tag) => {
                out.push_str("</");
                out.push_str(tag.name());
                out.push('>');
            }
        }
    }
}

fn write_blocks(blocks: &[Block]) -> String {
    let mut out = String::new();
    for b in blocks {
        match b {
            Block::Flow(kind, items) => {
                out.push('<');
                out.push_str(kind.name());
                out.push('>');
                write_items(items, &mut out);
                out.push_str("</");
                out.push_str(kind.name());
                out.push('>');
            }
            Block::Row(cells) => {
                out.push_str("<tr>");
                for c in cells {
                    out.push_str("<td>");
                    write_items(c, &mut out);
                    out.push_str("</td>");
                }
                out.push_str("</tr>");
            }
        }
    }
    out
}

// ------------------------------------------------------------------------------------
// Conversion
// ------------------------------------------------------------------------------------

/// What became of a scripture reference.
enum Link {
    To(String),
    /// The source gave nothing usable (counted in `Stats::unparsed`).
    Unresolved,
    /// The reference is left with no `to`, and counted in `Stats::refs_withheld` (the
    /// `osisRef` is contradicted by the shown text) or `Stats::refs_impossible`.
    Withheld,
}

fn join_osis(ranges: &[reference::Range]) -> String {
    ranges.iter().map(|r| r.osis()).collect::<Vec<_>>().join(" ")
}

/// Every verse the ranges cover (so "Gen 1" and "Gen 1:1-31" compare equal), or `None` if a
/// range leaves the KJV's chapters (a book it does not have, or a chapter past the end).
fn verse_set(ranges: &[reference::Range]) -> Option<BTreeSet<(&'static str, u32, u32)>> {
    let mut out = BTreeSet::new();
    for r in ranges {
        let book = kjv_sword::kjv::book(r.book)?;
        let chapters = book.verses.len() as u32;
        let last = |c: u32| book.verses.get(c.checked_sub(1)? as usize).map(|&v| u32::from(v));
        let end_chapter = if r.end.0 == reference::END { chapters } else { r.end.0 };
        let end_verse = if r.end.0 == reference::END || r.end.1 == reference::END { last(end_chapter)? } else { r.end.1 };
        if end_chapter > chapters || r.start.0 > end_chapter {
            return None;
        }
        for c in r.start.0..=end_chapter {
            let from = if c == r.start.0 { r.start.1.max(1) } else { 1 };
            let to = if c == end_chapter { end_verse } else { last(c)? };
            if to > last(c)? || from > last(c)? {
                return None;
            }
            for v in from..=to {
                out.insert((r.book, c, v));
            }
        }
    }
    Some(out)
}

/// Do the ranges lie in the KJV's chapters and verses? A book the KJV does not have (the
/// Apocrypha) cannot be judged and passes.
fn possible(ranges: &[reference::Range]) -> bool {
    ranges.iter().all(|r| kjv_sword::kjv::book(r.book).is_none() || verse_set(std::slice::from_ref(r)).is_some())
}

/// Can the shown text be read on its own? Not if it has no number ("Canticles"), or a roman
/// numeral for a chapter (MHC writes "Isa. i. 13"), which the reference parser does not read.
fn checkable(shown: &str) -> bool {
    if !shown.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    let mut words = shown.split(|c: char| !c.is_alphabetic()).filter(|w| !w.is_empty());
    words.next();
    !words.any(|w| w.chars().all(|c| matches!(c.to_ascii_lowercase(), 'i' | 'v' | 'x' | 'l' | 'c')))
}

struct Conv {
    opts: Options,
    blocks: Vec<Block>,
    cur: Option<(Kind, Vec<Item>)>,
    /// Inside a footnote or another inline-only container (a heading, list item, cell).
    inline_depth: u32,
    in_fn: u32,
    stats: Stats,
}

type Res<T = ()> = Result<T, String>;

const TITLE_TYPES: &[&str] = &["x-s", "x-s2", "x-s3", "x-s4", "x-ms", "main", "x-IS"];

fn check_attrs(el: &El, allowed: &[&str]) -> Res {
    for (k, _) in &el.attrs {
        if !allowed.contains(&k.as_str()) {
            return Err(format!("unexpected attribute {k:?} on {}", el.describe()));
        }
    }
    Ok(())
}

impl Conv {
    fn count(&mut self, what: impl Into<String>) {
        *self.stats.ignored.entry(what.into()).or_default() += 1;
    }

    fn drop_data(&mut self, what: impl Into<String>) {
        *self.stats.dropped_data.entry(what.into()).or_default() += 1;
    }

    fn flush(&mut self) {
        if let Some((kind, items)) = self.cur.take() {
            let items = tidy(items);
            if !items.is_empty() {
                self.blocks.push(Block::Flow(kind, items));
            }
        }
    }

    fn begin(&mut self, kind: Kind) {
        self.flush();
        self.cur = Some((kind, Vec::new()));
    }

    fn push(&mut self, item: Item) {
        if self.cur.is_none() {
            self.cur = Some((Kind::P, Vec::new()));
        }
        self.cur.as_mut().unwrap().1.push(item);
    }

    fn text(&mut self, t: &str) {
        if self.cur.is_none() && t.bytes().all(is_ws) {
            return;
        }
        self.push(Item::Text(t.to_string()));
    }

    fn inline(&mut self, tag: Tag, kids: &[Node]) -> Res {
        self.push(Item::Open(tag.clone()));
        self.walk(kids)?;
        self.push(Item::Close(tag));
        Ok(())
    }

    /// Runs `f` with the current block set aside (for a cell's content).
    fn collect_inline(&mut self, kids: &[Node], context: &str) -> Res<Vec<Item>> {
        let saved = self.cur.take();
        self.cur = Some((Kind::P, Vec::new()));
        self.inline_depth += 1;
        let r = self.walk(kids);
        self.inline_depth -= 1;
        let (_, items) = self.cur.take().unwrap();
        self.cur = saved;
        r.map_err(|e| format!("{e} (inside {context})"))?;
        Ok(tidy(items))
    }

    fn walk(&mut self, nodes: &[Node]) -> Res {
        for n in nodes {
            match n {
                Node::Text(t) => self.text(t),
                Node::El(e) => self.element(e)?,
            }
        }
        Ok(())
    }

    /// Paragraph-level boundary elements are only meaningful between blocks. Inside a
    /// footnote a paragraph break becomes a line break; anywhere else inline-only it is an
    /// error.
    fn boundary(&mut self, el: &El, starts: bool) -> Res {
        if self.inline_depth == 0 {
            if starts {
                self.begin(Kind::P);
            } else {
                self.flush();
            }
            return Ok(());
        }
        if self.in_fn > 0 && self.inline_depth == self.in_fn {
            if starts {
                self.push(Item::Br);
            }
            // (counted by kind: the milestones' ids are different every time)
            let kind = el.attr("type").map_or(String::new(), |t| format!(" type={t}"));
            self.count(format!("<{}{kind}> inside a footnote", el.name));
            return Ok(());
        }
        Err(format!("{} where only inline content can be", el.describe()))
    }

    fn element(&mut self, e: &El) -> Res {
        let thml = self.opts.dialect == Dialect::Thml;
        let kids = &e.kids[..];
        match (e.name.as_str(), thml) {
            // ---- shared inline styles ----
            ("i", _) => {
                check_attrs(e, &[])?;
                self.inline(Tag::I, kids)
            }
            ("b", _) => {
                check_attrs(e, &[])?;
                self.inline(Tag::B, kids)
            }
            ("sup", _) => {
                check_attrs(e, &[])?;
                self.inline(Tag::Sup, kids)
            }
            // ---- OSIS ----
            ("div", false) => self.div(e),
            ("chapter", false) => {
                check_attrs(e, &["n", "osisID", "sID", "eID"])?;
                if !kids.is_empty() {
                    return Err(format!("{} has content", e.describe()));
                }
                self.count("chapter milestone");
                Ok(())
            }
            ("lg", false) => {
                check_attrs(e, &["sID", "eID"])?;
                if !kids.is_empty() {
                    return self.walk(kids);
                }
                self.count("lg milestone");
                Ok(())
            }
            ("l", false) => {
                check_attrs(e, &["level", "sID", "eID"])?;
                if e.attr("eID").is_some() && kids.is_empty() {
                    if self.inline_depth > 0 {
                        return Err(format!("{} where only inline content can be", e.describe()));
                    }
                    self.flush();
                } else if e.attr("sID").is_some() && kids.is_empty() {
                    if self.inline_depth > 0 {
                        return Err(format!("{} where only inline content can be", e.describe()));
                    }
                    self.begin(Kind::L);
                } else {
                    if self.inline_depth > 0 {
                        return Err(format!("{} where only inline content can be", e.describe()));
                    }
                    self.begin(Kind::L);
                    self.inline_depth += 1;
                    let r = self.walk(kids);
                    self.inline_depth -= 1;
                    r?;
                    self.flush();
                }
                Ok(())
            }
            ("title", false) => {
                check_attrs(e, &["type"])?;
                if let Some(t) = e.attr("type")
                    && !TITLE_TYPES.contains(&t)
                {
                    return Err(format!("unknown title type in {}", e.describe()));
                }
                if self.inline_depth > 0 {
                    if self.in_fn > 0 && self.inline_depth == self.in_fn {
                        self.push(Item::Br);
                        self.count("title inside a footnote");
                        self.walk(kids)?;
                        return self.push_br();
                    }
                    return Err(format!("{} where only inline content can be", e.describe()));
                }
                self.begin(Kind::H);
                self.inline_depth += 1;
                let r = self.walk(kids);
                self.inline_depth -= 1;
                r?;
                self.flush();
                Ok(())
            }
            ("milestone", false) => {
                check_attrs(e, &["type", "n"])?;
                match e.attr("type") {
                    Some(t @ ("x-usfm-toc1" | "x-usfm-toc2" | "x-usfm-toc3")) => {
                        if e.attr("n").is_some_and(|n| !n.is_empty()) {
                            self.drop_data(format!("milestone {t} n= (book title in an attribute)"));
                        }
                        self.count(format!("milestone {t}"));
                        Ok(())
                    }
                    Some("x-p") => self.boundary(e, true),
                    _ => Err(format!("unknown milestone {}", e.describe())),
                }
            }
            ("hi", false) => {
                check_attrs(e, &["type"])?;
                let tag = match e.attr("type") {
                    Some("italic" | "underline") => Tag::I,
                    Some("bold") => Tag::B,
                    Some("super") => Tag::Sup,
                    Some("small-caps") => Tag::Sc,
                    _ => return Err(format!("unknown style {}", e.describe())),
                };
                self.inline(tag, kids)
            }
            ("transChange", false) => {
                check_attrs(e, &["type"])?;
                self.inline(Tag::I, kids)
            }
            ("q", false) => {
                check_attrs(e, &["who", "marker", "level", "type", "sID", "eID"])?;
                if e.attr("sID").is_some() || e.attr("eID").is_some() {
                    return Err(format!("quotation milestone {} is not supported", e.describe()));
                }
                // The marks a reader would see are in `marker`, not in the text
                if e.attr("marker").is_some_and(|m| !m.is_empty()) {
                    return Err(format!("{} has quotation marks that are not in its text", e.describe()));
                }
                self.walk(kids)
            }
            ("foreign", false) => {
                check_attrs(e, &["xml:lang", "type"])?;
                let code = e.attr("xml:lang").ok_or_else(|| format!("{} has no xml:lang", e.describe()))?;
                if code.is_empty() || !code.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-') {
                    return Err(format!("odd language code in {}", e.describe()));
                }
                self.inline(Tag::Lang(code.to_string()), kids)
            }
            ("lb", false) => {
                check_attrs(e, &["type"])?;
                self.push(Item::Br);
                self.walk(kids)
            }
            ("note", false) => {
                check_attrs(e, &["placement", "type", "n", "resp"])?;
                self.stats.footnotes += 1;
                self.push(Item::Open(Tag::Fn));
                self.in_fn += 1;
                self.inline_depth += 1;
                let r = self.walk(kids);
                self.inline_depth -= 1;
                self.in_fn -= 1;
                r?;
                self.push(Item::Close(Tag::Fn));
                Ok(())
            }
            ("reference", false) => {
                check_attrs(e, &["osisRef", "type"])?;
                let link = match e.attr("osisRef") {
                    None => Link::Unresolved,
                    Some(v) => self.osis_link(v, &e.text()),
                };
                self.reference(e, link)
            }
            ("list", false) => {
                check_attrs(e, &[])?;
                if self.inline_depth > 0 {
                    return Err(format!("{} where only inline content can be", e.describe()));
                }
                self.flush();
                self.count("list");
                self.walk(kids)
            }
            ("item", false) => {
                check_attrs(e, &["type"])?;
                if self.inline_depth > 0 {
                    return Err(format!("{} where only inline content can be", e.describe()));
                }
                self.begin(Kind::Li);
                self.inline_depth += 1;
                let r = self.walk(kids);
                self.inline_depth -= 1;
                r?;
                self.flush();
                Ok(())
            }
            ("table", false) => {
                check_attrs(e, &[])?;
                if self.inline_depth > 0 {
                    return Err(format!("{} where only inline content can be", e.describe()));
                }
                self.flush();
                self.count("table");
                self.walk(kids)
            }
            ("row", false) => {
                check_attrs(e, &[])?;
                if self.inline_depth > 0 {
                    return Err(format!("{} where only inline content can be", e.describe()));
                }
                self.flush();
                let mut cells = Vec::new();
                for k in kids {
                    match k {
                        Node::Text(t) if t.bytes().all(is_ws) => {}
                        Node::Text(_) => return Err("text directly inside <row>".into()),
                        Node::El(c) if c.name == "cell" => {
                            check_attrs(c, &["role"])?;
                            if let Some(role) = c.attr("role") {
                                if role != "label" {
                                    return Err(format!("unknown cell role in {}", c.describe()));
                                }
                                self.drop_data("cell role=label");
                            }
                            cells.push(self.collect_inline(&c.kids, "<cell>")?);
                        }
                        Node::El(c) => return Err(format!("{} inside <row>", c.describe())),
                    }
                }
                if cells.iter().any(|c| !c.is_empty()) {
                    self.blocks.push(Block::Row(cells));
                }
                Ok(())
            }
            // ---- ThML ----
            ("br", true) => {
                check_attrs(e, &[])?;
                if !kids.is_empty() {
                    return Err(format!("{} has content", e.describe()));
                }
                self.push(Item::Br);
                Ok(())
            }
            ("p", true) => {
                check_attrs(e, &[])?;
                if self.inline_depth > 0 {
                    return Err(format!("{} where only inline content can be", e.describe()));
                }
                self.begin(Kind::P);
                self.walk(kids)?;
                self.flush();
                Ok(())
            }
            ("scripRef", true) => {
                check_attrs(e, &["passage"])?;
                let source = e.attr("passage").map(str::to_string).unwrap_or_else(|| e.text());
                let read = reference::parse_with(&source, RefOptions {
                    jud_is_judges: self.opts.jud_is_judges,
                    context: self.opts.context.map(|(book, chapter)| Context { book, chapter }),
                    // the Treasury's `*marg:` `*title` `*Gr:` labels (no other ThML source has a `*`)
                    starred_labels: true,
                });
                let link = match read {
                    Err(_) => Link::Unresolved,
                    Ok(r) if possible(&r) => Link::To(join_osis(&r)),
                    Ok(_) => {
                        self.stats.refs_impossible.push(source.clone());
                        Link::Withheld
                    }
                };
                self.reference(e, link)
            }
            ("sync", true) => {
                check_attrs(e, &["type", "value"])?;
                if e.attr("type") != Some("Strongs") || !kids.is_empty() {
                    return Err(format!("unknown {}", e.describe()));
                }
                self.drop_data("sync type=Strongs value=");
                Ok(())
            }
            _ => Err(format!("unknown element {}", e.describe())),
        }
    }

    fn push_br(&mut self) -> Res {
        self.push(Item::Br);
        Ok(())
    }

    /// The `to` for an OSIS reference: the source's `osisRef`, unless what the reader sees
    /// contradicts it. The `osisRef`s of JFB and KD were generated from the printed text and
    /// are wrong where it has a list or a range of chapters: "Mt 6:1-7; 9" has
    /// `osisRef="Matt.6.1-Matt.7.9"` and "Gen 12-14" has `osisRef="Gen.12.14"`. So the shown
    /// text is read independently (when it can be: see [`checkable`]), and the two readings
    /// of one book are compared verse by verse:
    ///
    /// * they cover the same verses, or the text is a whole chapter and the `osisRef` a
    ///   narrower passage in it ("John 1." for 1:1-18): the `osisRef`;
    /// * the text is a range of chapters a to b and the `osisRef` is the verse a:b, the
    ///   source's collapse of the range ("Gen 12-14" for Gen.12.14): the text's reading;
    /// * anything else: no `to` (the text is kept exactly; the build lists each case).
    fn osis_link(&mut self, osis_ref: &str, shown: &str) -> Link {
        let Ok(from_source) = reference::from_osis(osis_ref) else {
            return Link::Unresolved;
        };
        let source_to = join_osis(&from_source);
        if !possible(&from_source) {
            self.stats.refs_impossible.push(format!("{osis_ref} ({shown})"));
            return Link::Withheld;
        }
        if !checkable(shown) {
            return Link::To(source_to);
        }
        let options = RefOptions { jud_is_judges: self.opts.jud_is_judges, ..RefOptions::default() };
        let Ok(from_text) = reference::parse_with(shown, options) else {
            return Link::To(source_to);
        };
        // A different book is a different abbreviation read, not a different verse
        let books = |r: &[reference::Range]| r.iter().map(|r| r.book).collect::<BTreeSet<_>>();
        if books(&from_source) != books(&from_text) {
            return Link::To(source_to);
        }
        let (Some(in_source), Some(in_text)) = (verse_set(&from_source), verse_set(&from_text)) else {
            return Link::To(source_to);
        };
        if in_source == in_text {
            return Link::To(source_to);
        }
        if let ([s], [t]) = (from_source.as_slice(), from_text.as_slice()) {
            // the text is one whole chapter and the source a narrower passage in it
            if t.is_whole_chapters() && t.start.0 == t.end.0 && in_source.is_subset(&in_text) {
                return Link::To(source_to);
            }
            // the text is chapters a to b and the source says verse b of chapter a
            if s.is_single_verse() && t.is_whole_chapters() && t.start.0 < t.end.0 && s.start == (t.start.0, t.end.0) {
                if !possible(&from_text) {
                    self.stats.refs_impossible.push(format!("{osis_ref} ({shown})"));
                    return Link::Withheld;
                }
                self.stats.refs_corrected.push(format!("{osis_ref} ({shown})"));
                return Link::To(join_osis(&from_text));
            }
        }
        self.stats.refs_withheld.push(format!("{osis_ref} ({shown})"));
        Link::Withheld
    }

    fn reference(&mut self, e: &El, link: Link) -> Res {
        let to = match link {
            Link::To(to) => {
                self.stats.refs_resolved += 1;
                Some(to)
            }
            Link::Withheld => None,
            Link::Unresolved => {
                let shown = e.attr("osisRef").or_else(|| e.attr("passage")).map(str::to_string).unwrap_or_else(|| e.text());
                let text = e.text();
                // Record what the source cited: the attribute if there is one, and the shown text
                self.stats.unparsed.push(if shown == text || e.attr("osisRef").is_none() && e.attr("passage").is_none() {
                    text
                } else {
                    format!("{shown} ({text})")
                });
                None
            }
        };
        self.inline(Tag::Ref(to), &e.kids)
    }

    fn div(&mut self, e: &El) -> Res {
        check_attrs(e, &["type", "sID", "eID", "subType", "osisID", "canonical", "annotateRef", "annotateType"])?;
        let kind = e.attr("type").ok_or_else(|| format!("{} has no type", e.describe()))?;
        let milestone = e.kids.is_empty() && (e.attr("sID").is_some() || e.attr("eID").is_some());
        match kind {
            "x-p" => {
                if milestone {
                    self.boundary(e, e.attr("sID").is_some())
                } else {
                    self.boundary(e, true)?;
                    self.walk(&e.kids)?;
                    self.boundary(e, false)
                }
            }
            "introduction" | "book" | "section" | "x-milestone" => {
                if !milestone && !e.kids.is_empty() {
                    // a container: its content is the note's
                    self.count(format!("{} container", e.describe()));
                    return self.walk(&e.kids);
                }
                self.count(format!("div type={kind}"));
                Ok(())
            }
            _ => Err(format!("unknown {}", e.describe())),
        }
    }
}

/// Converts one note's raw markup to the note markup. The body is empty when the source
/// had no text (only structure).
pub fn convert(raw: &str, opts: Options) -> Result<(String, Stats), String> {
    let mut stats = Stats::default();
    let toks = tokenize(raw, opts.dialect, &mut stats)?;
    let tree = build_tree(toks)?;
    let mut conv = Conv { opts, blocks: Vec::new(), cur: None, inline_depth: 0, in_fn: 0, stats, };
    conv.walk(&tree)?;
    conv.flush();
    let body = write_blocks(&conv.blocks);
    Ok((body, conv.stats))
}

// ------------------------------------------------------------------------------------
// Inventory
// ------------------------------------------------------------------------------------

/// Counts every element with its attribute names (and the values of the attributes that
/// classify elements), for the build report.
pub fn inventory(raw: &str, dialect: Dialect, into: &mut BTreeMap<String, u64>) -> Result<(), String> {
    let mut stats = Stats::default();
    for t in tokenize(raw, dialect, &mut stats)? {
        let (name, attrs, kind) = match &t {
            Tok::Open(n, a) => (n, a.as_slice(), ""),
            Tok::Empty(n, a) => (n, a.as_slice(), "/"),
            Tok::Close(n) => {
                *into.entry(format!("</{n}>")).or_default() += 1;
                continue;
            }
            Tok::Text(_) => continue,
        };
        let mut names: Vec<&str> = attrs.iter().map(|(k, _)| k.as_str()).collect();
        names.sort_unstable();
        *into.entry(format!("<{name}{kind}> [{}]", names.join(" "))).or_default() += 1;
        for (k, v) in attrs {
            if matches!(k.as_str(), "type" | "subType" | "level" | "role" | "placement" | "canonical" | "annotateType" | "xml:lang") {
                *into.entry(format!("<{name}{kind}> {k}={v}")).or_default() += 1;
            }
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------
// Proof
// ------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sym {
    Ch(char),
    /// Whitespace in the text.
    Gap,
    /// A block boundary or `<br/>`: separates, but is not whitespace.
    Soft,
}

/// The source's text content: tags removed, entities decoded, whitespace as gaps. This
/// scans the raw text itself rather than reusing the tokenizer's output.
fn source_stream(raw: &str, dialect: Dialect) -> Result<Vec<Sym>, String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let rest = &raw[i..];
        let c = rest.chars().next().unwrap();
        match c {
            '<' => match scan_tag(rest) {
                Some((_, n)) => {
                    i += n;
                    continue;
                }
                None if dialect == Dialect::Thml => out.push(Sym::Ch('<')),
                None => return Err(format!("not a tag at: {}", context(raw, i))),
            },
            '&' => match scan_entity(rest) {
                Some((d, n)) => {
                    out.push(sym(d));
                    i += n;
                    continue;
                }
                None if dialect == Dialect::Thml => out.push(Sym::Ch('&')),
                None => return Err(format!("not an entity at: {}", context(raw, i))),
            },
            c => out.push(sym(c)),
        }
        i += c.len_utf8();
    }
    Ok(out)
}

fn sym(c: char) -> Sym {
    if c.is_ascii() && is_ws(c as u8) { Sym::Gap } else { Sym::Ch(c) }
}

const BLOCKS: &[&str] = &["p", "h", "l", "li", "tr", "td"];
const INLINES: &[&str] = &["i", "b", "sup", "sc", "lang", "ref", "fn"];

/// The converted body's text content, checking that the body is well formed in the closed
/// markup: known tags only, properly nested, blocks only at the top (cells inside rows),
/// `&`, `<`, `>` escaped.
fn body_stream(body: &str) -> Result<Vec<Sym>, String> {
    let mut out = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut i = 0;
    while i < body.len() {
        let rest = &body[i..];
        let c = rest.chars().next().unwrap();
        match c {
            '<' => {
                let (tok, n) = scan_tag(rest).ok_or_else(|| format!("bad tag in body at: {}", context(body, i)))?;
                match tok {
                    Tok::Open(name, attrs) => {
                        let ok_attrs = match name.as_str() {
                            "lang" => attrs.len() == 1 && attrs[0].0 == "code",
                            "ref" => attrs.is_empty() || (attrs.len() == 1 && attrs[0].0 == "to"),
                            _ => attrs.is_empty(),
                        };
                        if !ok_attrs {
                            return Err(format!("bad attributes on <{name}> in body"));
                        }
                        if BLOCKS.contains(&name.as_str()) {
                            let parent = stack.last().map(String::as_str);
                            let allowed = match name.as_str() {
                                "td" => parent == Some("tr"),
                                "tr" | "p" | "h" | "l" | "li" => parent.is_none(),
                                _ => false,
                            };
                            if !allowed {
                                return Err(format!("<{name}> inside {parent:?} in body"));
                            }
                            out.push(Sym::Soft);
                        } else if INLINES.contains(&name.as_str()) {
                            match stack.last().map(String::as_str) {
                                None | Some("tr") => return Err(format!("inline <{name}> outside a block")),
                                _ => {}
                            }
                        } else {
                            return Err(format!("unknown <{name}> in body"));
                        }
                        stack.push(name);
                    }
                    Tok::Close(name) => {
                        if stack.pop().as_deref() != Some(name.as_str()) {
                            return Err(format!("</{name}> does not match in body"));
                        }
                        if BLOCKS.contains(&name.as_str()) {
                            out.push(Sym::Soft);
                        }
                    }
                    Tok::Empty(name, attrs) => {
                        if name != "br" || !attrs.is_empty() {
                            return Err(format!("unknown <{name}/> in body"));
                        }
                        if !matches!(stack.last().map(String::as_str), Some(n) if n != "tr") {
                            return Err("<br/> outside a block".into());
                        }
                        out.push(Sym::Soft);
                    }
                    Tok::Text(_) => unreachable!(),
                }
                i += n;
                continue;
            }
            '&' => {
                let (d, n) = match rest {
                    r if r.starts_with("&amp;") => ('&', 5),
                    r if r.starts_with("&lt;") => ('<', 4),
                    r if r.starts_with("&gt;") => ('>', 4),
                    _ => return Err(format!("unescaped & in body at: {}", context(body, i))),
                };
                out.push(sym(d));
                i += n;
                continue;
            }
            '>' => return Err(format!("unescaped > in body at: {}", context(body, i))),
            c => {
                if matches!(stack.last().map(String::as_str), None | Some("tr")) {
                    return Err(format!("text outside a block in body at: {}", context(body, i)));
                }
                out.push(sym(c));
            }
        }
        i += c.len_utf8();
    }
    if let Some(open) = stack.last() {
        return Err(format!("<{open}> is never closed in body"));
    }
    Ok(out)
}

/// Collapses runs of gaps and softs; trims the ends.
fn squash(v: Vec<Sym>) -> Vec<Sym> {
    let mut out: Vec<Sym> = Vec::with_capacity(v.len());
    for s in v {
        match (s, out.last_mut()) {
            (Sym::Ch(_), _) => out.push(s),
            (Sym::Gap, Some(last @ (Sym::Soft | Sym::Gap))) => *last = Sym::Gap,
            (Sym::Soft, Some(Sym::Soft | Sym::Gap)) => {}
            _ => out.push(s),
        }
    }
    while matches!(out.last(), Some(Sym::Gap | Sym::Soft)) {
        out.pop();
    }
    let lead = out.iter().take_while(|s| !matches!(s, Sym::Ch(_))).count();
    out.drain(..lead);
    out
}

/// What the proof found.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Proof {
    /// Non-whitespace characters, identical on both sides.
    pub chars: u64,
    /// Gaps added at block boundaries and line breaks.
    pub structural_gaps: u64,
}

fn show(v: &[Sym], at: usize) -> String {
    let a = at.saturating_sub(25);
    let b = (at + 25).min(v.len());
    v[a..b]
        .iter()
        .map(|s| match s {
            Sym::Ch(c) => c.to_string(),
            Sym::Gap => "·".into(),
            Sym::Soft => "¶".into(),
        })
        .collect()
}

/// Proves the converted `body` has exactly the text of the `raw` source (see the module
/// documentation). The error names the first difference.
pub fn prove(raw: &str, body: &str, dialect: Dialect) -> Result<Proof, String> {
    let src = squash(source_stream(raw, dialect)?);
    let conv = squash(body_stream(body)?);
    let mut proof = Proof::default();
    let (mut i, mut j) = (0, 0);
    while i < src.len() || j < conv.len() {
        let fail = |why: &str| {
            format!(
                "{why} at source symbol {i} / converted symbol {j}:\n    source:    …{}…\n    converted: …{}…",
                show(&src, i),
                show(&conv, j)
            )
        };
        match (src.get(i), conv.get(j)) {
            (Some(Sym::Ch(a)), Some(Sym::Ch(b))) if a == b => {
                proof.chars += 1;
                i += 1;
                j += 1;
            }
            (Some(Sym::Ch(_)), Some(Sym::Ch(_))) => return Err(fail("a character differs")),
            (Some(Sym::Gap), Some(Sym::Gap | Sym::Soft)) => {
                i += 1;
                j += 1;
            }
            (Some(Sym::Ch(_)), Some(Sym::Soft)) => {
                proof.structural_gaps += 1;
                j += 1;
            }
            (Some(Sym::Gap), Some(Sym::Ch(_))) => return Err(fail("the source separates words the conversion joins")),
            (Some(Sym::Ch(_)), Some(Sym::Gap)) => return Err(fail("the conversion separates characters the source joins")),
            (Some(_), None) => return Err(fail("the converted text ends early")),
            (None, Some(_)) => return Err(fail("the converted text has extra content")),
            (Some(Sym::Soft), _) => unreachable!("source text has no soft gaps"),
            (None, None) => break,
        }
    }
    Ok(proof)
}

/// The text content of a converted body as plain text, whitespace collapsed (for tests
/// and for counting characters).
#[cfg_attr(not(test), allow(dead_code))]
pub fn body_text(body: &str) -> Result<String, String> {
    let v = squash(body_stream(body)?);
    Ok(v.into_iter()
        .map(|s| match s {
            Sym::Ch(c) => c,
            Sym::Gap | Sym::Soft => ' ',
        })
        .collect())
}

/// The source's text content as plain text, whitespace collapsed.
#[cfg_attr(not(test), allow(dead_code))]
pub fn source_text(raw: &str, dialect: Dialect) -> Result<String, String> {
    let v = squash(source_stream(raw, dialect)?);
    Ok(v.into_iter()
        .map(|s| match s {
            Sym::Ch(c) => c,
            Sym::Gap | Sym::Soft => ' ',
        })
        .collect())
}

/// The text content as the tokenizer sees it (used to cross-check `source_stream`).
#[cfg_attr(not(test), allow(dead_code))]
pub fn tokenized_text(raw: &str, dialect: Dialect) -> Result<String, String> {
    let mut stats = Stats::default();
    let mut s = String::new();
    for t in tokenize(raw, dialect, &mut stats)? {
        if let Tok::Text(t) = t {
            s.push_str(&t);
        }
    }
    Ok(s)
}
