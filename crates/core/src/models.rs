use serde::{Deserialize, Serialize};

/// Represents a single verse in the Bible
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verse {
    pub book: String,
    pub chapter: u32,
    pub verse_number: u32,
    pub text: String,
}

/// Represents a chapter containing multiple verses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub number: u32,
    /// Psalm superscription (title printed above verse 1), stored as verse 0
    #[serde(default)]
    pub superscription: Option<Verse>,
    pub verses: Vec<Verse>,
}

/// Represents which testament a book belongs to
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Testament {
    Old,
    New,
}

/// Represents a book of the Bible
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Book {
    pub name: String,
    pub testament: Testament,
    pub chapters: Vec<Chapter>,
}

/// Represents the entire Bible
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bible {
    pub books: Vec<Book>,
}

#[allow(dead_code)]
impl Bible {
    /// Get a specific verse by book name, chapter, and verse number
    pub fn get_verse(&self, book_name: &str, chapter: u32, verse: u32) -> Option<&Verse> {
        self.books
            .iter()
            .find(|b| b.name == book_name)
            .and_then(|book| book.chapters.iter().find(|c| c.number == chapter))
            .and_then(|chapter| chapter.verses.iter().find(|v| v.verse_number == verse))
    }

    /// Get an entire chapter by book name and chapter number
    pub fn get_chapter(&self, book_name: &str, chapter: u32) -> Option<&Chapter> {
        self.books
            .iter()
            .find(|b| b.name == book_name)
            .and_then(|book| book.chapters.iter().find(|c| c.number == chapter))
    }

    /// Search verses in the given books (case-, apostrophe- and æ-insensitive)
    fn search_books<'a>(
        &'a self,
        query: &str,
        include: impl Fn(&Book) -> bool,
    ) -> Vec<&'a Verse> {
        let query = fold_for_search(query);
        if query.is_empty() {
            return Vec::new();
        }
        self.books
            .iter()
            .filter(|b| include(b))
            .flat_map(|b| b.chapters.iter())
            .flat_map(|c| c.superscription.iter().chain(c.verses.iter()))
            .filter(|v| fold_for_search(&v.text).contains(&query))
            .collect()
    }

    /// Search for text across all verses
    pub fn search(&self, query: &str) -> Vec<&Verse> {
        self.search_books(query, |_| true)
    }

    /// Search within a specific book only
    pub fn search_in_book(&self, query: &str, book_name: &str) -> Vec<&Verse> {
        self.search_books(query, |b| b.name == book_name)
    }

    /// Search within a specific testament only
    pub fn search_in_testament(&self, query: &str, testament: &Testament) -> Vec<&Verse> {
        self.search_books(query, |b| &b.testament == testament)
    }

    /// Get list of book names
    pub fn book_names(&self) -> Vec<&str> {
        self.books.iter().map(|b| b.name.as_str()).collect()
    }

    /// Get chapter count for a book
    pub fn chapter_count(&self, book_name: &str) -> Option<usize> {
        self.books
            .iter()
            .find(|b| b.name == book_name)
            .map(|b| b.chapters.len())
    }

    /// Get verse count for a chapter
    pub fn verse_count(&self, book_name: &str, chapter: u32) -> Option<usize> {
        self.get_chapter(book_name, chapter).map(|c| c.verses.len())
    }
}

/// A bookmark to a specific verse
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bookmark {
    pub book: String,
    pub chapter: u32,
    pub verse: u32,
    pub note: Option<String>,
    pub created_at: u64,
}

/// A history entry for reading tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub book: String,
    pub chapter: u32,
    pub timestamp: u64,
}

/// Search scope options
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum SearchScope {
    #[default]
    All,
    CurrentBook,
    OldTestament,
    NewTestament,
}

impl SearchScope {
    pub fn label(&self) -> &'static str {
        match self {
            SearchScope::All => "All",
            SearchScope::CurrentBook => "Current Book",
            SearchScope::OldTestament => "Old Testament",
            SearchScope::NewTestament => "New Testament",
        }
    }
}

// ============================================================================
// Original Language Support - Hebrew/Greek with Strong's Numbers
// ============================================================================

use std::collections::HashMap;

use crate::text::fold_for_search;

