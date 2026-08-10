//! Loader for STEP Bible TSV data files
//!
//! Parses TAHOT (Hebrew OT), TAGNT (Greek NT), and lexicon files.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::LazyLock;

use crate::models::{
    ExtendedBible, InterlinearVerse, LexiconEntry, OriginalLanguage, OriginalWord, StrongsIndex,
    VerseRef,
};

/// Book name mapping from STEP Bible abbreviations to standard names.
/// These must match the book names used in the KJV text files.
static BOOK_NAME_MAPPING: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    // Old Testament
    map.insert("Gen", "Genesis");
    map.insert("Exo", "Exodus");
    map.insert("Lev", "Leviticus");
    map.insert("Num", "Numbers");
    map.insert("Deu", "Deuteronomy");
    map.insert("Jos", "Joshua");
    map.insert("Jdg", "Judges");
    map.insert("Rut", "Ruth");
    map.insert("1Sa", "First Samuel");
    map.insert("2Sa", "Second Samuel");
    map.insert("1Ki", "First Kings");
    map.insert("2Ki", "Second Kings");
    map.insert("1Ch", "First Chronicles");
    map.insert("2Ch", "Second Chronicles");
    map.insert("Ezr", "Ezra");
    map.insert("Neh", "Nehemiah");
    map.insert("Est", "Esther");
    map.insert("Job", "Job");
    map.insert("Psa", "Psalms");
    map.insert("Pro", "Proverbs");
    map.insert("Ecc", "Ecclesiastes");
    map.insert("Sng", "Song of Solomon");
    map.insert("Isa", "Isaiah");
    map.insert("Jer", "Jeremiah");
    map.insert("Lam", "Lamentations");
    map.insert("Ezk", "Ezekiel");
    map.insert("Dan", "Daniel");
    map.insert("Hos", "Hosea");
    map.insert("Jol", "Joel");
    map.insert("Amo", "Amos");
    map.insert("Oba", "Obadiah");
    map.insert("Jon", "Jonah");
    map.insert("Mic", "Micah");
    map.insert("Nah", "Nahum");
    map.insert("Nam", "Nahum"); // Alternative abbreviation in some STEP files
    map.insert("Hab", "Habakkuk");
    map.insert("Zep", "Zephaniah");
    map.insert("Hag", "Haggai");
    map.insert("Zec", "Zechariah");
    map.insert("Mal", "Malachi");
    // New Testament
    map.insert("Mat", "Matthew");
    map.insert("Mrk", "Mark");
    map.insert("Luk", "Luke");
    map.insert("Jhn", "John");
    map.insert("Act", "Acts");
    map.insert("Rom", "Romans");
    map.insert("1Co", "First Corinthians");
    map.insert("2Co", "Second Corinthians");
    map.insert("Gal", "Galatians");
    map.insert("Eph", "Ephesians");
    map.insert("Php", "Philippians");
    map.insert("Col", "Colossians");
    map.insert("1Th", "First Thessalonians");
    map.insert("2Th", "Second Thessalonians");
    map.insert("1Ti", "First Timothy");
    map.insert("2Ti", "Second Timothy");
    map.insert("Tit", "Titus");
    map.insert("Phm", "Philemon");
    map.insert("Heb", "Hebrews");
    map.insert("Jas", "James");
    map.insert("1Pe", "First Peter");
    map.insert("2Pe", "Second Peter");
    map.insert("1Jn", "First John");
    map.insert("2Jn", "Second John");
    map.insert("3Jn", "Third John");
    map.insert("Jud", "Jude");
    map.insert("Rev", "Revelation");
    map
});

