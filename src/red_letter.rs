//! Words-of-Christ (red letter) index loaded from kjvstudy.org data.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use serde::Deserialize;

/// How Christ's words appear in a verse
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedLetterSpec {
    /// The entire verse is spoken by Christ
    Full,
    /// Exact substring of the verse that is spoken by Christ
    Quote(String),
}

#[derive(Debug, Deserialize)]
struct RedLetterFile {
    verses: HashMap<String, String>,
}

/// Lookup table: (book, chapter, verse) -> red-letter spec
#[derive(Debug, Clone, Default)]
pub struct RedLetterIndex {
    entries: HashMap<(String, u32, u32), RedLetterSpec>,
}

impl RedLetterIndex {
    pub fn load(path: &Path) -> Result<Self, String> {
        let file = File::open(path)
            .map_err(|e| format!("Failed to open red-letter data {}: {}", path.display(), e))?;
        let reader = BufReader::new(file);
        let data: RedLetterFile = serde_json::from_reader(reader)
            .map_err(|e| format!("Failed to parse red-letter data: {}", e))?;

        let mut entries = HashMap::with_capacity(data.verses.len());
        for (key, value) in data.verses {
            if let Some((book, chapter, verse)) = parse_verse_key(&key) {
                let spec = if value == "full" {
                    RedLetterSpec::Full
                } else {
                    RedLetterSpec::Quote(value)
                };
                entries.insert((book, chapter, verse), spec);
            }
        }

        Ok(Self { entries })
    }