/// Canonical Strong's key as used in the STEP data: letter + 4 digits ("h430" -> "H0430").
/// A bare number is treated as Hebrew.
pub fn normalize_strongs(input: &str) -> Option<String> {
    let s = input.trim().to_uppercase();
    let (letter, rest) = match s.chars().next()? {
        c @ ('H' | 'G') => (c, &s[1..]),
        c if c.is_ascii_digit() => ('H', s.as_str()),
        _ => return None,
    };
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    let n: u32 = digits.parse().ok()?;
    Some(format!("{}{:04}", letter, n))
}

/// Generate common Strong's key spellings (padded / unpadded).
fn strongs_key_variants(strongs: &str) -> Vec<String> {
    let mut out = Vec::new();
    let Some(letter) = strongs.chars().next() else {
        return out;
    };
    if letter != 'H' && letter != 'G' {
        return out;
    }
    let digits: String = strongs.chars().skip(1).filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return out;
    }
    let trimmed = digits.trim_start_matches('0');
    let trimmed = if trimmed.is_empty() { "0" } else { trimmed };
    out.push(format!("{}{}", letter, trimmed));
    out.push(format!("{}{:0>4}", letter, trimmed));
    out.push(format!("{}{:0>5}", letter, trimmed));
    out.sort();
    out.dedup();
    out.retain(|v| v != strongs);
    out
}

/// Language of the original text
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OriginalLanguage {
    Hebrew,
    Aramaic,
    Greek,
}

/// A single word from the original language text
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OriginalWord {
    /// Position in the verse (1-indexed)
    pub position: u32,
    /// Original Hebrew/Greek text
    pub original_text: String,
    /// Romanized transliteration
    pub transliteration: String,
    /// English translation/gloss
    pub english_gloss: String,
    /// Strong's number (e.g., "H430" for Hebrew, "G2316" for Greek)
    pub strongs_number: Option<String>,
    /// Morphology code (e.g., "HNcmpa" for Hebrew noun)
    pub morphology: Option<String>,
}

/// A verse with interlinear original language data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterlinearVerse {
    pub book: String,
    pub chapter: u32,
    pub verse_number: u32,
    pub language: OriginalLanguage,
    pub original_words: Vec<OriginalWord>,
}

/// A reference to a specific verse
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct VerseRef {
    pub book: String,
    pub chapter: u32,
    pub verse: u32,
}

impl VerseRef {
    pub fn new(book: &str, chapter: u32, verse: u32) -> Self {
        Self {
            book: book.to_string(),
            chapter,
            verse,
        }
    }
}

/// A lexicon/dictionary entry for a Strong's number
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LexiconEntry {
    /// Strong's number (e.g., "H0001" or "G0001")
    pub strongs_number: String,
    /// Original word in Hebrew/Greek
    pub original_word: String,
    /// Romanized transliteration
    pub transliteration: String,
    /// Morphology type (e.g., "N-M" for noun masculine)
    pub morph: String,
    /// Brief definition/gloss
    pub gloss: String,
    /// Full definition with details
    pub definition: String,
}

/// Pre-computed concordance index for fast Strong's lookups
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StrongsIndex {
    /// Hebrew Strong's numbers to verse references
    pub hebrew: HashMap<String, Vec<VerseRef>>,
    /// Greek Strong's numbers to verse references
    pub greek: HashMap<String, Vec<VerseRef>>,
}

impl StrongsIndex {
    pub fn new() -> Self {
        Self {
            hebrew: HashMap::new(),
            greek: HashMap::new(),
        }
    }

    /// Record that a Strong's number occurs in a verse (each verse listed once)
    pub fn add_occurrence(&mut self, strongs: &str, verse_ref: VerseRef) {
        let map = if strongs.starts_with('H') {
            &mut self.hebrew
        } else {
            &mut self.greek
        };
        let refs = map.entry(strongs.to_string()).or_default();
        // Words arrive in text order, so a repeat within a verse is always the last entry
        if refs.last() != Some(&verse_ref) {
            refs.push(verse_ref);
        }
    }

    /// Get all verses containing a Strong's number ("H430", "h0430" and "H0430" are equivalent)
    pub fn get_occurrences(&self, strongs: &str) -> Option<&Vec<VerseRef>> {
        let key = normalize_strongs(strongs)?;
        if key.starts_with('H') {
            self.hebrew.get(&key)
        } else {
            self.greek.get(&key)
        }
    }

