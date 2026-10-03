//! The library's translations for the app's interface: the catalogue, and any
//! chapter of any translation with its heading and neighbours.

use kjv_library::alignment::Ref;
use kjv_library::books::{self, Section};
use kjv_library::view::ChapterView;
use kjv_library::{BibleInfo, Library};
use serde::Serialize;

use crate::api::ChapterRef;
use crate::bundle::DataBundle;

#[derive(Debug, Serialize)]
pub struct BibleSummary {
    pub id: String,
    pub abbr: String,
    pub name: String,
    pub year: String,
    pub group: String,
    pub licence: String,
    pub credit: String,
    pub about: String,
    pub books: Vec<BibleBook>,
}

#[derive(Debug, Serialize)]
pub struct BibleBook {
    /// USFM code ("1SA"), as Scripture references in notes use
    pub code: String,
    /// The app's key ("First Samuel")
    pub name: String,
    /// "1 Samuel"
    pub display: String,
    pub abbr: String,
    /// "old", "apocrypha", or "new"
    pub section: &'static str,
    /// The translation's own name for the book ("Kings I")
    pub title: String,
    pub chapters: usize,
    /// Its chapter numbers (not always 1..=chapters)
    pub numbers: Vec<u32>,
}

fn section_key(s: Section) -> &'static str {
    match s {
        Section::Old => "old",
        Section::Apocrypha => "apocrypha",
        Section::New => "new",
    }
}

pub fn bibles(lib: &Library) -> Vec<BibleSummary> {
    lib.bibles()
        .iter()
        .map(|b| BibleSummary {
            id: b.id.clone(),
            abbr: b.abbr.clone(),
            name: b.name.clone(),
            year: b.year.clone(),
            group: b.group.clone(),
            licence: b.licence.clone(),
            credit: b.credit.clone(),
            about: b.about.clone(),
            books: b
                .books
                .iter()
                .filter_map(|e| {
                    let k = books::by_code(&e.code)?;
                    Some(BibleBook {
                        code: k.code.to_string(),
                        name: k.name.to_string(),
                        display: k.display.to_string(),
                        abbr: k.abbr.to_string(),
                        section: section_key(k.section),
                        title: e.name.clone(),
                        chapters: e.chapters,
                        numbers: e.numbers.clone(),
                    })
                })
                .collect(),
        })
        .collect()
}

#[derive(Debug, Serialize)]
pub struct BibleChapter {
    pub bible: String,
    pub abbr: String,
    /// The app's book key ("Psalms")
    pub book: String,
    pub chapter: u32,
    /// "Psalm 23", "1 Samuel 3"
    pub heading: String,
    #[serde(flatten)]
    pub view: ChapterView,
    pub prev: Option<ChapterRef>,
    pub next: Option<ChapterRef>,
}

fn heading(display: &str, code: &str, chapter: u32) -> String {
    if code == "PSA" { format!("Psalm {}", chapter) } else { format!("{} {}", display, chapter) }
}

/// The chapter numbers book `code` of translation `info` has, in order.
fn chapter_numbers(lib: &Library, info: &BibleInfo, code: &str) -> Result<Vec<u32>, String> {
    Ok(lib.book(&info.id, code)?.chapters.iter().map(|c| c.number).collect())
}

