//! The commentaries in `data/library/commentaries/`, checked against their sources.
//!
//! For every module this reads the pinned source with `kjv-sword`, reads the written
//! files, and checks, with text extraction written here and not shared with the converter:
//!
//! * every distinct entry, with the orphans that follow it, is in exactly one note, under its
//!   own verse and range (trimmed where the module's settings say so, and listed);
//! * every orphan byte is in exactly one note, and no note has any other source;
//! * each note's text, with markup removed, is the source's: the same characters in the same
//!   order, whitespace aside;
//! * every body is well formed in the closed markup (an unknown element would be an error);
//! * the files are in the canonical form (key order, escapes, line endings) and `index.toml`
//!   states the same counts;
//! * known notes are where they belong.
//!
//! The sources are in `.cache/sources/` (git-ignored). When they are not there the checks are
//! skipped, with a note.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use kjv_import::commentaries::{self, Entry as Config, Note, book_file};
use kjv_import::markup;
use kjv_library::reference::{END, from_osis};
use kjv_import::{cache, library};
use kjv_sword::{Extraction, Module, kjv};

// ---------------------------------------------------------------------------------------
// Text extraction, independent of the converter
// ---------------------------------------------------------------------------------------

/// The length of the tag at the start of `s` (which starts with `<`), if it is one: a name,
/// then any number of `name="value"` (or `'value'`) attributes, then `>` or `/>`. Anything
/// else after a `<` (the Treasury's `<See definition 1497|,]`) is text.
fn tag_len(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let name_char = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'_' | b':' | b'.' | b'-');
    let ws = |c: u8| matches!(c, b' ' | b'\t' | b'\n' | b'\r');
    let mut i = 1;
    if b.get(i) == Some(&b'/') {
        i += 1;
    }
    if !b.get(i)?.is_ascii_alphabetic() && *b.get(i)? != b'_' {
        return None;
    }
    while b.get(i).is_some_and(|&c| name_char(c)) {
        i += 1;
    }
    loop {
        let before = i;
        while b.get(i).is_some_and(|&c| ws(c)) {
            i += 1;
        }
        match b.get(i)? {
            b'>' => return Some(i + 1),
            b'/' if b.get(i + 1) == Some(&b'>') => return Some(i + 2),
            &c if i > before && (c.is_ascii_alphabetic() || c == b'_') => {
                while b.get(i).is_some_and(|&c| name_char(c)) {
                    i += 1;
                }
                while b.get(i).is_some_and(|&c| ws(c)) {
                    i += 1;
                }
                if b.get(i)? != &b'=' {
                    return None;
                }
                i += 1;
                while b.get(i).is_some_and(|&c| ws(c)) {
                    i += 1;
                }
                let q = *b.get(i)?;
                if q != b'"' && q != b'\'' {
                    return None;
                }
                i += 1;
                loop {
                    match *b.get(i)? {
                        c if c == q => break,
                        b'<' => return None,
                        _ => i += 1,
                    }
                }
                i += 1;
            }
            _ => return None,
        }
    }
}

/// The character an entity at the start of `s` (which starts with `&`) stands for.
fn entity(s: &str) -> Option<(char, usize)> {
    let end = s.bytes().take(12).position(|b| b == b';')?;
    let name = &s[1..end];
    let c = match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        _ => {
            let n = match name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
                Some(h) => u32::from_str_radix(h, 16).ok()?,
                None => name.strip_prefix('#')?.parse().ok()?,
            };
            char::from_u32(n).filter(|&c| c != '\0')?
        }
    };
    Some((c, end + 1))
}

/// A source's text content: tags removed, entities decoded.
fn source_chars(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        let c = raw[i..].chars().next().unwrap();
        if c == '<' {
            if let Some(n) = tag_len(&raw[i..]) {
                i += n;
                continue;
            }
        } else if c == '&'
            && let Some((d, n)) = entity(&raw[i..])
        {
            out.push(d);
            i += n;
            continue;
        }
        out.push(c);
        i += c.len_utf8();
    }
    out
}

