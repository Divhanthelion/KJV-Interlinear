use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufRead};
use std::path::Path;

use crate::models::{Bible, Book, Chapter, Testament, Verse};

impl Bible {
    /// Load Bible from Old and New Testament directories
    pub fn from_directories(old_testament_path: &Path, new_testament_path: &Path) -> io::Result<Self> {
        let mut bible = Bible { books: Vec::new() };

        // Get the standard book order
        let book_order = get_standard_book_order();

        // Read Old Testament books
        read_testament_books(&mut bible, old_testament_path, Testament::Old)?;

        // Read New Testament books
        read_testament_books(&mut bible, new_testament_path, Testament::New)?;

        // Sort books according to the standard biblical order
        bible.books.sort_by(|a, b| {
            let a_order = book_order.get(&a.name).unwrap_or(&999);
            let b_order = book_order.get(&b.name).unwrap_or(&999);
            a_order.cmp(b_order)
        });

        Ok(bible)
    }
}

fn read_testament_books(bible: &mut Bible, testament_path: &Path, testament: Testament) -> io::Result<()> {
    for entry in fs::read_dir(testament_path)? {
        let entry = entry?;
        let file_path = entry.path();

        // Skip .DS_Store files and Zone.Identifier files (Windows metadata)
        if let Some(file_name) = file_path.file_name().and_then(|n| n.to_str())
            && (file_name == ".DS_Store" || file_name.contains("Zone.Identifier"))
        {
            continue;
        }

        if file_path.is_file()
            && let Some(book_name) = file_path.file_stem().and_then(|s| s.to_str())
        {
            let book = parse_book_file(&file_path, book_name.to_string(), testament.clone())?;
            bible.books.push(book);
        }
    }

    Ok(())
}

fn parse_book_file(file_path: &Path, book_name: String, testament: Testament) -> io::Result<Book> {
    let file = File::open(file_path)?;

    let mut book = Book { name: book_name, testament, chapters: Vec::new() };

    let reader = io::BufReader::new(file);
    for line_result in reader.lines() {
        let line = line_result?;

        // Skip empty lines
        let line = line.trim_start_matches('\u{FEFF}').trim();
        if line.is_empty() {
            continue;
        }

        // Format: "chapter:verse text"; verse 0 holds a Psalm superscription
        let Some((chapter_num, verse_num, text)) = parse_line(line) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}: malformed line {:?}", file_path.display(), line),
            ));
        };

        while book.chapters.len() < chapter_num as usize {
            book.chapters.push(Chapter {
                number: book.chapters.len() as u32 + 1,
                superscription: None,
                verses: Vec::new(),
            });
        }

        let verse = Verse { book: book.name.clone(), chapter: chapter_num, verse_number: verse_num, text };
        let chapter = &mut book.chapters[chapter_num as usize - 1];
        if verse_num == 0 {
            chapter.superscription = Some(verse);
        } else {
            chapter.verses.push(verse);
        }
    }

    Ok(book)
}

/// Parse "12:3 verse text" into (chapter, verse, text with whitespace collapsed).
fn parse_line(line: &str) -> Option<(u32, u32, String)> {
    let (reference, text) = line.split_once(' ')?;
    let (chapter_str, verse_str) = reference.split_once(':')?;
    let chapter: u32 = chapter_str.parse().ok()?;
    let verse: u32 = verse_str.parse().ok()?;
    if chapter == 0 || chapter > 200 || verse > 200 {
        return None;
    }
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return None;
    }
    Some((chapter, verse, text))
}

/// The 66 books in canonical order, named as in the text files.
pub const CANONICAL_BOOKS: [&str; 66] = [
    // Old Testament (39 books)
    "Genesis",
    "Exodus",
    "Leviticus",
    "Numbers",
    "Deuteronomy",
    "Joshua",
    "Judges",
    "Ruth",
    "First Samuel",
    "Second Samuel",
    "First Kings",
    "Second Kings",
    "First Chronicles",
    "Second Chronicles",
    "Ezra",
    "Nehemiah",
    "Esther",
    "Job",
    "Psalms",
    "Proverbs",
    "Ecclesiastes",
    "Song of Solomon",
    "Isaiah",
    "Jeremiah",
    "Lamentations",
    "Ezekiel",
    "Daniel",
    "Hosea",
    "Joel",
    "Amos",
    "Obadiah",
    "Jonah",
    "Micah",
    "Nahum",
    "Habakkuk",
    "Zephaniah",
    "Haggai",
    "Zechariah",
    "Malachi",
    // New Testament (27 books)
    "Matthew",
    "Mark",
    "Luke",
    "John",
    "Acts",
    "Romans",
    "First Corinthians",
    "Second Corinthians",
    "Galatians",
    "Ephesians",
    "Philippians",
    "Colossians",
    "First Thessalonians",
    "Second Thessalonians",
    "First Timothy",
    "Second Timothy",
    "Titus",
    "Philemon",
    "Hebrews",
    "James",
    "First Peter",
    "Second Peter",
    "First John",
    "Second John",
    "Third John",
    "Jude",
    "Revelation",
];

/// Position of a book in canonical order (Genesis = 0, Revelation = 65).
pub fn canonical_book_index(name: &str) -> Option<usize> {
    CANONICAL_BOOKS.iter().position(|b| *b == name)
}

/// Returns a mapping of book names to their canonical order
fn get_standard_book_order() -> HashMap<String, usize> {
    let books = CANONICAL_BOOKS;

    books.iter().enumerate().map(|(i, &name)| (name.to_string(), i)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_book_order_has_66_books() {
        let order = get_standard_book_order();
        assert_eq!(order.len(), 66);
    }

    #[test]
    fn test_genesis_is_first() {
        let order = get_standard_book_order();
        assert_eq!(order.get("Genesis"), Some(&0));
    }

    #[test]
    fn test_parse_line() {
        assert_eq!(parse_line("3:16 For God so  loved"), Some((3, 16, "For God so loved".to_string())));
        assert_eq!(parse_line("51:0 To the chief Musician"), Some((51, 0, "To the chief Musician".to_string())));
        assert_eq!(parse_line("chapter one"), None);
        assert_eq!(parse_line("0:1 text"), None);
        assert_eq!(parse_line("1:1 "), None);
    }

    #[test]
    fn test_revelation_is_last() {
        let order = get_standard_book_order();
        assert_eq!(order.get("Revelation"), Some(&65));
    }
}