/// Parse a STEP Bible reference like "Gen.1.1#01=L" or "Mat.1.1#01=NKO"
fn parse_reference(reference: &str) -> Option<(String, u32, u32, u32)> {
    // Format: Book.Chapter.Verse#WordNum=Type
    let book_mapping = &*BOOK_NAME_MAPPING;

    // Split on # to separate reference from word number
    let parts: Vec<&str> = reference.split('#').collect();
    if parts.len() < 2 {
        return None;
    }

    // Parse book.chapter.verse
    let ref_parts: Vec<&str> = parts[0].split('.').collect();
    if ref_parts.len() < 3 {
        return None;
    }

    let book_abbrev = ref_parts[0];
    let book_name = book_mapping.get(book_abbrev).unwrap_or(&book_abbrev);

    let chapter: u32 = ref_parts[1].parse().ok()?;
    let verse: u32 = ref_parts[2].parse().ok()?;

    // Parse word number (remove =Type suffix)
    let word_part = parts[1].split('=').next()?;
    let word_num: u32 = word_part.parse().ok()?;

    Some((book_name.to_string(), chapter, verse, word_num))
}

/// Extract the primary Strong's number from a dStrongs field
/// Examples: "H9003/{H7225G}" -> "H7225", "{H1254A}" -> "H1254", "G0976=N-NSF" -> "G0976"
fn extract_strongs_number(dstrongs: &str) -> Option<String> {
    // Look for patterns like H1234, G1234, possibly with letter suffix
    let mut result = String::new();

    let cleaned = dstrongs.replace(['{', '}'], "").replace('/', " ");

    for part in cleaned.split_whitespace() {
        // Skip prefix markers like H9003 (preposition markers)
        if part.starts_with("H900") || part.starts_with("H901") {
            continue;
        }

        // Find H or G followed by numbers
        if let Some(pos) = part.find(['H', 'G']) {
            let substr = &part[pos..];
            let mut num = String::new();
            num.push(substr.chars().next()?);

            for c in substr.chars().skip(1) {
                if c.is_ascii_digit() {
                    num.push(c);
                } else {
                    break;
                }
            }

            if num.len() > 1 {
                result = num;
                break;
            }
        }
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// Clean Hebrew text (remove forward slashes used for prefix/suffix markers)
fn clean_hebrew_text(text: &str) -> String {
    text.replace(['/', '\\'], "")
}

/// Clean Greek text (remove parenthetical transliteration)
fn clean_greek_text(text: &str) -> String {
    if let Some(paren_pos) = text.find('(') {
        text[..paren_pos].trim().to_string()
    } else {
        text.to_string()
    }
}

/// Extract transliteration from Greek field like "Βίβλος (Biblos)"
fn extract_greek_transliteration(text: &str) -> String {
    if let Some(start) = text.find('(')
        && let Some(rel_end) = text[start + 1..].find(')') {
            return text[start + 1..start + 1 + rel_end].to_string();
        }
    String::new()
}

/// Load Hebrew OT data from TAHOT TSV file
pub fn load_hebrew_ot(
    path: &Path,
    verses: &mut HashMap<VerseRef, InterlinearVerse>,
    strongs_index: &mut StrongsIndex,
) -> Result<usize, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open {}: {}", path.display(), e))?;
    let reader = BufReader::new(file);
    let mut word_count = 0;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        // Skip header lines, comments, and empty lines
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with('=')
            || line.starts_with('\t')
            || line.starts_with("TAHOT")
            || line.starts_with("Ref")
            || line.starts_with("Word")
            || !line.contains('.')
        {
            continue;
        }

        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 6 {
            continue;
        }

        // Parse: Reference, Hebrew, Transliteration, English, dStrongs, Grammar
        let (book, chapter, verse, word_num) = match parse_reference(fields[0]) {
            Some(r) => r,
            None => continue,
        };

        let hebrew_text = clean_hebrew_text(fields[1]);
        let transliteration = fields[2].replace('.', "");
        let english_gloss = fields[3].to_string();
        let strongs = extract_strongs_number(fields[4]);
        let morphology = if fields.len() > 5 && !fields[5].is_empty() {
            Some(fields[5].to_string())
        } else {
            None
        };

        let verse_ref = VerseRef::new(&book, chapter, verse);

        // Add to Strong's index
        if let Some(ref s) = strongs {
            strongs_index.add_occurrence(s, verse_ref.clone());
        }

        let word = OriginalWord {
            position: word_num,
            original_text: hebrew_text,
            transliteration,
            english_gloss,
            strongs_number: strongs,
            morphology,
        };

        // Get or create the interlinear verse
        let interlinear = verses
            .entry(verse_ref.clone())
            .or_insert_with(|| InterlinearVerse {
                book: book.clone(),
                chapter,
                verse_number: verse,
                language: OriginalLanguage::Hebrew,
                original_words: Vec::new(),
            });

        interlinear.original_words.push(word);
        word_count += 1;
    }

    Ok(word_count)
}