/// A converted body's text content: tags removed, `&amp;` `&lt;` `&gt;` decoded. Any other
/// `&` or a `>` that is not a tag's end is a failure.
fn body_chars(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut i = 0;
    while i < body.len() {
        let c = body[i..].chars().next().unwrap();
        match c {
            '<' => {
                let n = tag_len(&body[i..]).unwrap_or_else(|| panic!("not a tag in {:.80}", &body[i..]));
                i += n;
                continue;
            }
            '&' => {
                let (d, n) = if body[i..].starts_with("&amp;") {
                    ('&', 5)
                } else if body[i..].starts_with("&lt;") {
                    ('<', 4)
                } else if body[i..].starts_with("&gt;") {
                    ('>', 4)
                } else {
                    panic!("unescaped & in {:.80}", &body[i..]);
                };
                out.push(d);
                i += n;
                continue;
            }
            '>' => panic!("unescaped > in {:.80}", &body[..i + 1]),
            _ => out.push(c),
        }
        i += c.len_utf8();
    }
    out
}

/// Everything but the whitespace the converter is allowed to collapse (ASCII space, tab,
/// newline, carriage return).
fn solid(s: &str) -> String {
    s.chars().filter(|c| !matches!(c, ' ' | '\t' | '\n' | '\r')).collect()
}

// ---------------------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------------------

type Pos = (u32, u32);

#[derive(Debug, Clone)]
struct Out {
    from: Pos,
    to: Pos,
    body: String,
}

fn pos(s: &str) -> Pos {
    let (c, v) = s.split_once(':').unwrap_or_else(|| panic!("{s:?} is not chapter:verse"));
    (c.parse().unwrap(), v.parse().unwrap())
}

/// What the module declares and the files hold.
struct Loaded {
    config: Config,
    ex: Extraction,
    out: BTreeMap<&'static str, Vec<Out>>,
    index: toml::Table,
}

impl Loaded {
    fn notes(&self, book: &str) -> &[Out] {
        self.out.get(book).map_or(&[], Vec::as_slice)
    }

    /// The notes of `book` whose range contains the verse.
    fn covering(&self, book: &str, chapter: u32, verse: u32) -> Vec<&Out> {
        self.notes(book).iter().filter(|n| n.from <= (chapter, verse) && (chapter, verse) <= n.to).collect()
    }

    fn all(&self) -> impl Iterator<Item = (&'static str, &Out)> + '_ {
        self.out.iter().flat_map(|(b, v)| v.iter().map(move |n| (*b, n)))
    }
}

fn load(id: &str) -> Option<Loaded> {
    let config = commentaries::catalogue().unwrap().into_iter().find(|c| c.id == id).unwrap_or_else(|| panic!("no commentary {id}"));
    let zip = cache().join(&config.source);
    if !zip.exists() {
        eprintln!("skipping {id}: {} is not in .cache/sources (run `kjv-import fetch`)", config.source);
        return None;
    }
    let module = Module::open_zip(&zip).unwrap();
    let ex = module.read().unwrap();
    let dir = library().join("commentaries").join(id);
    let mut out: BTreeMap<&'static str, Vec<Out>> = BTreeMap::new();
    let mut seen_files = 0;
    for f in fs::read_dir(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())).flatten() {
        let name = f.file_name().to_string_lossy().into_owned();
        let Some(code) = name.strip_suffix(".jsonl") else { continue };
        seen_files += 1;
        let book = kjv::book(code).unwrap_or_else(|| panic!("{id}/{name}: not a book of the KJV")).code;
        let bytes = fs::read(f.path()).unwrap();
        assert!(!bytes.contains(&b'\r'), "{id}/{name} has a carriage return");
        let text = String::from_utf8(bytes).unwrap_or_else(|_| panic!("{id}/{name} is not UTF-8"));
        assert!(text.ends_with('\n') && !text.is_empty(), "{id}/{name} does not end with a newline");
        let mut notes = Vec::new();
        for (n, line) in text.split('\n').enumerate() {
            if n == text.matches('\n').count() {
                assert!(line.is_empty());
                break;
            }
            let v: serde_json::Value = serde_json::from_str(line).unwrap_or_else(|e| panic!("{id}/{name} line {}: {e}", n + 1));
            let obj = v.as_object().unwrap();
            assert!(obj.keys().all(|k| matches!(k.as_str(), "from" | "to" | "body")), "{id}/{name} line {}: keys {:?}", n + 1, obj.keys().collect::<Vec<_>>());
            let from = pos(obj["from"].as_str().unwrap());
            let to = obj.get("to").map_or(from, |t| pos(t.as_str().unwrap()));
            if let Some(t) = obj.get("to") {
                assert_ne!(pos(t.as_str().unwrap()), from, "{id}/{name} line {}: a `to` equal to `from`", n + 1);
            }
            assert!(from <= to, "{id}/{name} line {}: range runs backwards", n + 1);
            notes.push(Out { from, to, body: obj["body"].as_str().unwrap().to_string() });
        }
        // canonical order, one note per start, and the file is exactly what the writer makes
        for w in notes.windows(2) {
            assert!(w[0].from < w[1].from, "{id}/{name}: {:?} then {:?}", w[0].from, w[1].from);
        }
        let as_notes: Vec<Note> = notes.iter().map(|n| Note { book, from: n.from, to: n.to, body: n.body.clone() }).collect();
        let refs: Vec<&Note> = as_notes.iter().collect();
        assert!(book_file(&refs) == text, "{id}/{name} is not in the canonical form");
        out.insert(book, notes);
    }
    assert!(seen_files > 0, "{id}: no book files");
    let index: toml::Table = fs::read_to_string(dir.join("index.toml")).unwrap().parse().unwrap();
    Some(Loaded { config, ex, out, index })
}

