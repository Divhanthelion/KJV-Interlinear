//! Ready-to-draw views of the data for the app's interface. Every text decision
//! (red letter, search hits, glosses, names) is made here so it is tested in Rust.

use serde::{Deserialize, Serialize};

use crate::bundle::DataBundle;
use crate::models::{LexiconEntry, OriginalLanguage, Testament, Verse, normalize_strongs};
use crate::text::{Segment, find_folded_ranges, format_gloss, segments};

/// Display name and abbreviation for each book, keyed by the name used in the data.
const BOOK_LABELS: [(&str, &str, &str); 66] = [
    ("Genesis", "Genesis", "Gen"),
    ("Exodus", "Exodus", "Exod"),
    ("Leviticus", "Leviticus", "Lev"),
    ("Numbers", "Numbers", "Num"),
    ("Deuteronomy", "Deuteronomy", "Deut"),
    ("Joshua", "Joshua", "Josh"),
    ("Judges", "Judges", "Judg"),
    ("Ruth", "Ruth", "Ruth"),
    ("First Samuel", "1 Samuel", "1 Sam"),
    ("Second Samuel", "2 Samuel", "2 Sam"),
    ("First Kings", "1 Kings", "1 Kgs"),
    ("Second Kings", "2 Kings", "2 Kgs"),
    ("First Chronicles", "1 Chronicles", "1 Chr"),
    ("Second Chronicles", "2 Chronicles", "2 Chr"),
    ("Ezra", "Ezra", "Ezra"),
    ("Nehemiah", "Nehemiah", "Neh"),
    ("Esther", "Esther", "Esth"),
    ("Job", "Job", "Job"),
    ("Psalms", "Psalms", "Ps"),
    ("Proverbs", "Proverbs", "Prov"),
    ("Ecclesiastes", "Ecclesiastes", "Eccl"),
    ("Song of Solomon", "Song of Solomon", "Song"),
    ("Isaiah", "Isaiah", "Isa"),
    ("Jeremiah", "Jeremiah", "Jer"),
    ("Lamentations", "Lamentations", "Lam"),
    ("Ezekiel", "Ezekiel", "Ezek"),
    ("Daniel", "Daniel", "Dan"),
    ("Hosea", "Hosea", "Hos"),
    ("Joel", "Joel", "Joel"),
    ("Amos", "Amos", "Amos"),
    ("Obadiah", "Obadiah", "Obad"),
    ("Jonah", "Jonah", "Jonah"),
    ("Micah", "Micah", "Mic"),
    ("Nahum", "Nahum", "Nah"),
    ("Habakkuk", "Habakkuk", "Hab"),
    ("Zephaniah", "Zephaniah", "Zeph"),
    ("Haggai", "Haggai", "Hag"),
    ("Zechariah", "Zechariah", "Zech"),
    ("Malachi", "Malachi", "Mal"),
    ("Matthew", "Matthew", "Matt"),
    ("Mark", "Mark", "Mark"),
    ("Luke", "Luke", "Luke"),
    ("John", "John", "John"),
    ("Acts", "Acts", "Acts"),
    ("Romans", "Romans", "Rom"),
    ("First Corinthians", "1 Corinthians", "1 Cor"),
    ("Second Corinthians", "2 Corinthians", "2 Cor"),
    ("Galatians", "Galatians", "Gal"),
    ("Ephesians", "Ephesians", "Eph"),
    ("Philippians", "Philippians", "Phil"),
    ("Colossians", "Colossians", "Col"),
    ("First Thessalonians", "1 Thessalonians", "1 Thess"),
    ("Second Thessalonians", "2 Thessalonians", "2 Thess"),
    ("First Timothy", "1 Timothy", "1 Tim"),
    ("Second Timothy", "2 Timothy", "2 Tim"),
    ("Titus", "Titus", "Titus"),
    ("Philemon", "Philemon", "Phlm"),
    ("Hebrews", "Hebrews", "Heb"),
    ("James", "James", "Jas"),
    ("First Peter", "1 Peter", "1 Pet"),
    ("Second Peter", "2 Peter", "2 Pet"),
    ("First John", "1 John", "1 John"),
    ("Second John", "2 John", "2 John"),
    ("Third John", "3 John", "3 John"),
    ("Jude", "Jude", "Jude"),
    ("Revelation", "Revelation", "Rev"),
];