    /// Number of verses containing a Strong's number
    pub fn occurrence_count(&self, strongs: &str) -> usize {
        self.get_occurrences(strongs).map(|v| v.len()).unwrap_or(0)
    }
}

/// Extended Bible with original language support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedBible {
    /// Hebrew OT interlinear data (verse ref -> interlinear verse)
    pub interlinear_ot: HashMap<VerseRef, InterlinearVerse>,
    /// Greek NT interlinear data (verse ref -> interlinear verse)
    pub interlinear_nt: HashMap<VerseRef, InterlinearVerse>,
    /// Hebrew lexicon (Strong's number -> entry)
    pub hebrew_lexicon: HashMap<String, LexiconEntry>,
    /// Greek lexicon (Strong's number -> entry)
    pub greek_lexicon: HashMap<String, LexiconEntry>,
    /// Concordance index; rebuilt from the verses rather than stored in the bundle
    #[serde(skip)]
    pub strongs_index: StrongsIndex,
}

impl Default for ExtendedBible {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
impl ExtendedBible {
    pub fn new() -> Self {
        Self {
            interlinear_ot: HashMap::new(),
            interlinear_nt: HashMap::new(),
            hebrew_lexicon: HashMap::new(),
            greek_lexicon: HashMap::new(),
            strongs_index: StrongsIndex::new(),
        }
    }

    /// Check if original language data is loaded
    pub fn is_loaded(&self) -> bool {
        !self.interlinear_ot.is_empty() || !self.interlinear_nt.is_empty()
    }

    /// Get interlinear data for a verse
    pub fn get_interlinear(
        &self,
        book: &str,
        chapter: u32,
        verse: u32,
    ) -> Option<&InterlinearVerse> {
        let verse_ref = VerseRef::new(book, chapter, verse);
        self.interlinear_ot
            .get(&verse_ref)
            .or_else(|| self.interlinear_nt.get(&verse_ref))
    }

    /// Rebuild the Strong's concordance: verses in canonical order, each listed once.
    pub fn rebuild_strongs_index(&mut self) {
        let mut verses: Vec<&InterlinearVerse> = self
            .interlinear_ot
            .values()
            .chain(self.interlinear_nt.values())
            .collect();
        verses.sort_by_key(|v| {
            (
                crate::parsing::canonical_book_index(&v.book).unwrap_or(usize::MAX),
                v.chapter,
                v.verse_number,
            )
        });
        let mut index = StrongsIndex::new();
        for verse in verses {
            let verse_ref = VerseRef::new(&verse.book, verse.chapter, verse.verse_number);
            for word in &verse.original_words {
                if let Some(s) = &word.strongs_number {
                    index.add_occurrence(s, verse_ref.clone());
                }
            }
        }
        self.strongs_index = index;
    }

    /// Get lexicon entry for a Strong's number
    pub fn get_lexicon_entry(&self, strongs: &str) -> Option<&LexiconEntry> {
        let map = if strongs.starts_with('H') {
            &self.hebrew_lexicon
        } else {
            &self.greek_lexicon
        };

        if let Some(entry) = map.get(strongs) {
            return Some(entry);
        }
        if let Some(entry) = normalize_strongs(strongs).and_then(|k| map.get(&k)) {
            return Some(entry);
        }

        // Try zero-padded / unpadded variants (G27 ↔ G0027)
        for variant in strongs_key_variants(strongs) {
            if let Some(entry) = map.get(&variant) {
                return Some(entry);
            }
        }
        None
    }

    /// Search by Strong's number
    pub fn search_strongs(&self, strongs: &str) -> Vec<&InterlinearVerse> {
        let verse_refs = match self.strongs_index.get_occurrences(strongs) {
            Some(refs) => refs,
            None => return Vec::new(),
        };

        verse_refs
            .iter()
            .filter_map(|vr| {
                self.interlinear_ot
                    .get(vr)
                    .or_else(|| self.interlinear_nt.get(vr))
            })
            .collect()
    }

