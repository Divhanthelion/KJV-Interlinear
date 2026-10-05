//! All app data in one value, built from the source files at compile time and
//! embedded in the app, so no platform has to parse ~110 MB of text at startup.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::models::{Bible, ExtendedBible, Testament};
use crate::original_languages::load_extended_bible;
use crate::parsing::CANONICAL_BOOKS;
use crate::red_letter::RedLetterIndex;

/// Bump when the bundle layout changes.
pub const BUNDLE_VERSION: u32 = 2;

/// The 1769 KJV: verses, chapters, and Psalm titles.
pub const KJV_VERSES: usize = 31_102;
pub const KJV_CHAPTERS: usize = 1_189;
pub const KJV_PSALM_TITLES: usize = 116;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataBundle {
    pub version: u32,
    pub bible: Bible,
    pub extended: ExtendedBible,
    pub red_letter: RedLetterIndex,
}

impl DataBundle {
    /// Build from the repository's source data: `old_testament/`, `new_testament/`, `data/`.
    pub fn from_sources(root: &Path) -> Result<Self, String> {
        let bible = Bible::from_directories(&root.join("old_testament"), &root.join("new_testament"))
            .map_err(|e| format!("KJV text: {}", e))?;
        let extended = load_extended_bible(&root.join("data"))?;
        let red_letter = RedLetterIndex::load(&root.join("data/words_of_jesus.json"))?;
        let bundle = Self { version: BUNDLE_VERSION, bible, extended, red_letter };
        bundle.validate()?;
        Ok(bundle)
    }

    /// Check the whole bundle is the complete KJV with everything attached to it,
    /// so a broken source file fails the app's build instead of shipping. Lists
    /// every problem found.
    pub fn validate(&self) -> Result<(), String> {
        let mut problems = Vec::new();
        let books = &self.bible.books;
        let names: Vec<&str> = books.iter().map(|b| b.name.as_str()).collect();
        if names != CANONICAL_BOOKS {
            problems.push(format!("books are not the 66 in canonical order: {:?}", names));
        }

        let (mut verses, mut chapters, mut titles) = (0, 0, 0);
        for (i, book) in books.iter().enumerate() {
            let testament = if i < 39 { Testament::Old } else { Testament::New };
            if book.testament != testament {
                problems.push(format!("{} is in the wrong testament", book.name));
            }
            for (c, chapter) in book.chapters.iter().enumerate() {
                chapters += 1;
                let at = format!("{} {}", book.name, chapter.number);
                if chapter.number as usize != c + 1 {
                    problems.push(format!("{}: chapter {} is numbered {}", book.name, c + 1, chapter.number));
                }
                if chapter.verses.is_empty() {
                    problems.push(format!("{} has no verses", at));
                }
                if let Some(title) = &chapter.superscription {
                    titles += 1;
                    if book.name != "Psalms" {
                        problems.push(format!("{} has a title; only Psalms do", at));
                    }
                    if title.verse_number != 0 {
                        problems.push(format!("{}: title is numbered {}", at, title.verse_number));
                    }
                }
                for (v, verse) in chapter.verses.iter().enumerate() {
                    verses += 1;
                    if verse.verse_number as usize != v + 1 {
                        problems.push(format!("{}: verse {} is numbered {}", at, v + 1, verse.verse_number));
                    }
                    if verse.book != book.name || verse.chapter != chapter.number {
                        problems.push(format!(
                            "{}:{} is filed as {} {}",
                            at, verse.verse_number, verse.book, verse.chapter
                        ));
                    }
                }
                for verse in chapter.superscription.iter().chain(&chapter.verses) {
                    let words = self.extended.get_interlinear(&book.name, chapter.number, verse.verse_number);
                    if words.is_none_or(|iv| iv.original_words.is_empty()) {
                        problems.push(format!("{}:{} has no Hebrew or Greek words", at, verse.verse_number));
                    }
                }
            }
        }
        let totals = (verses, chapters, titles);
        if totals != (KJV_VERSES, KJV_CHAPTERS, KJV_PSALM_TITLES) {
            problems.push(format!(
                "{} verses, {} chapters, and {} Psalm titles; the KJV has {}, {}, and {}",
                verses, chapters, titles, KJV_VERSES, KJV_CHAPTERS, KJV_PSALM_TITLES
            ));
        }
        problems.extend(self.red_letter.unresolved(&self.bible));

        if problems.is_empty() {
            return Ok(());
        }
        let shown = problems.iter().take(50).map(|p| format!("  {}", p)).collect::<Vec<_>>().join("\n");
        Err(format!("the data bundle is not the complete KJV ({} problems):\n{}", problems.len(), shown))
    }

    /// Compact binary form (bincode).
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        bincode::serialize(self).map_err(|e| format!("serialize bundle: {}", e))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let bundle: Self = bincode::deserialize(bytes).map_err(|e| format!("read bundle: {}", e))?;
        if bundle.version != BUNDLE_VERSION {
            return Err(format!("bundle version {} does not match {}", bundle.version, BUNDLE_VERSION));
        }
        let mut bundle = bundle;
        bundle.extended.rebuild_strongs_index();
        Ok(bundle)
    }
}
