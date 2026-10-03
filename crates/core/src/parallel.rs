//! Several translations side by side: the leading translation's chapter verse by
//! verse, each row with every other column's corresponding verses (found through the
//! verse alignment, so the Douay-Rheims' Psalm 22 sits beside the KJV's 23), and, if
//! asked, the Hebrew or Greek the KJV's verses are translated from.

use std::collections::{HashMap, HashSet};

use kjv_library::Library;
use kjv_library::alignment::Ref;
use kjv_library::books;
use kjv_library::view::{ChapterView as LibraryChapter, Heading, Part, VerseView as LibraryVerse};
use serde::{Deserialize, Serialize};

use crate::api::{self, ChapterOptions, ChapterRef, OriginalView};
use crate::bundle::DataBundle;
use crate::translations;

/// The column of Hebrew, Aramaic, and Greek
pub const ORIGINAL: &str = "original";

#[derive(Debug, Deserialize)]
pub struct ParallelArgs {
    /// Translation ids, the leading one first; "original" for the Hebrew and Greek
    pub columns: Vec<String>,
    /// The app's key for the book ("First Samuel"), and the chapter, in the leading
    /// translation's numbering
    pub book: String,
    pub chapter: u32,
}

#[derive(Debug, Serialize)]
pub struct Column {
    pub id: String,
    /// "KJV", "DRA", "Hebrew", "Greek", "Hebrew & Aramaic"
    pub abbr: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct ParallelChapter {
    /// The app's book key and chapter, as the leading translation has them
    pub book: String,
    pub chapter: u32,
    /// "Psalm 23"
    pub heading: String,
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
    /// The leading translation's headings after its last verse
    pub after: Vec<Heading>,
    pub prev: Option<ChapterRef>,
    pub next: Option<ChapterRef>,
}

/// One verse of the leading translation and what the other columns have for it.
#[derive(Debug, Serialize)]
pub struct Row {
    /// The leading verse's number: "16", "1-2"; "0" for a Psalm title
    pub number: String,
    /// The leading translation's headings before it
    pub before: Vec<Heading>,
    /// One per column, the leading translation's first
    pub cells: Vec<Cell>,
}

#[derive(Debug, Default, Serialize)]
pub struct Cell {
    /// Usually one verse; more where the column divides it differently; none where
    /// the column has nothing for it
    pub verses: Vec<CellVerse>,
    /// What the column has for this verse was given in an earlier row (one verse of
    /// the column holds this one and the one before)
    pub above: bool,
}

#[derive(Debug, Serialize)]
pub struct CellVerse {
    /// Its number, with the chapter, or book and chapter, where those differ from
    /// the row's: "16", "22:1", "Ezra 11:1"; "title" for a Psalm title
    pub label: String,
    /// The text, as the reader draws it
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<Part>,
    /// For the original-language column
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original: Option<OriginalView>,
}

/// A chapter of one column, ready to pick verses from.
enum Chapter {
    /// The app's KJV
    Core(api::ChapterView),
    Library(LibraryChapter),
}

impl Chapter {
    fn verse(&self, number: &str) -> Option<Vec<Part>> {
        match self {
            Chapter::Core(c) => {
                let n: u32 = number.parse().ok()?;
                let v = if n == 0 { c.title.as_ref()? } else { c.verses.iter().find(|v| v.number == n)? };
                Some(
                    v.segments
                        .iter()
                        .map(|s| Part::Text { text: s.text.clone(), styles: if s.red { vec!["wj".into()] } else { Vec::new() } })
                        .collect(),
                )
            }
            Chapter::Library(c) => {
                let v = if number == "0" { c.title.as_ref() } else { None }.or_else(|| c.verses.iter().find(|v| v.number == number))?;
                Some(v.parts.clone())
            }
        }
    }
}

fn in_core(data: &DataBundle, code: &str) -> Option<String> {
    let name = books::by_code(code)?.name;
    data.bible.books.iter().any(|b| b.name == name).then(|| name.to_string())
}

struct Chapters<'a> {
    data: &'a DataBundle,
    lib: &'a Library,
    kept: HashMap<(String, String, u32), Option<Chapter>>,
}

impl Chapters<'_> {
    /// Chapter `chapter` of `code` in translation `id` (the KJV's 66 books from the app's KJV).
    fn get(&mut self, id: &str, code: &str, chapter: u32) -> &Option<Chapter> {
        let key = (id.to_string(), code.to_string(), chapter);
        if !self.kept.contains_key(&key) {
            let c = match (id, in_core(self.data, code)) {
                ("kjv", Some(name)) => {
                    let options = ChapterOptions { red_letter: true, query: None, original: false };
                    api::chapter(self.data, &name, chapter, &options).ok().map(Chapter::Core)
                }
                _ => self.lib.chapter(id, code, chapter).ok().map(Chapter::Library),
            };
            self.kept.insert(key.clone(), c);
        }
        &self.kept[&key]
    }
}