    /// Get count of occurrences for a Strong's number
    pub fn strongs_count(&self, strongs: &str) -> usize {
        self.strongs_index.occurrence_count(strongs)
    }
}

/// Search result for Strong's concordance
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct StrongsSearchResult {
    pub verse_ref: VerseRef,
    pub strongs_number: String,
    pub matched_word_positions: Vec<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_bible() -> Bible {
        Bible {
            books: vec![
                Book {
                    name: "Genesis".to_string(),
                    testament: Testament::Old,
                    chapters: vec![Chapter {
                        number: 1,
                        superscription: None,
                        verses: vec![
                            Verse {
                                book: "Genesis".to_string(),
                                chapter: 1,
                                verse_number: 1,
                                text: "In the beginning God created the heaven and the earth."
                                    .to_string(),
                            },
                            Verse {
                                book: "Genesis".to_string(),
                                chapter: 1,
                                verse_number: 2,
                                text: "And the earth was without form, and void.".to_string(),
                            },
                        ],
                    }],
                },
                Book {
                    name: "John".to_string(),
                    testament: Testament::New,
                    chapters: vec![Chapter {
                        number: 1,
                        superscription: None,
                        verses: vec![Verse {
                            book: "John".to_string(),
                            chapter: 1,
                            verse_number: 1,
                            text: "In the beginning was the Word.".to_string(),
                        }],
                    }],
                },
            ],
        }
    }

    #[test]
    fn test_get_verse() {
        let bible = create_test_bible();
        let verse = bible.get_verse("Genesis", 1, 1);
        assert!(verse.is_some());
        assert!(verse.unwrap().text.contains("beginning"));
    }

    #[test]
    fn test_get_verse_not_found() {
        let bible = create_test_bible();
        assert!(bible.get_verse("Genesis", 1, 99).is_none());
        assert!(bible.get_verse("NotABook", 1, 1).is_none());
    }

    #[test]
    fn test_get_chapter() {
        let bible = create_test_bible();
        let chapter = bible.get_chapter("Genesis", 1);
        assert!(chapter.is_some());
        assert_eq!(chapter.unwrap().verses.len(), 2);
    }

    #[test]
    fn test_search_finds_matches() {
        let bible = create_test_bible();
        let results = bible.search("beginning");
        assert_eq!(results.len(), 2); // Genesis 1:1 and John 1:1
    }

    #[test]
    fn test_search_case_insensitive() {
        let bible = create_test_bible();
        let results = bible.search("BEGINNING");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_no_matches() {
        let bible = create_test_bible();
        let results = bible.search("xyz123");
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_in_book() {
        let bible = create_test_bible();
        let results = bible.search_in_book("beginning", "Genesis");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_search_in_testament() {
        let bible = create_test_bible();
        let old_results = bible.search_in_testament("beginning", &Testament::Old);
        let new_results = bible.search_in_testament("beginning", &Testament::New);
        assert_eq!(old_results.len(), 1);
        assert_eq!(new_results.len(), 1);
    }

    #[test]
    fn test_chapter_count() {
        let bible = create_test_bible();
        assert_eq!(bible.chapter_count("Genesis"), Some(1));
        assert_eq!(bible.chapter_count("NotABook"), None);
    }

    #[test]
    fn test_search_folds_apostrophes() {
        let mut bible = create_test_bible();
        bible.books[0].chapters[0].verses[0].text = "Moses\u{2019} seat".to_string();
        assert_eq!(bible.search("moses' seat").len(), 1);
    }

    #[test]
    fn test_normalize_strongs() {
        assert_eq!(normalize_strongs("H430").as_deref(), Some("H0430"));
        assert_eq!(normalize_strongs(" g2316 ").as_deref(), Some("G2316"));
        assert_eq!(normalize_strongs("430").as_deref(), Some("H0430"));
        assert_eq!(normalize_strongs("H0430G").as_deref(), Some("H0430"));
        assert_eq!(normalize_strongs("X12"), None);
        assert_eq!(normalize_strongs("H"), None);
    }

    #[test]
    fn test_strongs_index_lists_each_verse_once() {
        let mut idx = StrongsIndex::new();
        idx.add_occurrence("H0430", VerseRef::new("Genesis", 1, 1));
        idx.add_occurrence("H0430", VerseRef::new("Genesis", 1, 1));
        idx.add_occurrence("H0430", VerseRef::new("Genesis", 1, 2));
        assert_eq!(idx.occurrence_count("H430"), 2);
    }

    #[test]
    fn test_book_names() {
        let bible = create_test_bible();
        let names = bible.book_names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"Genesis"));
        assert!(names.contains(&"John"));
    }
}