    pub fn get(&self, book: &str, chapter: u32, verse: u32) -> Option<&RedLetterSpec> {
        self.entries.get(&(book.to_string(), chapter, verse))
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

/// Fold typographic variants so kjvstudy quotes match Project Gutenberg text.
fn fold_char(c: char) -> Option<char> {
    match c {
        '\u{2018}' | '\u{2019}' | '\u{201B}' | '\u{2032}' | '\u{02BC}' => Some('\''),
        '\u{201C}' | '\u{201D}' | '\u{201F}' => Some('"'),
        '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' => Some('-'),
        // Collapse all whitespace to a single space sentinel handled by fold_string
        c if c.is_whitespace() => Some(' '),
        other => Some(other.to_ascii_lowercase()),
    }
}

/// Folded form plus map from each folded char index -> byte range in original.
fn fold_with_map(s: &str) -> (String, Vec<(usize, usize)>) {
    let mut folded = String::new();
    let mut map = Vec::new();
    let mut last_was_space = false;

    for (i, ch) in s.char_indices() {
        let end = i + ch.len_utf8();
        let Some(fc) = fold_char(ch) else {
            continue;
        };
        if fc == ' ' {
            if last_was_space || folded.is_empty() {
                continue;
            }
            last_was_space = true;
            folded.push(' ');
            map.push((i, end));
            continue;
        }
        last_was_space = false;
        folded.push(fc);
        map.push((i, end));
    }

    while folded.ends_with(' ') {
        folded.pop();
        map.pop();
    }

    (folded, map)
}

fn fold_string(s: &str) -> String {
    fold_with_map(s).0
}

/// Find `needle` in `haystack` allowing folded apostrophe/dash/case/whitespace.
/// Returns byte range in the original haystack.
fn find_folded(haystack: &str, needle: &str) -> Option<(usize, usize)> {
    if needle.is_empty() {
        return None;
    }

    // Fast path: exact match
    if let Some(pos) = haystack.find(needle) {
        return Some((pos, pos + needle.len()));
    }

    let (hay_folded, hay_map) = fold_with_map(haystack);
    let needle_folded = fold_string(needle);

    if !needle_folded.is_empty()
        && let Some(byte_pos) = hay_folded.find(&needle_folded) {
            let char_pos = hay_folded[..byte_pos].chars().count();
            let needle_chars = needle_folded.chars().count();
            let start = hay_map[char_pos].0;
            let end = hay_map[char_pos + needle_chars - 1].1;
            return Some((start, end));
        }

    // Trailing punctuation drift (e.g. "cumi." vs "cumi;")
    let trimmed = needle.trim_end_matches(|c: char| {
        matches!(c, '.' | ';' | ',' | ':' | '!' | '?' | '"' | '\'')
    });
    if trimmed.len() < needle.len() && !trimmed.is_empty()
        && let Some(r) = find_folded(haystack, trimmed) {
            return Some(r);
        }

    // Hyphen optional: "Bar-jona" vs "Barjona"
    if needle.contains('-') {
        let dehyphen: String = needle.chars().filter(|&c| c != '-').collect();
        if dehyphen != needle
            && let Some(r) = find_folded(haystack, &dehyphen) {
                return Some(r);
            }
    }

    // Truncated quotes in source JSON: shorten from the end by words until it matches
    let words: Vec<&str> = needle.split_whitespace().collect();
    if words.len() >= 4 {
        for keep in (3..words.len()).rev() {
            let candidate = words[..keep].join(" ");
            if candidate.len() < 12 {
                break;
            }
            // Avoid infinite recursion into this branch: use folded-only search
            let (hay_folded, hay_map) = fold_with_map(haystack);
            let needle_folded = fold_string(&candidate);
            if needle_folded.is_empty() {
                continue;
            }
            if let Some(byte_pos) = hay_folded.find(&needle_folded) {
                let char_pos = hay_folded[..byte_pos].chars().count();
                let needle_chars = needle_folded.chars().count();
                let start = hay_map[char_pos].0;
                let end = hay_map[char_pos + needle_chars - 1].1;
                return Some((start, end));
            }
            if candidate.contains('-') {
                let dehyphen: String = candidate.chars().filter(|&c| c != '-').collect();
                let needle_folded = fold_string(&dehyphen);
                if let Some(byte_pos) = hay_folded.find(&needle_folded) {
                    let char_pos = hay_folded[..byte_pos].chars().count();
                    let needle_chars = needle_folded.chars().count();
                    let start = hay_map[char_pos].0;
                    let end = hay_map[char_pos + needle_chars - 1].1;
                    return Some((start, end));
                }
            }
        }
    }

    None
}

/// Split verse text into (segment, is_red) runs for painting.
///
/// If the quote substring is not found in the Gutenberg text, returns a single
/// non-red segment (never paint the whole verse as a fallback).
pub fn red_letter_segments<'a>(
    verse_text: &'a str,
    spec: &RedLetterSpec,
) -> Vec<(&'a str, bool)> {
    match spec {
        RedLetterSpec::Full => vec![(verse_text, true)],
        RedLetterSpec::Quote(quote) => {
            if quote.is_empty() {
                return vec![(verse_text, false)];
            }
            if let Some((pos, end)) = find_folded(verse_text, quote) {
                let mut parts = Vec::new();
                if pos > 0 {
                    parts.push((&verse_text[..pos], false));
                }
                parts.push((&verse_text[pos..end], true));
                if end < verse_text.len() {
                    parts.push((&verse_text[end..], false));
                }
                parts
            } else {
                // Leave uncolored rather than over-paint
                vec![(verse_text, false)]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_key() {
        assert_eq!(
            parse_verse_key("Matthew 3:15"),
            Some(("Matthew".to_string(), 3, 15))
        );
    }

    #[test]
    fn parse_numbered_book_key() {
        assert_eq!(
            parse_verse_key("First John 1:1"),
            Some(("First John".to_string(), 1, 1))
        );
    }

    #[test]
    fn segments_full() {
        let segs = red_letter_segments("Hello world", &RedLetterSpec::Full);
        assert_eq!(segs, vec![("Hello world", true)]);
    }

    #[test]
    fn segments_partial() {
        let text = "But he answered and said, It is written, Man shall not live by bread alone.";
        let quote = "It is written, Man shall not live by bread alone.";
        let segs = red_letter_segments(text, &RedLetterSpec::Quote(quote.to_string()));
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0], ("But he answered and said, ", false));
        assert!(segs[1].1);
        assert!(segs[1].0.starts_with("It is written"));
    }

    #[test]
    fn segments_curly_apostrophe() {
        let text = "Saying, The scribes and the Pharisees sit in Moses\u{2019} seat:";
        let quote = "The scribes and the Pharisees sit in Moses' seat:";
        let segs = red_letter_segments(text, &RedLetterSpec::Quote(quote.to_string()));
        assert_eq!(segs.len(), 2);
        assert!(segs[1].1);
        assert!(segs[1].0.contains("Moses"));
    }

    #[test]
    fn segments_barjona_hyphen() {
        let text = "And Jesus answered and said unto him, Blessed art thou, Simon Barjona: for flesh and blood hath not revealed it unto thee, but my Father which is in heaven.";
        let quote = "Blessed art thou, Simon Bar-jona: for flesh and blood hath not revealed it unto thee, but my Father ";
        let segs = red_letter_segments(text, &RedLetterSpec::Quote(quote.to_string()));
        assert!(segs.iter().any(|(_, red)| *red));
    }

    #[test]
    fn segments_case_and_double_space() {
        let text = "Then Jesus said unto them, Yet a little while is the light with you.  Walk while ye have the light, lest darkness come upon you:";
        let quote = "Yet a little while is the light with you. Walk while ye have the light, lest darkness come upon you:";
        let segs = red_letter_segments(text, &RedLetterSpec::Quote(quote.to_string()));
        assert_eq!(segs.len(), 2);
        assert!(segs[1].1);
    }

    #[test]
    fn segments_miss_stays_normal() {
        let segs = red_letter_segments(
            "Actual verse text",
            &RedLetterSpec::Quote("not in the verse".to_string()),
        );
        assert_eq!(segs, vec![("Actual verse text", false)]);
    }

    #[test]
    fn find_folded_multibyte_before_match() {
        // Non-ASCII prefix makes folded byte offset ≠ char index; must not panic.
        let text = "And he said, °It is written, Man shall not live by bread alone.";
        let quote = "it is written, man shall not live by bread alone.";
        let segs = red_letter_segments(text, &RedLetterSpec::Quote(quote.to_string()));
        assert_eq!(segs.len(), 2);
        assert!(!segs[0].1);
        assert!(segs[1].1);
        assert!(segs[1].0.starts_with("It is written") || segs[1].0.contains("It is written"));
        assert!(segs[0].0.contains('°'));
    }

    #[test]
    fn non_jesus_verse_has_no_entry() {
        let mut idx = RedLetterIndex::default();
        idx.entries
            .insert(("Matthew".to_string(), 1, 1), RedLetterSpec::Full);
        assert!(idx.get("Matthew", 1, 2).is_none());
        assert!(idx.get("Genesis", 1, 1).is_none());
    }
}