/// "First Samuel" -> "1 Samuel"
pub fn display_name(book: &str) -> &str {
    BOOK_LABELS
        .iter()
        .find(|(name, _, _)| *name == book)
        .map_or(book, |(_, display, _)| display)
}

fn abbreviation(book: &str) -> &str {
    BOOK_LABELS
        .iter()
        .find(|(name, _, _)| *name == book)
        .map_or(book, |(_, _, abbr)| abbr)
}

/// "Genesis 1", "Psalm 23" (a single psalm is "Psalm")
pub fn chapter_heading(book: &str, chapter: u32) -> String {
    if book == "Psalms" {
        format!("Psalm {}", chapter)
    } else {
        format!("{} {}", display_name(book), chapter)
    }
}

/// "John 3:16", "Psalm 51 (title)" for verse 0
pub fn reference(book: &str, chapter: u32, verse: u32) -> String {
    if verse == 0 {
        format!("{} (title)", chapter_heading(book, chapter))
    } else if book == "Psalms" {
        format!("Psalm {}:{}", chapter, verse)
    } else {
        format!("{} {}:{}", display_name(book), chapter, verse)
    }
}

/// "H0430" -> "H430"
pub fn strongs_display(key: &str) -> String {
    let mut chars = key.chars();
    match chars.next() {
        Some(letter @ ('H' | 'G')) => {
            let digits: String = chars.skip_while(|c| *c == '0').collect();
            if digits.is_empty() {
                format!("{}0", letter)
            } else {
                format!("{}{}", letter, digits)
            }
        }
        _ => key.to_string(),
    }
}

// ---------------------------------------------------------------- books

#[derive(Debug, Serialize)]
pub struct BookInfo {
    /// Key used in every other call ("First Samuel")
    pub name: String,
    /// "1 Samuel"
    pub display: String,
    /// "1 Sam"
    pub abbr: String,
    /// "old" or "new"
    pub testament: &'static str,
    pub chapters: u32,
}

pub fn books(data: &DataBundle) -> Vec<BookInfo> {
    data.bible
        .books
        .iter()
        .map(|b| BookInfo {
            name: b.name.clone(),
            display: display_name(&b.name).to_string(),
            abbr: abbreviation(&b.name).to_string(),
            testament: testament_key(&b.testament),
            chapters: b.chapters.len() as u32,
        })
        .collect()
}

fn testament_key(t: &Testament) -> &'static str {
    match t {
        Testament::Old => "old",
        Testament::New => "new",
    }
}

// ---------------------------------------------------------------- chapter

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ChapterOptions {
    /// Color the words of Christ
    #[serde(default)]
    pub red_letter: bool,
    /// Mark matches of this search query
    #[serde(default)]
    pub query: Option<String>,
    /// Include Hebrew/Greek words
    #[serde(default)]
    pub original: bool,
}