/// Load Greek NT data from TAGNT TSV file
pub fn load_greek_nt(
    path: &Path,
    verses: &mut HashMap<VerseRef, InterlinearVerse>,
    strongs_index: &mut StrongsIndex,
) -> Result<usize, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open {}: {}", path.display(), e))?;
    let reader = BufReader::new(file);
    let mut word_count = 0;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        // Skip header lines, comments, and empty lines
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with('=')
            || line.starts_with('\t')
            || line.starts_with("TAGNT")
            || line.starts_with("Word")
            || line.starts_with('$')
            || line.starts_with('*')
            || !line.contains('.')
        {
            continue;
        }

        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 5 {
            continue;
        }

        // Parse: Reference, Greek(translit), English, dStrongs=Grammar, DictForm=Gloss
        let (book, chapter, verse, word_num) = match parse_reference(fields[0]) {
            Some(r) => r,
            None => continue,
        };

        let greek_text = clean_greek_text(fields[1]);
        let transliteration = extract_greek_transliteration(fields[1]);
        let english_gloss = fields[2].to_string();

        // Parse dStrongs=Grammar field (e.g., "G0976=N-NSF")
        let strongs_grammar = fields[3];
        let (strongs, morphology) = if strongs_grammar.contains('=') {
            let parts: Vec<&str> = strongs_grammar.split('=').collect();
            (
                extract_strongs_number(parts[0]),
                if parts.len() > 1 {
                    Some(parts[1].to_string())
                } else {
                    None
                },
            )
        } else {
            (extract_strongs_number(strongs_grammar), None)
        };

        let verse_ref = VerseRef::new(&book, chapter, verse);

        // Add to Strong's index
        if let Some(ref s) = strongs {
            strongs_index.add_occurrence(s, verse_ref.clone());
        }

        let word = OriginalWord {
            position: word_num,
            original_text: greek_text,
            transliteration,
            english_gloss,
            strongs_number: strongs,
            morphology,
        };

        // Get or create the interlinear verse
        let interlinear = verses
            .entry(verse_ref.clone())
            .or_insert_with(|| InterlinearVerse {
                book: book.clone(),
                chapter,
                verse_number: verse,
                language: OriginalLanguage::Greek,
                original_words: Vec::new(),
            });

        interlinear.original_words.push(word);
        word_count += 1;
    }

    Ok(word_count)
}

/// Convert STEP Bible lexicon markup into readable plain text.
///
/// Handles `<b>`, `<i>`, `<BR />`, `<ref='…'>…</ref>`, `<lb>`, and `__N.` section markers.
pub fn clean_lexicon_markup(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '<' {
            // Find end of tag
            if let Some(rel_end) = chars[i..].iter().position(|&c| c == '>') {
                let tag: String = chars[i + 1..i + rel_end].iter().collect();
                let tag_lower = tag.to_ascii_lowercase();
                let tag_name = tag_lower
                    .trim_start_matches('/')
                    .split([' ', '=', '\''])
                    .next()
                    .unwrap_or("");

                match tag_name {
                    "br" | "lb" => {
                        if !out.ends_with('\n') {
                            out.push('\n');
                        }
                    }
                    // Keep inner text for b/i/ref; skip the tags themselves
                    "b" | "i" | "ref" => {}
                    _ => {
                        // Unknown tag — drop it
                    }
                }
                i += rel_end + 1;
                continue;
            }
        }

        // Section markers like "__1." or "__(1)"
        if chars[i] == '_' && i + 1 < chars.len() && chars[i + 1] == '_' {
            if !out.ends_with('\n') && !out.is_empty() {
                out.push('\n');
            }
            i += 2;
            continue;
        }

        out.push(chars[i]);
        i += 1;
    }

    // Collapse runs of blank lines and trim
    let mut cleaned = String::new();
    let mut blank = false;
    for line in out.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !blank && !cleaned.is_empty() {
                cleaned.push('\n');
                blank = true;
            }
        } else {
            if !cleaned.is_empty() && !cleaned.ends_with('\n') {
                cleaned.push('\n');
            } else if cleaned.ends_with('\n') && blank {
                // already have one blank
            }
            cleaned.push_str(trimmed);
            blank = false;
        }
    }

    cleaned.trim().to_string()
}

