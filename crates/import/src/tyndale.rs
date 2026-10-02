//! The Tyndale Open Study Notes (Tyndale House Publishers, CC BY-SA 4.0), from
//! `tyndale_open-studynotes.zip`: five XML files of `<item>`s, each with its passage in
//! `<refs>` (the NLT's numbering) and its text in `<body>`. Two commentaries come from it:
//!
//! * part `notes`: each book's introduction summary and introduction
//!   (`BookIntroSummaries.xml`, `BookIntros.xml`) as its introduction ("0:0"), and the
//!   study notes (`StudyNotes.xml`) on their passages;
//! * part `articles`: the profiles of people and the theme articles (`Profiles.xml`,
//!   `ThemeNotes.xml`) on their passages; one whose passage is a whole book is that
//!   book's introduction.
//!
//! Notes are in the order of their passages, and in the source's order where they start
//! together (a section's overview, then its first verse). Every body is converted with
//! [`markup::convert`] ([`Dialect::Tyndale`]) and proven to have the source's text
//! ([`markup::prove`]). An item's `<title>` repeats what its place or its body says (the
//! book's name; a profile's title, which opens its body) and is counted, not kept. The
//! NLT's 3 John 1:15 and Revelation 12:18 are the KJV's 1:14 and 13:1 (counted as
//! `refs_renumbered`, with the links in the notes).
//!
//! Links to other items (`?item=Blessing_ThemeNote_Filament`, `?item=Gen_BookIntro_Filament`)
//! open the passage that item is about; a book's introduction, and an article on a whole
//! book, are about its first chapter (where the app shows a book's introduction).

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;

use kjv_library::books;
use kjv_sword::kjv;

use crate::commentaries::{Built, Entry, Note, Report, SourceInfo, built};
use crate::markup::{self, Dialect, Options};
use crate::{cache, sources};

/// The folder the zip keeps its files in.
const FOLDER: &str = "Tyndale Open Study Notes";

/// Every file, in the order the notes come from them.
const FILES: &[&str] = &["BookIntroSummaries.xml", "BookIntros.xml", "StudyNotes.xml", "Profiles.xml", "ThemeNotes.xml"];

struct Item<'a> {
    name: &'a str,
    typename: &'a str,
    title: Option<&'a str>,
    refs: &'a str,
    body: &'a str,
}

fn attribute<'a>(attrs: &'a str, name: &str) -> Option<&'a str> {
    let start = attrs.find(&format!("{name}=\""))? + name.len() + 2;
    let len = attrs[start..].find('"')?;
    Some(&attrs[start..start + len])
}

/// The release and `<item>`s of one file, checked to hold nothing else.
fn items<'a>(file: &str, xml: &'a str) -> Result<(&'a str, Vec<Item<'a>>), String> {
    let bad = |what: &str, at: &str| format!("{file}: {what} at {:?}", at.chars().take(60).collect::<String>());
    let rest = xml.strip_prefix("<items release=\"").ok_or_else(|| bad("expected <items release=", xml))?;
    let (release, mut rest) = rest.split_once("\">").ok_or_else(|| bad("bad <items>", rest))?;
    let mut out = Vec::new();
    loop {
        rest = rest.trim_start();
        if let Some(after) = rest.strip_prefix("</items>") {
            if !after.trim().is_empty() {
                return Err(bad("text after </items>", after));
            }
            break;
        }
        let r = rest.strip_prefix("<item ").ok_or_else(|| bad("expected <item>", rest))?;
        let (attrs, r) = r.split_once('>').ok_or_else(|| bad("bad <item>", r))?;
        let (inner, r) = r.split_once("</item>").ok_or_else(|| bad("an <item> never closed", r))?;
        let name = attribute(attrs, "name").ok_or_else(|| bad("an <item> without a name", attrs))?;
        let typename = attribute(attrs, "typename").ok_or_else(|| bad("an <item> without a typename", attrs))?;
        if attribute(attrs, "product") != Some("TyndaleOpenStudyNotes") {
            return Err(bad("an <item> of another product", attrs));
        }
        let mut inner = inner.trim();
        let mut title = None;
        if let Some(t) = inner.strip_prefix("<title>") {
            let (t, after) = t.split_once("</title>").ok_or_else(|| bad("a <title> never closed", t))?;
            title = Some(t);
            inner = after.trim_start();
        }
        let r2 = inner.strip_prefix("<refs>").ok_or_else(|| bad("expected <refs>", inner))?;
        let (refs, after) = r2.split_once("</refs>").ok_or_else(|| bad("a <refs> never closed", r2))?;
        let body = after
            .trim()
            .strip_prefix("<body>")
            .and_then(|b| b.strip_suffix("</body>"))
            .ok_or_else(|| bad("expected <body>…</body> and nothing else", after))?;
        out.push(Item { name, typename, title, refs, body });
        rest = r;
    }
    Ok((release, out))
}

