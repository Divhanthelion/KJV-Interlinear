//! Words of Christ, from the 1769 KJV's own red-letter markup (eBible.org USFM `\wj`).
//!
//! Each verse maps to the exact substrings spoken by Christ, in order, so a verse
//! with narration between two quotations has two spans.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct WordsOfJesusFile {
    verses: HashMap<String, Vec<String>>,
}

/// Lookup table: (book, chapter, verse) -> spoken substrings in verse order
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RedLetterIndex {
    entries: HashMap<(String, u32, u32), Vec<String>>,
}

impl RedLetterIndex {
    pub fn load(path: &Path) -> Result<Self, String> {
        let file = File::open(path)
            .map_err(|e| format!("Failed to open red-letter data {}: {}", path.display(), e))?;
        let data: WordsOfJesusFile = serde_json::from_reader(BufReader::new(file))
            .map_err(|e| format!("Failed to parse red-letter data: {}", e))?;

        let mut entries = HashMap::with_capacity(data.verses.len());
        for (key, spans) in data.verses {
            let (book, chapter, verse) =
                parse_verse_key(&key).ok_or_else(|| format!("bad red-letter key {:?}", key))?;
            entries.insert((book, chapter, verse), spans);
        }
        Ok(Self { entries })
    }

    /// The spoken substrings of a verse, if Christ speaks in it.
    pub fn get(&self, book: &str, chapter: u32, verse: u32) -> Option<&[String]> {
        self.entries
            .get(&(book.to_string(), chapter, verse))
            .map(Vec::as_slice)
    }

    /// Byte ranges of the spoken words in `text`. Each span is found after the previous
    /// one; a span that doesn't occur is skipped (the data tests guarantee none do).
    pub fn ranges(&self, book: &str, chapter: u32, verse: u32, text: &str) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut from = 0;
        for span in self.get(book, chapter, verse).into_iter().flatten() {
            if let Some(pos) = text[from..].find(span.as_str()) {
                let start = from + pos;
                out.push((start, start + span.len()));
                from = start + span.len();
            }
        }
        out
    }

    /// All (book, chapter, verse) keys in the index.
    pub fn keys(&self) -> impl Iterator<Item = (String, u32, u32)> + '_ {
        self.entries.keys().cloned()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Parse keys like "Matthew 3:15" or "First John 1:1"
fn parse_verse_key(key: &str) -> Option<(String, u32, u32)> {
    let (book_part, ref_part) = key.rsplit_once(' ')?;
    let (chapter_str, verse_str) = ref_part.split_once(':')?;
    let chapter: u32 = chapter_str.parse().ok()?;
    let verse: u32 = verse_str.parse().ok()?;
    Some((book_part.to_string(), chapter, verse))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(entries: &[(&str, u32, u32, &[&str])]) -> RedLetterIndex {
        RedLetterIndex {
            entries: entries
                .iter()
                .map(|(b, c, v, spans)| {
                    ((b.to_string(), *c, *v), spans.iter().map(|s| s.to_string()).collect())
                })
                .collect(),
        }
    }

    #[test]
    fn parse_keys() {
        assert_eq!(parse_verse_key("Matthew 3:15"), Some(("Matthew".to_string(), 3, 15)));
        assert_eq!(parse_verse_key("First John 1:1"), Some(("First John".to_string(), 1, 1)));
        assert_eq!(parse_verse_key("Matthew"), None);
    }

    #[test]
    fn ranges_follow_each_other_in_order() {
        // Acts 1:4: a quotation, narration ("saith he,"), then a second quotation
        let text = "And, being assembled together with them, commanded them that they should not depart from Jerusalem, but wait for the promise of the Father, which, saith he, ye have heard of me.";
        let idx = index(&[("Acts", 1, 4, &["but wait for the promise of the Father, which,", "ye have heard of me."])]);
        let r = idx.ranges("Acts", 1, 4, text);
        assert_eq!(r.len(), 2);
        assert_eq!(&text[r[0].0..r[0].1], "but wait for the promise of the Father, which,");
        assert_eq!(&text[r[1].0..r[1].1], "ye have heard of me.");
    }

    #[test]
    fn repeated_phrase_is_matched_in_sequence() {
        let text = "Verily, verily, I say unto thee, Verily.";
        let idx = index(&[("John", 3, 3, &["Verily,", "Verily."])]);
        let r = idx.ranges("John", 3, 3, text);
        assert_eq!(r, vec![(0, 7), (33, 40)]);
    }

    #[test]
    fn missing_verse_has_no_ranges() {
        let idx = index(&[("Matthew", 1, 1, &["x"])]);
        assert!(idx.get("Matthew", 1, 2).is_none());
        assert!(idx.ranges("Genesis", 1, 1, "In the beginning").is_empty());
    }
}