/// Load lexicon data from TBESH or TBESG TSV file
pub fn load_lexicon(path: &Path) -> Result<HashMap<String, LexiconEntry>, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open {}: {}", path.display(), e))?;
    let reader = BufReader::new(file);
    let mut lexicon = HashMap::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        // Skip header lines and comments
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with('=')
            || line.starts_with('$')
            || line.starts_with('*')
            || line.starts_with("eStrong")
            || line.starts_with('-')
        {
            continue;
        }

        let fields: Vec<&str> = line.split('\t').collect();
        // Format: eStrong#, dStrong, uStrong, Hebrew/Greek, Transliteration, Morph, Gloss, Meaning
        if fields.len() < 8 {
            continue;
        }

        let strongs_raw = fields[0];
        // Extract just the number part (e.g., "H0001" from various formats)
        let strongs_number = if let Some(pos) = strongs_raw.find(['H', 'G']) {
            let mut num = String::new();
            for c in strongs_raw[pos..].chars() {
                if c == 'H' || c == 'G' || c.is_ascii_digit() {
                    num.push(c);
                } else {
                    break;
                }
            }
            num
        } else {
            continue;
        };

        if strongs_number.len() < 2 {
            continue;
        }

        let entry = LexiconEntry {
            strongs_number: strongs_number.clone(),
            original_word: fields[3].to_string(),
            transliteration: fields[4].to_string(),
            morph: fields[5].to_string(),
            gloss: fields[6].to_string(),
            definition: clean_lexicon_markup(fields[7]),
        };

        // Only insert if not already present (first entry wins)
        lexicon.entry(strongs_number).or_insert(entry);
    }

    Ok(lexicon)
}