#[derive(Debug, Serialize)]
pub struct WordView {
    pub text: String,
    pub translit: String,
    pub gloss: String,
    /// "H430" for display
    pub strongs: Option<String>,
    /// "H0430" for lookups
    pub key: Option<String>,
    pub morph: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OriginalView {
    /// BCP 47 language code: "he", "arc", or "grc"
    pub lang: &'static str,
    pub words: Vec<WordView>,
}

#[derive(Debug, Serialize)]
pub struct VerseView {
    /// 0 for a Psalm title
    pub number: u32,
    pub segments: Vec<Segment>,
    pub original: Option<OriginalView>,
}

#[derive(Debug, Serialize)]
pub struct ChapterRef {
    pub book: String,
    pub chapter: u32,
}

#[derive(Debug, Serialize)]
pub struct ChapterView {
    pub book: String,
    pub chapter: u32,
    /// "Genesis 1", "Psalm 23"
    pub heading: String,
    pub testament: &'static str,
    pub title: Option<VerseView>,
    pub verses: Vec<VerseView>,
    pub prev: Option<ChapterRef>,
    pub next: Option<ChapterRef>,
}

pub fn chapter(
    data: &DataBundle,
    book: &str,
    chapter: u32,
    options: &ChapterOptions,
) -> Result<ChapterView, String> {
    let books = &data.bible.books;
    let book_index = books
        .iter()
        .position(|b| b.name == book)
        .ok_or_else(|| format!("no book named {:?}", book))?;
    let b = &books[book_index];
    let ch = b
        .chapters
        .iter()
        .find(|c| c.number == chapter)
        .ok_or_else(|| format!("{} has no chapter {}", book, chapter))?;

    let query = options.query.as_deref().map(str::trim).filter(|q| !q.is_empty());
    let view = |v: &Verse| verse_view(data, v, query, options);

    let prev = if chapter > 1 {
        Some(ChapterRef { book: b.name.clone(), chapter: chapter - 1 })
    } else if book_index > 0 {
        let pb = &books[book_index - 1];
        Some(ChapterRef { book: pb.name.clone(), chapter: pb.chapters.len() as u32 })
    } else {
        None
    };
    let next = if (chapter as usize) < b.chapters.len() {
        Some(ChapterRef { book: b.name.clone(), chapter: chapter + 1 })
    } else {
        books
            .get(book_index + 1)
            .map(|nb| ChapterRef { book: nb.name.clone(), chapter: 1 })
    };

    Ok(ChapterView {
        book: b.name.clone(),
        chapter,
        heading: chapter_heading(&b.name, chapter),
        testament: testament_key(&b.testament),
        title: ch.superscription.as_ref().map(view),
        verses: ch.verses.iter().map(view).collect(),
        prev,
        next,
    })
}

fn verse_view(
    data: &DataBundle,
    verse: &Verse,
    query: Option<&str>,
    options: &ChapterOptions,
) -> VerseView {
    let red = if options.red_letter { red_ranges(data, verse) } else { Vec::new() };
    let hits = query.map(|q| find_folded_ranges(&verse.text, q)).unwrap_or_default();
    let original = if options.original { original_view(data, &verse.book, verse.chapter, verse.verse_number) } else { None };
    VerseView {
        number: verse.verse_number,
        segments: segments(&verse.text, &red, &hits),
        original,
    }
}

/// The Hebrew, Aramaic, or Greek words of KJV verse `book` `chapter`:`verse`.
pub fn original_view(data: &DataBundle, book: &str, chapter: u32, verse: u32) -> Option<OriginalView> {
    data.extended.get_interlinear(book, chapter, verse).map(|iv| OriginalView {
        lang: match iv.language {
            OriginalLanguage::Hebrew => "he",
            OriginalLanguage::Aramaic => "arc",
            OriginalLanguage::Greek => "grc",
        },
        words: iv
            .original_words
            .iter()
            .map(|w| WordView {
                text: w.original_text.trim().to_string(),
                translit: w.transliteration.clone(),
                gloss: format_gloss(&w.english_gloss),
                strongs: w.strongs_number.as_deref().map(strongs_display),
                key: w.strongs_number.clone(),
                morph: w.morphology.clone(),
            })
            .collect(),
    })
}

/// Byte ranges spoken by Christ.
fn red_ranges(data: &DataBundle, verse: &Verse) -> Vec<(usize, usize)> {
    data.red_letter
        .ranges(&verse.book, verse.chapter, verse.verse_number, &verse.text)
}

// ---------------------------------------------------------------- search

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    All,
    Book,
    Old,
    New,
}

#[derive(Debug, Serialize)]
pub struct Hit {
    pub book: String,
    pub chapter: u32,
    pub verse: u32,
    /// "John 3:16"
    pub reference: String,
    pub segments: Vec<Segment>,
}

#[derive(Debug, Serialize)]
pub struct SearchResults {
    pub query: String,
    /// All matching verses (Psalm titles count as verses)
    pub total: usize,
    /// The first `limit` matches in canonical order
    pub hits: Vec<Hit>,
}

