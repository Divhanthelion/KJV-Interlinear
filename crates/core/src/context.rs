//! Scripture context for AI chat: turns a scope (a verse, a chapter, some books, the
//! whole Bible) into plain text a language model can read, with a size estimate so
//! the app can tell whether it fits the model's context window.

use serde::{Deserialize, Serialize};

use crate::api::{chapter_heading, display_name, reference, strongs_display};
use crate::bundle::DataBundle;
use crate::models::{Book, Chapter, OriginalLanguage, Verse};
use crate::text::format_gloss;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Scope {
    /// Nothing attached
    None,
    Verse {
        book: String,
        chapter: u32,
        verse: u32,
    },
    /// Verses `from..=to` of one chapter
    Verses {
        book: String,
        chapter: u32,
        from: u32,
        to: u32,
    },
    Chapter {
        book: String,
        chapter: u32,
    },
    Book {
        book: String,
    },
    /// Whole books, in canonical order whatever order they are given in
    Books {
        books: Vec<String>,
    },
    Bible,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ContextOptions {
    /// Add each verse's Hebrew/Greek words with Strong's numbers and glosses
    pub original: bool,
}

#[derive(Debug, Serialize)]
pub struct ContextText {
    /// "John 3", "Genesis–Deuteronomy", "The whole Bible"
    pub label: String,
    pub text: String,
    pub verses: usize,
    /// Rough token count for typical tokenizers (see `estimate_tokens`)
    pub tokens: usize,
}

/// Size of a scope without building its text for the caller.
#[derive(Debug, Serialize)]
pub struct ContextSize {
    pub label: String,
    pub verses: usize,
    pub tokens: usize,
}

/// Build the text for `scope`. Errors name the book or chapter that doesn't exist.
pub fn build(data: &DataBundle, scope: &Scope, options: &ContextOptions) -> Result<ContextText, String> {
    let mut out = Writer { data, options, text: String::new(), verses: 0 };
    let label = match scope {
        Scope::None => String::new(),
        Scope::Verse { book, chapter, verse } => {
            let (b, ch) = find_chapter(data, book, *chapter)?;
            let v = find_verse(ch, *verse)
                .ok_or_else(|| format!("{} has no verse {}", chapter_heading(book, *chapter), verse))?;
            out.book_heading(b);
            out.chapter_heading(b, ch);
            out.verse(v);
            reference(book, *chapter, *verse)
        }
        Scope::Verses { book, chapter, from, to } => {
            let (b, ch) = find_chapter(data, book, *chapter)?;
            let (from, to) = (*from.min(to), *from.max(to));
            let picked: Vec<&Verse> = all_verses(ch).filter(|v| (from..=to).contains(&v.verse_number)).collect();
            if picked.is_empty() {
                return Err(format!("{} has no verses {}–{}", chapter_heading(book, *chapter), from, to));
            }
            out.book_heading(b);
            out.chapter_heading(b, ch);
            // Label what is attached: John 3:30–99 is John 3:30–36
            let (first, last) = (picked[0].verse_number, picked[picked.len() - 1].verse_number);
            for v in picked {
                out.verse(v);
            }
            verses_label(book, *chapter, first, last)
        }
        Scope::Chapter { book, chapter } => {
            let (b, ch) = find_chapter(data, book, *chapter)?;
            out.book_heading(b);
            out.chapter(b, ch);
            chapter_heading(book, *chapter)
        }
        Scope::Book { book } => {
            let b = find_book(data, book)?;
            out.book(b);
            display_name(book).to_string()
        }
        Scope::Books { books } => {
            for name in books {
                find_book(data, name)?;
            }
            let picked: Vec<&Book> = data.bible.books.iter().filter(|b| books.contains(&b.name)).collect();
            for b in &picked {
                out.book(b);
            }
            books_label(data, &picked)
        }
        Scope::Bible => {
            for b in &data.bible.books {
                out.book(b);
            }
            "The whole Bible".to_string()
        }
    };
    let tokens = estimate_tokens(&out.text);
    Ok(ContextText { label, verses: out.verses, tokens, text: out.text })
}

pub fn size(data: &DataBundle, scope: &Scope, options: &ContextOptions) -> Result<ContextSize, String> {
    let c = build(data, scope, options)?;
    Ok(ContextSize { label: c.label, verses: c.verses, tokens: c.tokens })
}

/// Tokens for `text`, erring a little high. Measured on Qwen's tokenizer: English
/// KJV runs 3.9 characters per token (the whole Bible is ~1.09M tokens); pointed
/// Hebrew and polytonic Greek cost ~1.8 tokens per character because every vowel
/// point and accent is its own code point. The app corrects this per model from the
/// token counts providers report.
pub fn estimate_tokens(text: &str) -> usize {
    let (mut ascii, mut other) = (0usize, 0usize);
    for c in text.chars() {
        if c.is_ascii() {
            ascii += 1;
        } else {
            other += 1;
        }
    }
    (ascii * 10).div_ceil(38) + (other * 9).div_ceil(5)
}

/// "Romans 8:28–30", "John 3:36", "Psalm 51 (title)", "Psalm 51:1–3 (with title)".
fn verses_label(book: &str, chapter: u32, first: u32, last: u32) -> String {
    let range = |from: u32| {
        if from == last {
            reference(book, chapter, from)
        } else {
            format!("{}–{}", reference(book, chapter, from), last)
        }
    };
    match (first, last) {
        (0, 0) => reference(book, chapter, 0),
        (0, _) => format!("{} (with title)", range(1)),
        _ => range(first),
    }
}

/// Consecutive books as ranges: "Genesis–Deuteronomy", "Matthew–John, Romans–Jude",
/// "Ruth, Esther".
fn books_label(data: &DataBundle, picked: &[&Book]) -> String {
    if picked.len() == data.bible.books.len() {
        return "The whole Bible".to_string();
    }
    let index = |b: &Book| data.bible.books.iter().position(|x| x.name == b.name).unwrap_or(0);
    let mut runs: Vec<Vec<&Book>> = Vec::new();
    for b in picked {
        match runs.last_mut() {
            Some(run) if index(run[run.len() - 1]) + 1 == index(b) => run.push(b),
            _ => runs.push(vec![b]),
        }
    }
    runs.iter()
        .map(|run| {
            let first = display_name(&run[0].name);
            let last = display_name(&run[run.len() - 1].name);
            match run.len() {
                1 => first.to_string(),
                2 => format!("{}, {}", first, last),
                _ => format!("{}–{}", first, last),
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn find_book<'a>(data: &'a DataBundle, name: &str) -> Result<&'a Book, String> {
    data.bible.books.iter().find(|b| b.name == name).ok_or_else(|| format!("no book named {:?}", name))
}

fn find_chapter<'a>(data: &'a DataBundle, book: &str, chapter: u32) -> Result<(&'a Book, &'a Chapter), String> {
    let b = find_book(data, book)?;
    let ch = b
        .chapters
        .iter()
        .find(|c| c.number == chapter)
        .ok_or_else(|| format!("{} has no chapter {}", display_name(book), chapter))?;
    Ok((b, ch))
}

/// The Psalm title (verse 0) first, then the verses.
fn all_verses(ch: &Chapter) -> impl Iterator<Item = &Verse> {
    ch.superscription.iter().chain(ch.verses.iter())
}

fn find_verse(ch: &Chapter, verse: u32) -> Option<&Verse> {
    all_verses(ch).find(|v| v.verse_number == verse)
}

struct Writer<'a> {
    data: &'a DataBundle,
    options: &'a ContextOptions,
    text: String,
    verses: usize,
}

impl Writer<'_> {
    fn book(&mut self, b: &Book) {
        self.book_heading(b);
        for ch in &b.chapters {
            self.chapter(b, ch);
        }
    }

    fn book_heading(&mut self, b: &Book) {
        if !self.text.is_empty() {
            self.text.push('\n');
        }
        self.text.push_str("# ");
        self.text.push_str(display_name(&b.name));
        self.text.push('\n');
    }

    fn chapter_heading(&mut self, b: &Book, ch: &Chapter) {
        self.text.push_str("## ");
        self.text.push_str(&chapter_heading(&b.name, ch.number));
        self.text.push('\n');
    }

    fn chapter(&mut self, b: &Book, ch: &Chapter) {
        self.chapter_heading(b, ch);
        for v in all_verses(ch) {
            self.verse(v);
        }
    }

    /// "16 For God so loved…"; a Psalm title is "(title) To the chief Musician…"
    fn verse(&mut self, v: &Verse) {
        if v.verse_number == 0 {
            self.text.push_str("(title) ");
        } else {
            self.text.push_str(&v.verse_number.to_string());
            self.text.push(' ');
        }
        self.text.push_str(&v.text);
        self.text.push('\n');
        self.verses += 1;
        if self.options.original {
            self.original(v);
        }
    }

    /// "   Hebrew: בְּרֵאשִׁית H7225 in beginning | בָּרָא H1254 created | …"
    fn original(&mut self, v: &Verse) {
        let Some(iv) = self.data.extended.get_interlinear(&v.book, v.chapter, v.verse_number) else {
            return;
        };
        let language = match iv.language {
            OriginalLanguage::Hebrew => "Hebrew",
            OriginalLanguage::Aramaic => "Aramaic",
            OriginalLanguage::Greek => "Greek",
        };
        let words: Vec<String> = iv
            .original_words
            .iter()
            .map(|w| {
                let mut word = w.original_text.trim().to_string();
                if let Some(s) = &w.strongs_number {
                    word.push(' ');
                    word.push_str(&strongs_display(s));
                }
                let gloss = format_gloss(&w.english_gloss);
                if !gloss.is_empty() {
                    word.push(' ');
                    word.push_str(&gloss);
                }
                word
            })
            .collect();
        if !words.is_empty() {
            self.text.push_str("   ");
            self.text.push_str(language);
            self.text.push_str(": ");
            self.text.push_str(&words.join(" | "));
            self.text.push('\n');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::estimate_tokens;

    #[test]
    fn estimates_err_high_for_pointed_hebrew() {
        assert_eq!(estimate_tokens("In the beginning"), 5);
        // 11 code points of pointed Hebrew
        assert_eq!(estimate_tokens("בְּרֵאשִׁית"), 20);
    }
}