/// Load all original language data from the data directory
pub fn load_extended_bible(data_dir: &Path) -> Result<ExtendedBible, String> {
    let mut extended = ExtendedBible::new();

    // Load Hebrew OT files
    let hebrew_files = [
        "TAHOT_Gen-Deu.txt",
        "TAHOT_Jos-Est.txt",
        "TAHOT_Job-Sng.txt",
        "TAHOT_Isa-Mal.txt",
    ];

    for file_name in &hebrew_files {
        let path = data_dir.join(file_name);
        if path.exists() {
            match load_hebrew_ot(
                &path,
                &mut extended.interlinear_ot,
                &mut extended.strongs_index,
            ) {
                Ok(count) => eprintln!("Loaded {} Hebrew words from {}", count, file_name),
                Err(e) => eprintln!("Warning: Failed to load {}: {}", file_name, e),
            }
        }
    }

    // Load Greek NT files
    let greek_files = ["TAGNT_Mat-Jhn.txt", "TAGNT_Act-Rev.txt"];

    for file_name in &greek_files {
        let path = data_dir.join(file_name);
        if path.exists() {
            match load_greek_nt(
                &path,
                &mut extended.interlinear_nt,
                &mut extended.strongs_index,
            ) {
                Ok(count) => eprintln!("Loaded {} Greek words from {}", count, file_name),
                Err(e) => eprintln!("Warning: Failed to load {}: {}", file_name, e),
            }
        }
    }

    // Load Hebrew lexicon
    let hebrew_lexicon_path = data_dir.join("TBESH.txt");
    if hebrew_lexicon_path.exists() {
        match load_lexicon(&hebrew_lexicon_path) {
            Ok(lex) => {
                eprintln!("Loaded {} Hebrew lexicon entries", lex.len());
                extended.hebrew_lexicon = lex;
            }
            Err(e) => eprintln!("Warning: Failed to load Hebrew lexicon: {}", e),
        }
    }

    // Load Greek lexicon
    let greek_lexicon_path = data_dir.join("TBESG.txt");
    if greek_lexicon_path.exists() {
        match load_lexicon(&greek_lexicon_path) {
            Ok(lex) => {
                eprintln!("Loaded {} Greek lexicon entries", lex.len());
                extended.greek_lexicon = lex;
            }
            Err(e) => eprintln!("Warning: Failed to load Greek lexicon: {}", e),
        }
    }

    // Ensure word order is stable for rendering
    for verse in extended
        .interlinear_ot
        .values_mut()
        .chain(extended.interlinear_nt.values_mut())
    {
        verse.original_words.sort_by_key(|w| w.position);
    }

    Ok(extended)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_reference_hebrew() {
        let (book, chapter, verse, word) = parse_reference("Gen.1.1#01=L").unwrap();
        assert_eq!(book, "Genesis");
        assert_eq!(chapter, 1);
        assert_eq!(verse, 1);
        assert_eq!(word, 1);

        // Test numbered books map to "First/Second" names
        let (book, _, _, _) = parse_reference("1Sa.1.1#01=L").unwrap();
        assert_eq!(book, "First Samuel");
    }

    #[test]
    fn test_parse_reference_greek() {
        let (book, chapter, verse, word) = parse_reference("Mat.1.1#01=NKO").unwrap();
        assert_eq!(book, "Matthew");
        assert_eq!(chapter, 1);
        assert_eq!(verse, 1);
        assert_eq!(word, 1);

        // Test numbered books map to "First/Second" names
        let (book, _, _, _) = parse_reference("1Co.1.1#01=NKO").unwrap();
        assert_eq!(book, "First Corinthians");
    }

    #[test]
    fn test_extract_strongs_hebrew() {
        assert_eq!(
            extract_strongs_number("H9003/{H7225G}"),
            Some("H7225".to_string())
        );
        assert_eq!(
            extract_strongs_number("{H1254A}"),
            Some("H1254".to_string())
        );
        assert_eq!(
            extract_strongs_number("{H0430G}"),
            Some("H0430".to_string())
        );
    }

    #[test]
    fn test_extract_strongs_greek() {
        assert_eq!(
            extract_strongs_number("G0976=N-NSF"),
            Some("G0976".to_string())
        );
        assert_eq!(
            extract_strongs_number("G2424G=N-GSM-P"),
            Some("G2424".to_string())
        );
    }

    #[test]
    fn test_clean_hebrew_text() {
        assert_eq!(clean_hebrew_text("בְּ/רֵאשִׁ֖ית"), "בְּרֵאשִׁ֖ית");
        assert_eq!(clean_hebrew_text("הַ/שָּׁמַ֖יִם"), "הַשָּׁמַ֖יִם");
    }

    #[test]
    fn test_extract_greek_transliteration() {
        assert_eq!(extract_greek_transliteration("Βίβλος (Biblos)"), "Biblos");
        assert_eq!(
            extract_greek_transliteration("γενέσεως (geneseōs)"),
            "geneseōs"
        );
        // Closing paren before opening must not panic or slice incorrectly
        assert_eq!(extract_greek_transliteration("foo) bar (baz"), "");
        assert_eq!(extract_greek_transliteration("foo) bar (baz)"), "baz");
    }

    #[test]
    fn test_clean_lexicon_markup_strips_tags() {
        let raw = " <b>σύ</b>, <BR /> <i>pron.</i> of 2nd of person(s), <BR /><b>thou, you</b>, \
            <ref='Mat.25.39'>Mat.25:39</ref> __1. Emphatic";
        let cleaned = clean_lexicon_markup(raw);
        assert!(!cleaned.contains('<'), "left over tags: {}", cleaned);
        assert!(!cleaned.contains('>'), "left over tags: {}", cleaned);
        assert!(cleaned.contains("σύ"));
        assert!(cleaned.contains("pron."));
        assert!(cleaned.contains("thou, you"));
        assert!(cleaned.contains("Mat.25:39"));
        assert!(cleaned.contains("1. Emphatic"));
    }

    #[test]
    fn test_clean_lexicon_markup_newlines_from_br() {
        let cleaned = clean_lexicon_markup("a<BR />b<br>c");
        assert_eq!(cleaned, "a\nb\nc");
    }
}