/// Whether a passage is the whole of its book in the KJV.
fn whole_book(r: &kjv_library::reference::Range) -> bool {
    let Some(book) = kjv::book(r.book) else { return false };
    let chapters = book.verses.len() as u32;
    r.start == (1, 1) && r.end == (chapters, u32::from(book.verses[chapters as usize - 1]))
}

/// The keys links name an item by: `Blessing_ThemeNote`; a study note by its name
/// (`IISam.4.4_StudyNote`) or its passage (`2Sam.4.4_StudyNote`); a book's introduction
/// (named `GenIntro`, `IICorIntro`) as `Gen_BookIntro`, `GenIntro_BookIntro`, or by its
/// passage's book, `2Cor_BookIntro`; and anything with a title by it,
/// `title:The Messianic Banquet_ThemeNote`.
fn link_keys(item: &Item) -> Vec<String> {
    let mut keys = match item.typename {
        "BookIntro" => vec![
            format!("{}_BookIntro", item.name.strip_suffix("Intro").unwrap_or(item.name)),
            format!("{}_BookIntro", item.name),
            format!("{}_BookIntro", item.refs.split('.').next().unwrap_or("")),
        ],
        "StudyNote" => vec![format!("{}_StudyNote", item.name), format!("{}_StudyNote", item.refs)],
        other => vec![format!("{}_{}", item.name, other)],
    };
    if let Some(title) = item.title {
        keys.push(format!("title:{title}_{}", item.typename));
    }
    keys.sort_unstable();
    keys.dedup();
    keys
}