fn int(t: &toml::Table, key: &str) -> u64 {
    t[key].as_integer().unwrap_or_else(|| panic!("index.toml: {key}")) as u64
}

// ---------------------------------------------------------------------------------------
// The checks
// ---------------------------------------------------------------------------------------

struct Expected {
    to: Pos,
    /// the source's text content (entities decoded, tags removed) of the entry and the
    /// orphans that follow it
    chars: String,
}

fn check(m: &Loaded) {
    let id = m.config.id.as_str();
    let ex = &m.ex;
    let repeated: BTreeSet<usize> = ex.repeats.iter().map(|r| r.entry).collect();

    // Repeats are copies: identical to what they repeat, and never themselves repeated.
    for r in &ex.repeats {
        assert_eq!(ex.entries[r.entry].text, ex.entries[r.first].text, "{id}: a repeat differs from its original");
        assert!(!repeated.contains(&r.first), "{id}: a repeat of a repeat");
    }

    // Every orphan follows an entry that is kept, and is attached to exactly that entry.
    let mut attached: BTreeMap<usize, Vec<&kjv_sword::Orphan>> = BTreeMap::new();
    for o in &ex.orphans {
        let host = o.after.unwrap_or_else(|| panic!("{id}: an orphan follows no entry: {:.60}", o.text));
        assert!(!repeated.contains(&host), "{id}: an orphan follows a repeat");
        attached.entry(host).or_default().push(o);
    }
    for list in attached.values_mut() {
        list.sort_by_key(|o| (o.testament, o.block, o.offset));
    }
    let orphan_total: usize = attached.values().map(Vec::len).sum();
    assert_eq!(orphan_total, ex.orphans.len(), "{id}: an orphan was attached twice or not at all");

    // What each distinct entry should have become.
    let mut expected: BTreeMap<(&'static str, Pos), Expected> = BTreeMap::new();
    let mut distinct = 0usize;
    let mut source_solid = 0usize;
    let mut source_bytes = 0usize;
    for (i, e) in ex.entries.iter().enumerate() {
        if repeated.contains(&i) {
            continue;
        }
        distinct += 1;
        let mut chars = source_chars(&e.text);
        source_bytes += e.text.len();
        for o in attached.get(&i).into_iter().flatten() {
            chars.push_str(&source_chars(&o.text));
            source_bytes += o.text.len();
        }
        source_solid += solid(&chars).chars().count();
        let key = (e.book, (e.chapter, e.verse));
        let to = (e.to_chapter, e.to_verse);
        assert!(expected.insert(key, Expected { to, chars }).is_none(), "{id}: two distinct entries at {key:?}");
    }
    assert_eq!(source_bytes, ex.entries.iter().enumerate().filter(|(i, _)| !repeated.contains(i)).map(|(_, e)| e.text.len()).sum::<usize>() + ex.orphans.iter().map(|o| o.text.len()).sum::<usize>());

    // Every note is one of them, once, with its text and range.
    let trim = m.config.trim_cross_chapter;
    let mut found: BTreeSet<(&'static str, Pos)> = BTreeSet::new();
    let mut note_solid = 0usize;
    let mut trimmed_seen = 0usize;
    for (book, n) in m.all() {
        let key = (book, n.from);
        let want = expected.get(&key).unwrap_or_else(|| panic!("{id}: a note at {book} {:?} has no source entry", n.from));
        assert!(found.insert(key), "{id}: two notes at {key:?}");
        // the range
        if trim && want.to.0 != n.from.0 {
            let last = kjv::book(book).unwrap().verses[n.from.0 as usize - 1] as u32;
            assert_eq!(n.to, (n.from.0, last), "{id} {book} {:?}: a cross-chapter range is trimmed to its chapter's end", n.from);
            trimmed_seen += 1;
        } else {
            assert_eq!(n.to, want.to, "{id} {book} {:?}: the range", n.from);
        }
        // the text: the body must be well formed in the closed markup (this fails on unknown
        // markup), and have the source's characters in order
        markup::body_text(&n.body).unwrap_or_else(|e| panic!("{id} {book} {:?}: the body is not in the note markup: {e}", n.from));
        let got = solid(&body_chars(&n.body));
        let src = solid(&want.chars);
        if got != src {
            let at = got.chars().zip(src.chars()).take_while(|(a, b)| a == b).count();
            let show = |s: &str| s.chars().skip(at.saturating_sub(30)).take(70).collect::<String>();
            panic!("{id} {book} {:?}: the note's text differs from the source's at character {at}:\n  note:   {}\n  source: {}", n.from, show(&got), show(&src));
        }
        note_solid += got.chars().count();
        links_exist(id, book, n);
        assert!(!got.is_empty(), "{id} {book} {:?}: an empty note was written", n.from);
        // the body has no empty element and no whitespace at its edges
        assert!(!n.body.contains("<p></p>") && !n.body.contains("<i></i>"), "{id} {book} {:?}: an empty element", n.from);
    }
    // Every entry with text has its note; the others have only structure.
    let mut without_text = 0usize;
    for ((book, at), want) in &expected {
        if !found.contains(&(*book, *at)) {
            without_text += 1;
            assert_eq!(solid(&want.chars), "", "{id}: {book} {at:?} has text but no note: {:.80}", want.chars);
        }
    }
    assert_eq!(found.len() + without_text, distinct, "{id}: every distinct entry is a note or has no text");
    assert_eq!(note_solid, source_solid, "{id}: the notes hold every character of the source");

    // index.toml agrees
    let idx = &m.index;
    assert_eq!(int(idx, "notes") as usize, found.len(), "{id}: index notes");
    assert_eq!(int(idx, "books") as usize, m.out.len(), "{id}: index books");
    assert_eq!(int(idx, "characters") as usize, note_solid, "{id}: index characters");
    assert_eq!(int(idx, "source_entries") as usize, distinct, "{id}: index source_entries");
    assert_eq!(int(idx, "repeats_dropped") as usize, ex.repeats.len(), "{id}: index repeats");
    assert_eq!(int(idx, "orphans_merged") as usize, ex.orphans.len(), "{id}: index orphans");
    assert_eq!(int(idx, "orphan_bytes") as usize, ex.orphans.iter().map(|o| o.text.len()).sum::<usize>(), "{id}: index orphan bytes");
    assert_eq!(int(idx, "notes_without_text") as usize, without_text, "{id}: index notes_without_text");
    let books = idx["book"].as_array().unwrap();
    assert_eq!(books.len(), m.out.len());
    for b in books {
        let b = b.as_table().unwrap();
        let code = b["code"].as_str().unwrap();
        assert_eq!(b["notes"].as_integer().unwrap() as usize, m.notes(code).len(), "{id} {code}: index notes");
    }

    // Trimmed ranges are listed exactly, and each is a gap fill: it ends before the next
    // note begins (the module linked the note to every verse up to the next one).
    let listed = idx["trimmed"].as_array().unwrap();
    assert_eq!(listed.len(), trimmed_seen, "{id}: every trimmed range is listed");
    if !trim {
        assert_eq!(trimmed_seen, 0);
        assert!(listed.is_empty());
    } else {
        let mut by_book: BTreeMap<&str, Vec<(Pos, Pos)>> = BTreeMap::new();
        for ((book, at), want) in &expected {
            by_book.entry(book).or_default().push((*at, want.to));
        }
        let mut adjacent = 0usize;
        for t in listed {
            let t = t.as_table().unwrap();
            let book = t["book"].as_str().unwrap();
            let (from, was_to, to) = (pos(t["from"].as_str().unwrap()), pos(t["was_to"].as_str().unwrap()), pos(t["to"].as_str().unwrap()));
            assert_eq!(expected[&(kjv::book(book).unwrap().code, from)].to, was_to, "{id}: {book} {from:?} was linked to {was_to:?}");
            assert!(was_to.0 > from.0 && to.0 == from.0);
            let starts = &by_book[book];
            let next = starts.iter().map(|(s, _)| *s).find(|s| *s > from).unwrap_or_else(|| panic!("{id} {book} {from:?}: nothing follows the cross-chapter note"));
            assert!(next > was_to, "{id} {book} {from:?}: the next note {next:?} begins inside the range up to {was_to:?}");
            let last = kjv::book(book).unwrap().verses[was_to.0 as usize - 1] as u32;
            let after = if was_to.1 >= last { (was_to.0 + 1, 0) } else { (was_to.0, was_to.1 + 1) };
            if next == after || (after.1 == 0 && next == (after.0, 1)) {
                adjacent += 1;
            }
        }
        eprintln!("{id}: {} cross-chapter ranges trimmed; {adjacent} end exactly one verse before the next note", listed.len());
        assert_eq!(adjacent, listed.len(), "{id}: every trimmed range ends exactly where the next note begins");
    }
}

/// Every `to` is well formed OSIS that names verses the KJV has (a source typo such as
/// "Mic.35" is left without one).
fn links_exist(id: &str, book: &str, n: &Out) {
    let mut rest = n.body.as_str();
    while let Some(at) = rest.find("<ref to=\"") {
        rest = &rest[at + 9..];
        let end = rest.find('"').unwrap();
        let to = &rest[..end];
        let ranges = from_osis(to).unwrap_or_else(|e| panic!("{id} {book} {:?}: to={to:?}: {e}", n.from));
        for r in ranges {
            let Some(b) = kjv::book(r.book) else { continue }; // not in the KJV: the Apocrypha
            let last_chapter = b.verses.len() as u32;
            let verses = |c: u32| u32::from(b.verses[c as usize - 1]);
            let (end_c, end_v) = if r.end.0 == END { (last_chapter, verses(last_chapter)) } else { (r.end.0, if r.end.1 == END { verses(r.end.0.min(last_chapter)) } else { r.end.1 }) };
            assert!(r.start.0 >= 1 && end_c <= last_chapter, "{id} {book} {:?}: to={to:?} names a chapter {book} does not have", n.from);
            assert!(r.start.1 <= verses(r.start.0) && end_v <= verses(end_c), "{id} {book} {:?}: to={to:?} names a verse {book} does not have", n.from);
        }
        rest = &rest[end..];
    }
}

fn text_of(n: &Out) -> String {
    body_chars(&n.body)
}

// ---------------------------------------------------------------------------------------
// One test per module (they run side by side)
// ---------------------------------------------------------------------------------------

#[test]
fn mhc() {
    let Some(m) = load("mhc") else { return };
    check(&m);
    let notes = m.covering("JHN", 3, 16);
    assert!(!notes.is_empty());
    assert!(notes.iter().any(|n| text_of(n).contains("God so loved")), "Matthew Henry on John 3:16");
    assert!(m.out.len() == 66, "Matthew Henry covers every book");
}

#[test]
fn catena() {
    let Some(m) = load("catena") else { return };
    check(&m);
    assert!(!m.covering("MAT", 1, 1).is_empty(), "Catena Aurea has Matthew 1:1");
    assert_eq!(m.out.keys().copied().collect::<Vec<_>>(), ["JHN", "LUK", "MAT", "MRK"]);
    // the editor's footnotes are kept, in place
    assert!(m.all().any(|(_, n)| n.body.contains("<fn>")));
}

#[test]
fn wesley() {
    let Some(m) = load("wesley") else { return };
    check(&m);
    // the module repeats the last note of the book before at the start of a book it has
    // no notes for; the copies are dropped
    for b in ["JDG", "JON"] {
        assert!(m.notes(b).is_empty(), "Wesley has no notes in {b}: {:?}", m.notes(b).iter().map(|n| n.from).collect::<Vec<_>>());
    }
    assert_eq!(m.ex.repeats.len(), 31);
    assert_eq!(int(&m.index, "repeats_dropped"), 31);
    // cross-chapter: all 191 trimmed
    assert_eq!(m.index["trimmed"].as_array().unwrap().len(), 191);
    assert!(m.all().all(|(_, n)| n.from.0 == n.to.0), "no Wesley note crosses a chapter boundary");
    // Gen 12:20 stays in chapter 12
    let n = m.covering("GEN", 12, 20);
    assert_eq!(n.len(), 1);
    assert_eq!(n[0].to, (12, 20));
    assert!(m.covering("GEN", 13, 1).is_empty());
    // the references are relative to the note
    let any_ref = m.all().any(|(_, n)| n.body.contains("<ref to=\""));
    assert!(any_ref);
}

#[test]
fn kd() {
    let Some(m) = load("kd") else { return };
    check(&m);
    assert!(!m.covering("GEN", 1, 1).is_empty(), "Keil & Delitzsch on Genesis 1:1");
    assert_eq!(m.out.len(), 39, "the Old Testament only");
    assert!(m.all().any(|(_, n)| n.body.contains("<tr><td>")));
}

#[test]
fn jfb() {
    let Some(m) = load("jfb") else { return };
    check(&m);
    let notes = m.covering("JHN", 3, 16);
    assert!(!notes.is_empty());
    assert!(notes.iter().any(|n| text_of(n).contains("For God so loved")), "JFB on John 3:16 (it is an orphan in the module)");
    assert_eq!(m.out.len(), 66);
}

#[test]
fn tsk() {
    let Some(m) = load("tsk") else { return };
    check(&m);
    let notes = m.covering("GEN", 1, 1);
    assert!(!notes.is_empty());
    assert!(notes.iter().any(|n| n.body.contains("<ref to=\"") && n.body.contains("JHN.1.1")), "Treasury on Genesis 1:1 links John 1:1");
    // 64 stale copies at the start of books are dropped
    assert_eq!(m.ex.repeats.len(), 64);
    assert_eq!(m.out.len(), 66);
}

#[test]
fn gill() {
    let Some(m) = load("gill") else { return };
    check(&m);
    // Gill keeps the Hebrew (the other copies strip it)
    let hebrew = |s: &str| s.chars().any(|c| ('\u{5d0}'..='\u{5ea}').contains(&c));
    let chapter3: Vec<&Out> = m.notes("JHN").iter().filter(|n| n.from.0 == 3).collect();
    assert!(!chapter3.is_empty());
    assert!(chapter3.iter().any(|n| hebrew(&text_of(n))), "Hebrew in Gill's John 3");
    assert_eq!(m.out.len(), 66);
    // the module marks up nothing but paragraphs: no references, no notes
    assert!(m.all().all(|(_, n)| !n.body.contains("<ref") && !n.body.contains("<fn>")));
}

/// The odd characters the sources hold are kept exactly as stored.
#[test]
fn oddities_are_kept_as_stored() {
    // C1 control characters in the Treasury at Numbers 35:4, as stored (Latin-1 bytes 0x80..=0x9f)
    if let Some(m) = load("tsk") {
        let n = m.covering("NUM", 35, 4);
        let has_c1 = n.iter().any(|n| text_of(n).chars().any(|c| ('\u{80}'..='\u{9f}').contains(&c)));
        let in_source = m.ex.entries.iter().any(|e| e.book == "NUM" && e.chapter == 35 && e.verse == 4 && e.text.chars().any(|c| ('\u{80}'..='\u{9f}').contains(&c)));
        assert_eq!(has_c1, in_source, "the C1 controls of Numbers 35:4 are in the data exactly when they are in the source");
    }
    // U+FFFD in Gill at Ezekiel 45:1, and the U+0089s
    if let Some(m) = load("gill") {
        let in_source = |c: char| m.ex.entries.iter().map(|e| &e.text).chain(m.ex.orphans.iter().map(|o| &o.text)).any(|t| t.contains(c));
        for c in ['\u{fffd}', '\u{89}'] {
            let in_data = m.all().any(|(_, n)| text_of(n).contains(c));
            assert_eq!(in_data, in_source(c), "{c:?} is in Gill's data exactly when it is in the source");
        }
        let n = m.covering("EZK", 45, 1);
        assert!(n.iter().any(|n| text_of(n).contains('\u{fffd}')), "Gill at Ezekiel 45:1 keeps its U+FFFD");
    }
}
