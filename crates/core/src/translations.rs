//! The library's translations for the app's interface: the catalogue, and any
//! chapter of any translation with its heading and neighbours.

use kjv_library::books::{self, Section};
use kjv_library::view::ChapterView;
use kjv_library::{BibleInfo, Library};
use serde::Serialize;

use crate::api::ChapterRef;

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