pub fn search(
    data: &DataBundle,
    query: &str,
    scope: Scope,
    book: Option<&str>,
    limit: usize,
) -> SearchResults {
    let query = query.trim();
    let matches = match scope {
        Scope::All => data.bible.search(query),
        Scope::Book => data.bible.search_in_book(query, book.unwrap_or("")),
        Scope::Old => data.bible.search_in_testament(query, &Testament::Old),
        Scope::New => data.bible.search_in_testament(query, &Testament::New),
    };
    SearchResults {
        query: query.to_string(),
        total: matches.len(),
        hits: matches
            .into_iter()
            .take(limit)
            .map(|v| Hit {
                book: v.book.clone(),
                chapter: v.chapter,
                verse: v.verse_number,
                reference: reference(&v.book, v.chapter, v.verse_number),
                segments: segments(&v.text, &[], &find_folded_ranges(&v.text, query)),
            })
            .collect(),
    }
}

// ---------------------------------------------------------------- Strong's

#[derive(Debug, Serialize)]
pub struct LexiconView {
    /// "H430"
    pub strongs: String,
    /// "H0430"
    pub key: String,
    /// BCP 47 code of the headword
    pub lang: &'static str,
    pub word: String,
    pub translit: String,
    pub morph: String,
    pub gloss: String,
    pub definition: String,
}

pub fn lexicon(data: &DataBundle, strongs: &str) -> Option<LexiconView> {
    let key = normalize_strongs(strongs)?;
    let entry: &LexiconEntry = data.extended.get_lexicon_entry(&key)?;
    Some(LexiconView {
        strongs: strongs_display(&key),
        lang: if key.starts_with('H') { "he" } else { "grc" },
        key,
        word: entry.original_word.clone(),
        translit: entry.transliteration.clone(),
        morph: entry.morph.clone(),
        gloss: entry.gloss.clone(),
        definition: entry.definition.clone(),
    })
}

#[derive(Debug, Serialize)]
pub struct StrongsResults {
    /// None when the query isn't a Strong's number
    pub key: Option<String>,
    pub strongs: Option<String>,
    pub lexicon: Option<LexiconView>,
    /// All verses containing the number
    pub total: usize,
    pub hits: Vec<Hit>,
}

/// Verses containing a Strong's number ("H430", "h0430", "430" = Hebrew).
pub fn strongs_search(data: &DataBundle, query: &str, limit: usize) -> StrongsResults {
    let Some(key) = normalize_strongs(query) else {
        return StrongsResults { key: None, strongs: None, lexicon: None, total: 0, hits: Vec::new() };
    };
    let refs = data.extended.strongs_index.get_occurrences(&key);
    let hits = refs
        .into_iter()
        .flatten()
        .take(limit)
        .filter_map(|r| {
            let text = verse_text(data, &r.book, r.chapter, r.verse)?;
            Some(Hit {
                book: r.book.clone(),
                chapter: r.chapter,
                verse: r.verse,
                reference: reference(&r.book, r.chapter, r.verse),
                segments: segments(text, &[], &[]),
            })
        })
        .collect();
    StrongsResults {
        strongs: Some(strongs_display(&key)),
        lexicon: lexicon(data, &key),
        total: refs.map_or(0, Vec::len),
        key: Some(key),
        hits,
    }
}

/// Text of a verse, or of a Psalm title for verse 0.
pub fn verse_text<'a>(data: &'a DataBundle, book: &str, chapter: u32, verse: u32) -> Option<&'a str> {
    let ch = data.bible.get_chapter(book, chapter)?;
    if verse == 0 {
        ch.superscription.as_ref().map(|v| v.text.as_str())
    } else {
        ch.verses.iter().find(|v| v.verse_number == verse).map(|v| v.text.as_str())
    }
}

/// Clipboard text for one verse: "John 3:16 KJV\nFor God so loved…"
pub fn copy_verse(data: &DataBundle, book: &str, chapter: u32, verse: u32) -> Option<String> {
    let text = verse_text(data, book, chapter, verse)?;
    Some(format!("{} KJV\n{}", reference(book, chapter, verse), text))
}

/// Clipboard text for a chapter, one numbered verse per line, title first.
pub fn copy_chapter(data: &DataBundle, book: &str, chapter: u32) -> Option<String> {
    let ch = data.bible.get_chapter(book, chapter)?;
    let mut out = format!("{} KJV\n", chapter_heading(book, chapter));
    if let Some(title) = &ch.superscription {
        out.push_str(&title.text);
        out.push('\n');
    }
    for v in &ch.verses {
        out.push_str(&format!("{} {}\n", v.verse_number, v.text));
    }
    Some(out)
}