pub fn convert(entry: &Entry, pinned: &[sources::Source]) -> Result<Built, String> {
    let source = pinned.iter().find(|s| s.path == entry.source).ok_or_else(|| format!("{} is not pinned", entry.source))?;
    let file = File::open(cache().join(&entry.source)).map_err(|e| format!("{}: {}", entry.source, e))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("{}: {}", entry.source, e))?;
    let names: &[&str] = match entry.part.as_deref() {
        Some("notes") => &FILES[..3],
        Some("articles") => &FILES[3..],
        other => return Err(format!("{}: unknown part {:?} (notes or articles)", entry.id, other)),
    };
    let mut xml: Vec<String> = Vec::new();
    for name in FILES {
        let mut text = String::new();
        zip.by_name(&format!("{FOLDER}/{name}"))
            .map_err(|e| format!("{}: {name}: {e}", entry.source))?
            .read_to_string(&mut text)
            .map_err(|e| format!("{}: {name}: {e}", entry.source))?;
        xml.push(text);
    }
    let mut files: Vec<(&str, &str, Vec<Item>)> = Vec::new();
    for (name, text) in FILES.iter().zip(&xml) {
        let (release, list) = items(name, text)?;
        files.push((name, release, list));
    }
    let release = files[0].1;
    if let Some((name, other, _)) = files.iter().find(|(_, r, _)| *r != release) {
        return Err(format!("{name} is release {other}, the other files {release}"));
    }

    // Where every item is, for links to it
    let mut report = Report::default();
    let mut links: BTreeMap<String, String> = BTreeMap::new();
    let mut ignore = 0;
    for (_, _, list) in &files {
        for item in list {
            let ranges = markup::tyndale_ranges(item.refs, &mut ignore).ok_or_else(|| format!("{} {}: its <refs> can't be read", entry.id, item.name))?;
            let to = match ranges.as_slice() {
                [r] if whole_book(r) => format!("{}.1", r.book),
                _ => ranges.iter().map(|r| r.osis()).collect::<Vec<_>>().join(" "),
            };
            for key in link_keys(item) {
                // (two study notes on one passage, or two books' introductions titled
                // alike: the first)
                if links.contains_key(&key) && item.typename != "StudyNote" && !key.starts_with("title:") {
                    return Err(format!("{} {key}: two items named so", entry.id));
                }
                links.entry(key).or_insert_with(|| to.clone());
            }
        }
    }

    // (the file's place in `FILES`, the note)
    let mut notes: Vec<(usize, Note)> = Vec::new();
    for (k, (name, _, list)) in files.iter().enumerate().filter(|(_, (name, ..))| names.contains(name)) {
        for item in list {
            let ctx = |m: &str| format!("{} {name} {}: {m}", entry.id, item.name);
            report.entries += 1;
            report.source_bytes += item.body.len();
            let ranges = markup::tyndale_ranges(item.refs, &mut report.stats.renumbered).ok_or_else(|| ctx("its <refs> can't be read"))?;
            let [range] = ranges.as_slice() else { return Err(ctx("its <refs> span books")) };
            if !markup::possible(&ranges) {
                return Err(ctx("its <refs> name verses the KJV hasn't"));
            }
            let place = match (item.typename, whole_book(range)) {
                ("BookIntro" | "BookIntroSummary", true) | ("Profile" | "ThemeNote", true) => ((0, 0), (0, 0)),
                ("BookIntro" | "BookIntroSummary", false) => return Err(ctx("a book introduction not on the whole book")),
                ("StudyNote" | "Profile" | "ThemeNote", _) => (range.start, range.end),
                (other, _) => return Err(ctx(&format!("unknown typename {other:?}"))),
            };
            if item.title.is_some() {
                *report.stats.ignored.entry("item <title> (the book's name, or the title its body opens with)".into()).or_default() += 1;
            }
            let options = Options { dialect: Dialect::Tyndale, jud_is_judges: false, context: Some((range.book, range.start.0)) };
            let (body, stats) = markup::convert_linked(item.body, options, &links).map_err(|m| ctx(&m))?;
            report.stats.add(stats);
            let proof = markup::prove(item.body, &body, Dialect::Tyndale).map_err(|m| ctx(&format!("the converted note's text differs from the source's: {m}")))?;
            if body.is_empty() {
                report.empty.push((range.book, place.0, place.1));
                continue;
            }
            report.chars += proof.chars;
            report.stats.structural_gaps += proof.structural_gaps;
            report.body_bytes += body.len() as u64;
            let slot = report.per_book.entry(range.book).or_default();
            slot.0 += 1;
            slot.1 += proof.chars;
            notes.push((k, Note { book: range.book, from: place.0, to: place.1, body }));
        }
    }
    // In the order of their passages; where they start together, files in order, then the
    // source's order (stable)
    notes.sort_by_key(|(k, n)| (books::order(n.book), n.from, *k));
    let notes: Vec<Note> = notes.into_iter().map(|(_, n)| n).collect();
    report.notes = notes.len();
    let info = SourceInfo {
        module: format!("{FOLDER}: {}", names.join(", ")),
        version: Some(release.to_string()),
        text_source: None,
        markup: "Tyndale XML".into(),
        encoding: "UTF-8".into(),
    };
    built(entry, source, info, &notes, report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_and_nothing_else() {
        let xml = "<items release=\"1.25\">\n<item name=\"Gen.1.1\" typename=\"StudyNote\" product=\"TyndaleOpenStudyNotes\">\n<refs>Gen.1.1</refs>\n<body>\n<p class=\"sn-text\">x</p>\n</body>\n</item>\n</items>\n";
        let (release, list) = items("t", xml).unwrap();
        assert_eq!(release, "1.25");
        assert_eq!((list[0].name, list[0].typename, list[0].refs, list[0].title), ("Gen.1.1", "StudyNote", "Gen.1.1", None));
        assert_eq!(list[0].body.trim(), "<p class=\"sn-text\">x</p>");
        assert!(items("t", &xml.replace("<refs>", "<extra/><refs>")).is_err());
        assert!(items("t", &xml.replace("TyndaleOpenStudyNotes", "Other")).is_err());
    }
}