/// Chapter `chapter` of `book` (the app's key, e.g. "First Samuel") in translation `bible`.
pub fn chapter(lib: &Library, bible: &str, book: &str, chapter: u32) -> Result<BibleChapter, String> {
    let info = lib.bible(bible).ok_or_else(|| format!("no translation {:?}", bible))?;
    let k = books::by_name(book).ok_or_else(|| format!("no book named {:?}", book))?;
    let view = lib.chapter(bible, k.code, chapter)?;

    let position = info.books.iter().position(|b| b.code == k.code).ok_or_else(|| format!("{} has no {}", info.abbr, k.display))?;
    let numbers = chapter_numbers(lib, info, k.code)?;
    let at = numbers.iter().position(|&n| n == chapter).unwrap_or(0);
    let neighbour = |code: &str, number: u32| -> ChapterRef {
        ChapterRef { book: books::by_code(code).map_or(code, |b| b.name).to_string(), chapter: number }
    };
    let prev = if at > 0 {
        Some(neighbour(k.code, numbers[at - 1]))
    } else if position > 0 {
        let code = &info.books[position - 1].code;
        chapter_numbers(lib, info, code)?.last().map(|&n| neighbour(code, n))
    } else {
        None
    };
    let next = if at + 1 < numbers.len() {
        Some(neighbour(k.code, numbers[at + 1]))
    } else if let Some(b) = info.books.get(position + 1) {
        chapter_numbers(lib, info, &b.code)?.first().map(|&n| neighbour(&b.code, n))
    } else {
        None
    };

    Ok(BibleChapter {
        bible: info.id.clone(),
        abbr: info.abbr.clone(),
        book: k.name.to_string(),
        chapter,
        heading: heading(k.display, k.code, chapter),
        view,
        prev,
        next,
    })
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Mapped {
    /// The app's book key
    pub book: String,
    pub chapter: u32,
    /// The verse as numbered there ("16", "1-2"; "0" a Psalm title)
    pub verse: String,
}

/// Verse `number` of `code` `chapter` as translation `id` numbers it: the verse
/// itself, or the bridged verse that holds it ("1-2").
fn resolve(lib: &Library, id: &str, code: &str, chapter: u32, number: String) -> Result<Ref, String> {
    let exact = (code.to_string(), chapter, number);
    if lib.has_verse(id, &exact) {
        return Ok(exact);
    }
    let Ok(n) = exact.2.parse::<u32>() else { return Ok(exact) };
    let b = lib.book(id, code)?;
    Ok(kjv_library::usfm::verses(&b)
        .into_iter()
        .find(|v| {
            v.chapter == chapter
                && v.number.split_once('-').is_some_and(|(lo, hi)| {
                    lo.parse::<u32>().is_ok_and(|lo| lo <= n) && hi.parse::<u32>().is_ok_and(|hi| n <= hi)
                })
        })
        .map(|v| (code.to_string(), chapter, v.number))
        .unwrap_or(exact))
}

/// Where verse `verse` of `book` `chapter` in translation `from` is in translation
/// `to`: the first corresponding verse, or None if `to` has no counterpart. Verse 0
/// stands for the chapter (its first verse is mapped).
pub fn map(lib: &Library, from: &str, to: &str, book: &str, chapter: u32, verse: u32) -> Result<Option<Mapped>, String> {
    let k = books::by_name(book).ok_or_else(|| format!("no book named {:?}", book))?;
    let number = if verse > 0 {
        verse.to_string()
    } else {
        // The chapter's first verse (or its title)
        let b = lib.book(from, k.code)?;
        let first = kjv_library::usfm::verses(&b).into_iter().find(|v| v.chapter == chapter);
        match first {
            Some(v) => v.number,
            None => return Ok(None),
        }
    };
    let r = resolve(lib, from, k.code, chapter, number)?;
    let found = lib.map(from, to, &r)?;
    Ok(found.into_iter().next().and_then(|(code, c, v)| {
        Some(Mapped { book: books::by_code(&code)?.name.to_string(), chapter: c, verse: v })
    }))
}

// ---------------------------------------------------------------- commentaries

#[derive(Debug, Serialize)]
pub struct CommentaryNotes {
    pub id: String,
    pub name: String,
    pub author: String,
    pub tradition: String,
    pub credit: String,
    pub notes: Vec<NoteView>,
}

#[derive(Debug, Serialize)]
pub struct NoteView {
    /// "John 3:14-16", "John 3 (introduction)", "John (introduction)"
    pub label: String,
    pub body: String,
}

pub(crate) fn note_label(display: &str, code: &str, from: (u32, u32), to: (u32, u32)) -> String {
    let head = |c: u32| if code == "PSA" { format!("Psalm {}", c) } else { format!("{} {}", display, c) };
    match (from, to) {
        ((0, _), _) => format!("{} (introduction)", display),
        ((c, 0), (c2, 0)) if c == c2 => format!("{} (introduction)", head(c)),
        ((c, v), (c2, v2)) if (c, v) == (c2, v2) => format!("{}:{}", head(c), v),
        ((c, v), (c2, v2)) if c == c2 => format!("{}:{}-{}", head(c), v, v2),
        ((c, v), (c2, v2)) => format!("{}:{}-{}:{}", head(c), v, c2, v2),
    }
}

#[derive(Debug, Serialize)]
pub struct NotesOn {
    /// Where the notes are, in the KJV's numbering that every commentary follows
    /// ("Psalm 23:4" for the Douay-Rheims' Psalm 22:4)
    pub kjv: String,
    /// Whether that is the place as the translation numbers it
    pub same: bool,
    pub commentaries: Vec<CommentaryNotes>,
}

/// The KJV chapters that chapter `chapter` of `code` in translation `bible`
/// corresponds to: each that holds at least a quarter of its verses (so the
/// Douay-Rheims' Psalm 9 is the KJV's 9 and 10, but the WEB's Romans 14, which
/// prints the doxology the KJV has at 16:25-27, is only the KJV's 14).
fn kjv_chapters(lib: &Library, bible: &str, code: &str, chapter: u32) -> Result<Vec<(String, u32)>, String> {
    let b = lib.book(bible, code)?;
    let verses: Vec<String> =
        kjv_library::usfm::verses(&b).into_iter().filter(|v| v.chapter == chapter).map(|v| v.number).collect();
    let mut counts: Vec<((String, u32), usize)> = Vec::new();
    for n in &verses {
        let mut seen: Vec<(String, u32)> = Vec::new();
        for (kc, c, _) in lib.map(bible, "kjv", &(code.to_string(), chapter, n.clone()))? {
            if seen.contains(&(kc.clone(), c)) {
                continue;
            }
            seen.push((kc.clone(), c));
            match counts.iter_mut().find(|(k, _)| k.0 == kc && k.1 == c) {
                Some((_, count)) => *count += 1,
                None => counts.push(((kc, c), 1)),
            }
        }
    }
    let total = verses.len().max(1);
    Ok(counts.into_iter().filter(|(_, count)| count * 4 >= total).map(|(k, _)| k).collect())
}

/// "Psalm 23:4", "Psalm 9; Psalm 10", "Song of Three Children 1:1-2"
fn places_label(places: &[(String, u32, u32)]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut i = 0;
    while i < places.len() {
        let (code, c, _) = &places[i];
        let display = books::by_code(code).map_or(code.as_str(), |b| b.display);
        let head = if code == "PSA" { format!("Psalm {}", c) } else { format!("{} {}", display, c) };
        let mut verses: Vec<u32> = Vec::new();
        while i < places.len() && &places[i].0 == code && places[i].1 == *c {
            verses.push(places[i].2);
            i += 1;
        }
        verses.retain(|&v| v > 0);
        verses.sort();
        verses.dedup();
        if verses.is_empty() {
            parts.push(head);
            continue;
        }
        let mut runs: Vec<String> = Vec::new();
        let mut j = 0;
        while j < verses.len() {
            let start = verses[j];
            while j + 1 < verses.len() && verses[j + 1] == verses[j] + 1 {
                j += 1;
            }
            runs.push(if verses[j] == start { start.to_string() } else { format!("{}-{}", start, verses[j]) });
            j += 1;
        }
        parts.push(format!("{}:{}", head, runs.join(", ")));
    }
    parts.join("; ")
}

/// The notes of commentaries `ids` on verse `verse` of `book` `chapter` as numbered in
/// translation `bible` (every commentary is keyed to the KJV, so the verse is mapped
/// to the KJV first). Verse 0: the chapter's introductions (those of the KJV
/// chapters it corresponds to), and with a book's first chapter, the book's.
pub fn notes(lib: &Library, ids: &[String], bible: &str, book: &str, chapter: u32, verse: u32) -> Result<NotesOn, String> {
    let k = books::by_name(book).ok_or_else(|| format!("no book named {:?}", book))?;
    let places: Vec<(String, u32, u32)> = if verse == 0 {
        kjv_chapters(lib, bible, k.code, chapter)?.into_iter().map(|(code, c)| (code, c, 0)).collect()
    } else {
        let r = resolve(lib, bible, k.code, chapter, verse.to_string())?;
        lib.map(bible, "kjv", &r)?
            .into_iter()
            .filter_map(|(code, c, v)| Some((code, c, v.split('-').next()?.parse().ok()?)))
            .collect()
    };
    let same = places.len() == 1 && places[0] == (k.code.to_string(), chapter, verse);
    // A book's introduction comes before its first chapter's
    let mut lookups: Vec<(String, u32, u32)> = Vec::new();
    for p in &places {
        if verse == 0 && p.1 == 1 {
            lookups.push((p.0.clone(), 0, 0));
        }
        lookups.push(p.clone());
    }
    let mut out = Vec::new();
    for id in ids {
        let info = lib.commentaries().iter().find(|c| &c.id == id).ok_or_else(|| format!("no commentary {:?}", id))?;
        let mut notes: Vec<NoteView> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (code, c, v) in &lookups {
            let display = books::by_code(code).map_or(code.as_str(), |b| b.display);
            for n in lib.notes_on(id, code, *c, *v)? {
                // The same note reached from two places once (a commentary may have two
                // notes on one passage: Tyndale's book summary and introduction)
                if seen.insert((code.clone(), n.from, n.to, n.body.clone())) {
                    notes.push(NoteView { label: note_label(display, code, n.from, n.to), body: n.body });
                }
            }
        }
        out.push(CommentaryNotes {
            id: info.id.clone(),
            name: info.name.clone(),
            author: info.author.clone(),
            tradition: info.tradition.clone(),
            credit: info.credit.clone(),
            notes,
        });
    }
    Ok(NotesOn { kjv: places_label(&places), same, commentaries: out })
}

// ---------------------------------------------------------------- cross-references

#[derive(Debug, Serialize)]
pub struct CrossrefsOn {
    /// Where the references are from, in the KJV's numbering they follow
    pub kjv: String,
    /// Whether that is the place as the translation numbers it
    pub same: bool,
    pub collections: Vec<CollectionRefs>,
}

#[derive(Debug, Serialize)]
pub struct CollectionRefs {
    pub id: String,
    pub name: String,
    pub short: String,
    pub credit: String,
    /// How many places it gives from here (more than `lines` holds when cut short)
    pub total: usize,
    pub lines: Vec<RefLine>,
}

#[derive(Debug, Serialize)]
pub struct RefLine {
    /// A keyword ("God."), a remark, or nothing
    pub text: String,
    pub refs: Vec<Passage>,
}

/// A place, as the translation being read has it.
#[derive(Debug, Serialize)]
pub struct Passage {
    /// In the KJV's numbering, as the collection gives it: "ROM.5.8", "2CO.5.19-2CO.5.21"
    pub to: String,
    /// Where it is in the translation (the app's book key), to open it; None when the
    /// translation hasn't it
    pub book: Option<String>,
    pub chapter: u32,
    pub verse: u32,
    /// "Romans 5:8", "2 Corinthians 5:19-21": numbered as the translation numbers it
    /// (as the KJV does when the translation hasn't it)
    pub label: String,
    /// Its verses there: (number, text). Where the translation hasn't the place (the
    /// New Testament in an Old Testament translation, a verse it leaves out), the KJV's
    pub verses: Vec<(String, String)>,
    /// The verses are the KJV's, since the translation hasn't the place
    pub from_kjv: bool,
    /// A long range is cut short after its first few verses
    pub more: bool,
    pub votes: Option<i32>,
}

/// Verses of a range given in full; the rest is left for opening it.
const RANGE_VERSES: usize = 3;

/// "LUK.2.14" -> ("LUK", 2, "14")
fn osis_place(s: &str) -> Option<Ref> {
    let mut it = s.split('.');
    let (code, c, v) = (it.next()?, it.next()?.parse().ok()?, it.next()?);
    Some((code.to_string(), c, v.to_string()))
}

fn place_label(code: &str, c: u32, v: &str) -> String {
    let display = books::by_code(code).map_or(code, |b| b.display);
    if code == "PSA" { format!("Psalm {}:{}", c, v) } else { format!("{} {}:{}", display, c, v) }
}

/// The verses of `code` from `start` to `end` (chapter, number) in translation `bible`,
/// at most `max` of them, and whether any were left out. The KJV's own books come from
/// the app's KJV, so they read exactly as its reader shows them.
fn verse_texts(data: &DataBundle, lib: &Library, bible: &str, code: &str, start: (u32, &str), end: (u32, &str), max: usize) -> Result<(Vec<(String, String)>, bool), String> {
    let name = books::by_code(code).map(|b| b.name);
    let core = if bible == "kjv" { name.and_then(|n| data.bible.books.iter().find(|b| b.name == n)) } else { None };
    let list: Vec<(u32, String, String)> = match core {
        Some(b) => b
            .chapters
            .iter()
            .filter(|ch| ch.number >= start.0 && ch.number <= end.0)
            .flat_map(|ch| {
                ch.superscription
                    .iter()
                    .map(|s| (ch.number, "0".to_string(), s.text.clone()))
                    .chain(ch.verses.iter().map(|v| (ch.number, v.verse_number.to_string(), v.text.clone())))
                    .collect::<Vec<_>>()
            })
            .collect(),
        None => lib
            .verses(bible, code)?
            .iter()
            .filter(|v| v.chapter >= start.0 && v.chapter <= end.0)
            .map(|v| (v.chapter, v.number.clone(), v.text.clone()))
            .collect(),
    };
    let Some(from) = list.iter().position(|(c, n, _)| *c == start.0 && n == start.1) else { return Ok((Vec::new(), false)) };
    let to = list.iter().rposition(|(c, n, _)| *c == end.0 && n == end.1).filter(|&i| i >= from).unwrap_or(from);
    let span = &list[from..=to];
    let shown = span.iter().take(max).map(|(_, n, t)| (n.clone(), t.clone())).collect();
    Ok((shown, span.len() > max))
}

/// Place `to` (KJV numbering: "ROM.5.8", a range, or a whole chapter, "ECC.7") as
/// translation `bible` has it.
pub(crate) fn passage(data: &DataBundle, lib: &Library, bible: &str, to: &str, votes: Option<i32>) -> Result<Passage, String> {
    // A whole chapter: its verses from the first, shown and opened as the chapter
    if let [code, c] = to.split('.').collect::<Vec<_>>()[..]
        && let Ok(c) = c.parse::<u32>()
    {
        let last = lib.verses("kjv", code)?.iter().rfind(|v| v.chapter == c && !v.title).map(|v| v.number.clone());
        let Some(last) = last else { return Err(format!("bad place {:?}", to)) };
        let mut whole = passage(data, lib, bible, &format!("{code}.{c}.1-{code}.{c}.{last}"), votes)?;
        whole.to = to.to_string();
        whole.verse = 0;
        let display = books::by_code(code).map_or(code, |b| b.display);
        let shown = if whole.book.is_some() { whole.chapter } else { c };
        whole.label = if code == "PSA" { format!("Psalm {}", shown) } else { format!("{} {}", display, shown) };
        return Ok(whole);
    }
    let (a, b) = match to.split_once('-') {
        Some((a, b)) => (a, Some(b)),
        None => (to, None),
    };
    let start = osis_place(a).ok_or_else(|| format!("bad place {:?}", to))?;
    let end = b.and_then(osis_place);
    let mut out = Passage {
        to: to.to_string(),
        book: None,
        chapter: start.1,
        verse: 0,
        label: String::new(),
        verses: Vec::new(),
        from_kjv: false,
        more: false,
        votes,
    };
    let known = |r: &Ref| lib.has_verse("kjv", r);
    let kjv_label = || match &end {
        Some(e) if e.0 == start.0 && e.1 == start.1 => format!("{}-{}", place_label(&start.0, start.1, &start.2), e.2),
        Some(e) if e.0 == start.0 => format!("{}-{}:{}", place_label(&start.0, start.1, &start.2), e.1, e.2),
        Some(e) => format!("{} - {}", place_label(&start.0, start.1, &start.2), place_label(&e.0, e.1, &e.2)),
        None => place_label(&start.0, start.1, &start.2),
    };
    if !known(&start) {
        out.label = kjv_label();
        return Ok(out);
    }
    // Where it is in the translation: the first place its start maps to, through to the
    // last place its end maps to in the same book
    let Some(here) = lib.map("kjv", bible, &start)?.into_iter().next() else {
        // Not there: the KJV's words, so the reference can still be read
        out.label = kjv_label();
        let last = end.as_ref().filter(|e| e.0 == start.0).unwrap_or(&start);
        let (verses, cut) = verse_texts(data, lib, "kjv", &start.0, (start.1, &start.2), (last.1, &last.2), RANGE_VERSES)?;
        out.verses = verses;
        out.from_kjv = bible != "kjv";
        out.more = cut || matches!(&end, Some(e) if e.0 != start.0);
        return Ok(out);
    };
    // (The end only where it falls after the start: a translation that reorders
    // chapters, as the Septuagint's Jeremiah does, can put it before)
    let first = |n: &str| n.split('-').next().and_then(|x| x.parse::<u32>().ok()).unwrap_or(0);
    let there = match &end {
        Some(e) if e.0 == start.0 => lib
            .map("kjv", bible, e)?
            .into_iter()
            .rev()
            .find(|r| r.0 == here.0)
            .filter(|r| (r.1, first(&r.2)) >= (here.1, first(&here.2))),
        _ => None,
    };
    let last = there.clone().unwrap_or_else(|| here.clone());
    let (verses, cut) = verse_texts(data, lib, bible, &here.0, (here.1, &here.2), (last.1, &last.2), RANGE_VERSES)?;
    out.book = books::by_code(&here.0).map(|b| b.name.to_string());
    out.chapter = here.1;
    out.verse = here.2.split('-').next().and_then(|v| v.parse().ok()).unwrap_or(0);
    out.label = match &there {
        Some(t) if *t != here && t.1 == here.1 => format!("{}-{}", place_label(&here.0, here.1, &here.2), t.2.rsplit('-').next().unwrap_or(&t.2)),
        Some(t) if *t != here => format!("{}-{}:{}", place_label(&here.0, here.1, &here.2), t.1, t.2.rsplit('-').next().unwrap_or(&t.2)),
        _ => place_label(&here.0, here.1, &here.2),
    };
    // A range into another book (2 John 1:1 to 3 John 1:14), or whose end isn't found
    // after its start, is shown from its start
    out.more = cut || (end.is_some() && there.is_none());
    out.verses = verses;
    Ok(out)
}

/// The cross-references of collections `ids` from verse `verse` of `book` `chapter`
/// as numbered in translation `bible`, each place given as that translation has it. A
/// list (not the Treasury) gives at most `limit` places, most helpful first.
#[allow(clippy::too_many_arguments)]
pub fn crossrefs(data: &DataBundle, lib: &Library, ids: &[String], bible: &str, book: &str, chapter: u32, verse: u32, limit: Option<usize>) -> Result<CrossrefsOn, String> {
    let k = books::by_name(book).ok_or_else(|| format!("no book named {:?}", book))?;
    let places: Vec<(String, u32, u32)> = if verse == 0 {
        Vec::new()
    } else {
        let r = resolve(lib, bible, k.code, chapter, verse.to_string())?;
        lib.map(bible, "kjv", &r)?
            .into_iter()
            .filter_map(|(code, c, v)| Some((code, c, v.split('-').next()?.parse().ok()?)))
            .filter(|p| p.2 > 0)
            .collect()
    };
    let same = places.len() == 1 && places[0] == (k.code.to_string(), chapter, verse);
    let mut collections = Vec::new();
    for id in ids {
        let info = lib.crossrefs().iter().find(|c| &c.id == id).ok_or_else(|| format!("no cross-references {:?}", id))?;
        let mut lines: Vec<kjv_library::crossrefs::Line> = Vec::new();
        for (code, c, v) in &places {
            lines.extend(lib.crossrefs_from(id, code, *c, *v)?);
        }
        if info.commentary.is_none() && lines.len() > 1 {
            // A list from several places (a bridged verse): one list, most helpful first
            let mut refs: Vec<kjv_library::crossrefs::Target> = Vec::new();
            for t in lines.drain(..).flat_map(|l| l.refs) {
                if !refs.iter().any(|r| r.to == t.to) {
                    refs.push(t);
                }
            }
            refs.sort_by_key(|t| std::cmp::Reverse(t.votes));
            lines.push(kjv_library::crossrefs::Line { text: String::new(), refs });
        }
        let total = lines.iter().map(|l| l.refs.len()).sum();
        let mut out_lines = Vec::new();
        for line in lines {
            let mut refs = line.refs;
            if info.commentary.is_none()
                && let Some(n) = limit
            {
                refs.truncate(n);
            }
            let refs = refs.iter().map(|t| passage(data, lib, bible, &t.to, t.votes)).collect::<Result<Vec<_>, _>>()?;
            out_lines.push(RefLine { text: line.text, refs });
        }
        collections.push(CollectionRefs {
            id: info.id.clone(),
            name: info.name.clone(),
            short: info.short.clone(),
            credit: info.credit.clone(),
            total,
            lines: out_lines,
        });
    }
    Ok(CrossrefsOn { kjv: places_label(&places), same, collections })
}

#[cfg(test)]
mod tests {
    use super::places_label;

    fn p(code: &str, c: u32, v: u32) -> (String, u32, u32) {
        (code.to_string(), c, v)
    }

    #[test]
    fn place_labels() {
        assert_eq!(places_label(&[p("PSA", 23, 4)]), "Psalm 23:4");
        assert_eq!(places_label(&[p("PSA", 9, 0), p("PSA", 10, 0)]), "Psalm 9; Psalm 10");
        assert_eq!(places_label(&[p("JHN", 3, 16), p("JHN", 3, 17), p("JHN", 3, 19)]), "John 3:16-17, 19");
        assert_eq!(places_label(&[p("1SA", 1, 1)]), "1 Samuel 1:1");
        assert_eq!(places_label(&[]), "");
    }
}
