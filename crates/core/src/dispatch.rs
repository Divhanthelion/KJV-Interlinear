//! One JSON entry point for every data command, shared by the app and the
//! browser preview server so both expose exactly the same interface.

use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{self, ChapterOptions, Scope};
use crate::bundle::DataBundle;
use crate::context;

/// Most search results sent at once; the total count is always exact.
pub const SEARCH_LIMIT: usize = 500;

#[derive(Deserialize)]
struct ChapterArgs {
    book: String,
    chapter: u32,
    #[serde(default)]
    options: ChapterOptions,
}

#[derive(Deserialize)]
struct SearchArgs {
    query: String,
    scope: Scope,
    #[serde(default)]
    book: Option<String>,
}

#[derive(Deserialize)]
struct StrongsArgs {
    query: String,
}

#[derive(Deserialize)]
struct ContextArgs {
    scope: context::Scope,
    #[serde(default)]
    options: context::ContextOptions,
}

#[derive(Deserialize)]
struct VerseArgs {
    book: String,
    chapter: u32,
    #[serde(default)]
    verse: Option<u32>,
}

fn parse<T: for<'de> Deserialize<'de>>(name: &str, args: Value) -> Result<T, String> {
    serde_json::from_value(args).map_err(|e| format!("{}: bad arguments: {}", name, e))
}

fn to_json<T: serde::Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

/// Run command `name` with JSON `args`.
pub fn dispatch(data: &DataBundle, name: &str, args: Value) -> Result<Value, String> {
    match name {
        "books" => to_json(api::books(data)),
        "chapter" => {
            let a: ChapterArgs = parse(name, args)?;
            to_json(api::chapter(data, &a.book, a.chapter, &a.options)?)
        }
        "search" => {
            let a: SearchArgs = parse(name, args)?;
            to_json(api::search(data, &a.query, a.scope, a.book.as_deref(), SEARCH_LIMIT))
        }
        "strongs" => {
            let a: StrongsArgs = parse(name, args)?;
            to_json(api::strongs_search(data, &a.query, SEARCH_LIMIT))
        }
        "lexicon" => {
            let a: StrongsArgs = parse(name, args)?;
            to_json(api::lexicon(data, &a.query))
        }
        "copy_text" => {
            let a: VerseArgs = parse(name, args)?;
            let text = match a.verse {
                Some(v) => api::copy_verse(data, &a.book, a.chapter, v),
                None => api::copy_chapter(data, &a.book, a.chapter),
            };
            text.map(|t| json!(t))
                .ok_or_else(|| format!("no text for {} {}", a.book, a.chapter))
        }
        "context_size" => {
            let a: ContextArgs = parse(name, args)?;
            to_json(context::size(data, &a.scope, &a.options)?)
        }
        _ => Err(format!("unknown command {:?}", name)),
    }
}

#[derive(Deserialize)]
struct BibleChapterArgs {
    bible: String,
    book: String,
    chapter: u32,
}

/// Every command: the library's translations, then everything `dispatch` answers.
pub fn dispatch_all(
    data: &DataBundle,
    library: &kjv_library::Library,
    name: &str,
    args: Value,
) -> Result<Value, String> {
    match name {
        "bibles" => to_json(crate::translations::bibles(library)),
        "bible_chapter" => {
            let a: BibleChapterArgs = parse(name, args)?;
            to_json(crate::translations::chapter(library, &a.bible, &a.book, a.chapter)?)
        }
        "commentaries" => to_json(library.commentaries()),
        "notes" => {
            let a: NotesArgs = parse(name, args)?;
            to_json(crate::translations::notes(library, &a.commentaries, &a.bible, &a.book, a.chapter, a.verse)?)
        }
        "bible_map" => {
            let a: BibleMapArgs = parse(name, args)?;
            to_json(crate::translations::map(library, &a.from, &a.to, &a.book, a.chapter, a.verse)?)
        }
        _ => dispatch(data, name, args),
    }
}

#[derive(Deserialize)]
struct BibleMapArgs {
    from: String,
    to: String,
    book: String,
    chapter: u32,
    #[serde(default)]
    verse: u32,
}

#[derive(Deserialize)]
struct NotesArgs {
    commentaries: Vec<String>,
    bible: String,
    book: String,
    chapter: u32,
    #[serde(default)]
    verse: u32,
}