/// "16"; "22:1" in another chapter; "Ezra 11:1" in another book; "title" for a Psalm title
fn label(row: (&str, u32), at: &Ref) -> String {
    let number = if at.2 == "0" { "title".to_string() } else { at.2.clone() };
    if (at.0.as_str(), at.1) == row {
        return number;
    }
    if at.0 == row.0 {
        return if at.2 == "0" { format!("{} (title)", at.1) } else { format!("{}:{}", at.1, number) };
    }
    let head = if at.0 == "PSA" { format!("Psalm {}", at.1) } else { format!("{} {}", books::by_code(&at.0).map_or(at.0.as_str(), |b| b.display), at.1) };
    if at.2 == "0" { format!("{} (title)", head) } else { format!("{}:{}", head, number) }
}

/// The Hebrew, Aramaic, or Greek of KJV verse `at`.
fn original(data: &DataBundle, at: &Ref) -> Option<OriginalView> {
    let name = in_core(data, &at.0)?;
    api::original_view(data, &name, at.1, at.2.parse().ok()?)
}

/// The leading translation's chapter: each verse's number and the headings before it.
struct Leading {
    heading: String,
    prev: Option<ChapterRef>,
    next: Option<ChapterRef>,
    verses: Vec<(String, Vec<Heading>)>,
    after: Vec<Heading>,
}

pub fn chapter(data: &DataBundle, lib: &Library, args: &ParallelArgs) -> Result<ParallelChapter, String> {
    let lead = args.columns.first().ok_or("no columns")?.clone();
    if lead == ORIGINAL {
        return Err("the first column must be a translation".into());
    }
    let k = books::by_name(&args.book).ok_or_else(|| format!("no book named {:?}", args.book))?;
    let code = k.code;
    let mut columns = Vec::new();
    for id in &args.columns {
        if id == ORIGINAL {
            columns.push(Column { id: id.clone(), abbr: String::new(), name: String::new() });
        } else {
            let info = lib.bible(id).ok_or_else(|| format!("no translation {:?}", id))?;
            columns.push(Column { id: id.clone(), abbr: info.abbr.clone(), name: info.name.clone() });
        }
    }

    // The leading chapter: its verses, headings, and neighbours
    let mut chapters = Chapters { data, lib, kept: HashMap::new() };
    let Leading { heading, prev, next, verses: leading, after } = if lead == "kjv" && in_core(data, code).is_some() {
        let c = api::chapter(data, &args.book, args.chapter, &ChapterOptions::default())?;
        let verses = c.title.iter().chain(c.verses.iter()).map(|v| (v.number.to_string(), Vec::new())).collect();
        Leading { heading: c.heading, prev: c.prev, next: c.next, verses, after: Vec::new() }
    } else {
        let c = translations::chapter(lib, &lead, &args.book, args.chapter)?;
        let verses = c.view.title.iter().chain(c.view.verses.iter()).map(|v: &LibraryVerse| (v.number.clone(), v.before.clone())).collect();
        Leading { heading: c.heading, prev: c.prev, next: c.next, verses, after: c.view.after }
    };

    let mut shown: Vec<HashSet<Ref>> = vec![HashSet::new(); args.columns.len()];
    let mut languages: Vec<&'static str> = Vec::new();
    let mut rows = Vec::with_capacity(leading.len());
    for (number, before) in leading {
        let at: Ref = (code.to_string(), args.chapter, number.clone());
        let mut cells = Vec::with_capacity(args.columns.len());
        for (i, id) in args.columns.iter().enumerate() {
            let (target, refs) = if id == ORIGINAL {
                ("kjv", if lead == "kjv" { vec![at.clone()] } else { lib.map(&lead, "kjv", &at)? })
            } else if *id == lead {
                (id.as_str(), vec![at.clone()])
            } else {
                (id.as_str(), lib.map(&lead, id, &at)?)
            };
            let mut cell = Cell::default();
            let fresh: Vec<&Ref> = refs.iter().filter(|r| !shown[i].contains(*r)).collect();
            if fresh.is_empty() && !refs.is_empty() {
                cell.above = true;
            }
            for r in fresh {
                shown[i].insert(r.clone());
                let l = label((code, args.chapter), r);
                if id == ORIGINAL {
                    if let Some(o) = original(data, r) {
                        if !languages.contains(&o.lang) {
                            languages.push(o.lang);
                        }
                        cell.verses.push(CellVerse { label: l, parts: Vec::new(), original: Some(o) });
                    }
                } else if let Some(Some(parts)) = chapters.get(target, &r.0, r.1).as_ref().map(|c| c.verse(&r.2))
                    && !parts.is_empty()
                {
                    // (A verse with no text of its own, such as a Psalm title printed as a
                    // heading, isn't given)
                    cell.verses.push(CellVerse { label: l, parts, original: None });
                }
            }
            cells.push(cell);
        }
        rows.push(Row { number, before, cells });
    }

    // The original-language column is named for what it holds here
    let names: Vec<&str> = languages
        .iter()
        .map(|l| match *l {
            "he" => "Hebrew",
            "arc" => "Aramaic",
            _ => "Greek",
        })
        .collect();
    for c in &mut columns {
        if c.id == ORIGINAL {
            c.abbr = match names.as_slice() {
                [] => "Hebrew/Greek".to_string(),
                [one] => one.to_string(),
                [a, b] => format!("{} & {}", a, b),
                more => more.join(", "),
            };
            c.name = format!("{}, as the KJV translates it", c.abbr);
        }
    }
    Ok(ParallelChapter { book: args.book.clone(), chapter: args.chapter, heading, columns, rows, after, prev, next })
}
