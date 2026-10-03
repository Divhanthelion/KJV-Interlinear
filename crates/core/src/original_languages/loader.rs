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

/// A parsed STEP word reference.
#[derive(Debug, PartialEq)]
struct WordRef {
    /// Book, chapter, verse in KJV versification
    book: &'static str,
    chapter: u32,
    verse: u32,
    /// Source/edition code after '=' (e.g. "L", "Q(K)", "NKO", "N(k)O")
    word_type: String,
}

/// Parse a STEP reference into the KJV verse the word belongs to.
///
/// Forms: `Gen.1.1#01=L`, `Gen.31.55(32.1)#01=L` (Hebrew ref in brackets),
/// `Mat.17.15[17.14]#01=NKO` (KJV ref in square brackets),
/// `Mrk.12.15(12.14)#03=NKO` / `Rom.16.25{14.24}#01=NKO` (NA / other editions).
/// The leading ref is English (NRSV) versification; a `[..]` ref overrides it for the KJV.
fn parse_reference(reference: &str) -> Option<WordRef> {
    let (verse_part, word_part) = reference.split_once('#')?;
    let word_type = word_part.split_once('=').map(|(_, t)| t).unwrap_or("").to_string();

    let main_end = verse_part.find(['(', '[', '{']).unwrap_or(verse_part.len());
    let mut parts = verse_part[..main_end].split('.');
    let book = *BOOK_NAME_MAPPING.get(parts.next()?)?;
    let mut chapter: u32 = parts.next()?.parse().ok()?;
    let mut verse: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }

    if let Some(start) = verse_part.find('[') {
        let end = verse_part[start..].find(']')? + start;
        let (c, v) = verse_part[start + 1..end].split_once('.')?;
        chapter = c.parse().ok()?;
        verse = v.parse().ok()?;
    }

    Some(WordRef {
        book,
        chapter,
        verse,
        word_type,
    })
}

/// Extract the primary Strong's number from a dStrongs field
/// Examples: "H9003/{H7225G}" -> "H7225", "{H1254A}" -> "H1254", "G0976=N-NSF" -> "G0976"
fn extract_strongs_number(dstrongs: &str) -> Option<String> {
    // Hebrew marks the root with {braces}; prefixes/suffixes sit outside them
    if let Some(start) = dstrongs.find('{') {
        let root: String = dstrongs[start + 1..]
            .chars()
            .take_while(|c| *c != '}')
            .collect();
        if let Some(s) = leading_strongs(&root) {
            return Some(s);
        }
    }

    let cleaned = dstrongs.replace(['{', '}'], "").replace('/', " ");
    for part in cleaned.split_whitespace() {
        // Skip prefix markers like H9003 (preposition markers)
        if part.starts_with("H900") || part.starts_with("H901") {
            continue;
        }
        if let Some(pos) = part.find(['H', 'G'])
            && let Some(s) = leading_strongs(&part[pos..])
        {
            return Some(s);
        }
    }
    None
}

/// "H7225G" -> "H7225"; None unless a letter is followed by digits.
fn leading_strongs(s: &str) -> Option<String> {
    let mut chars = s.chars();
    let letter = chars.next().filter(|c| *c == 'H' || *c == 'G')?;
    let digits: String = chars.take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        Some(format!("{}{}", letter, digits))
    }
}

/// Clean Hebrew text (remove forward slashes used for prefix/suffix markers)
fn clean_hebrew_text(text: &str) -> String {
    text.replace(['/', '\\'], "")
}

/// Hebrew transliteration without STEP's syllable dots and morpheme slashes ("be./re.Shit" -> "bereShit")
fn clean_hebrew_transliteration(text: &str) -> String {
    text.replace(['.', '/', '\\'], "")
}

/// Clean Greek text: drop the parenthetical transliteration and editorial marks
/// (NA paragraph ¶, ¬, and the [[ ]] around passages NA considers doubtful).
fn clean_greek_text(text: &str) -> String {
    let word = match text.find('(') {
        Some(paren_pos) => &text[..paren_pos],
        None => text,
    };
    word.replace(['¶', '¬', '[', ']'], "").trim().to_string()
}

/// Extract transliteration from Greek field like "Βίβλος (Biblos)"
fn extract_greek_transliteration(text: &str) -> String {
    if let Some(start) = text.find('(')
        && let Some(rel_end) = text[start + 1..].find(')') {
            return text[start + 1..start + 1 + rel_end].to_string();
        }
    String::new()
}

/// Drop a versification note at the start of a gloss ("[13.1] And" -> "And").
fn strip_verse_marker(gloss: &str) -> &str {
    let gloss = gloss.trim();
    let Some(open) = gloss.chars().next().filter(|c| matches!(c, '[' | '(' | '{')) else {
        return gloss;
    };
    let close = match open {
        '[' => ']',
        '(' => ')',
        _ => '}',
    };
    match gloss.find(close) {
        Some(end)
            if end > 1
                && gloss[1..end].chars().all(|c| c.is_ascii_digit() || c == '.')
                && gloss[1..end].contains('.') =>
        {
            gloss[end + 1..].trim_start()
        }
        _ => gloss,
    }
}

/// How an editions list like "NA28+NA27+TR»1+Byz" includes the Textus Receptus.
#[derive(Debug, PartialEq)]
enum TrEdition {
    /// "TR": same word, same place
    InPlace,
    /// "TR»1" / "TR«2": the TR has it at a different position (or fused with a neighbour)
    Displaced,
    Missing,
}

fn tr_edition(editions: &str) -> TrEdition {
    for e in editions.split('+').map(str::trim) {
        if e == "TR" {
            return TrEdition::InPlace;
        }
        if e.starts_with("TR«") || e.starts_with("TR»") {
            return TrEdition::Displaced;
        }
    }
    TrEdition::Missing
}

/// The Textus Receptus reading from a TAGNT variants field, e.g.
/// "βληθῇ (T=blēthēa) may be cast - G0906=V-APS-3S in: TR«3+Byz«3 ¦ …".
fn tr_variant(variants: &str) -> Option<OriginalWord> {
    for variant in variants.split('¦') {
        let Some((body, editions)) = variant.trim().rsplit_once(" in: ") else {
            continue;
        };
        if tr_edition(editions) == TrEdition::Missing {
            continue;
        }
        let Some((words, tags)) = body.rsplit_once(" - ") else {
            continue;
        };
        let Some((greek, rest)) = words.split_once(" (") else {
            continue;
        };
        let Some((translit, gloss)) = rest.split_once(')') else {
            continue;
        };
        let translit = translit.split_once('=').map_or(translit, |(_, t)| t);
        let first_tag = tags.split(" + ").next().unwrap_or("");
        let (strongs, morph) = match first_tag.split_once('=') {
            Some((s, m)) => (extract_strongs_number(s), Some(m.trim().to_string())),
            None => (extract_strongs_number(first_tag), None),
        };
        return Some(OriginalWord {
            position: 0,
            original_text: clean_greek_text(greek),
            transliteration: translit.trim().to_string(),
            english_gloss: gloss.trim().to_string(),
            strongs_number: strongs,
            morphology: morph,
        });
    }
    None
}

/// Where a TAGNT word stands relative to the KJV's Greek text (Scrivener's TR).
#[derive(Debug, PartialEq)]
enum TrStatus {
    /// Word is in the TR as given
    Present,
    /// TR reads differently here: use the TR reading from the variants column, or
    /// drop the word if it has none (it is fused into a neighbouring TR word,
    /// e.g. NA "διὰ παντός" = TR "διαπαντός").
    Variant,
    /// Word is not in the TR at all
    Absent,
}

/// Classify by word type ("NKO", "N(k)O", "no", …) and editions list.
/// Upper/lower-case K outside brackets = in TR; inside brackets = TR differs.
fn tr_status(word_type: &str, editions: &str) -> TrStatus {
    let mut depth = 0;
    let (mut main_k, mut bracket_k) = (false, false);
    for c in word_type.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            'K' | 'k' if depth == 0 => main_k = true,
            'K' | 'k' => bracket_k = true,
            _ => {}
        }
    }
    let edition = tr_edition(editions);
    if main_k || edition == TrEdition::InPlace {
        TrStatus::Present
    } else if bracket_k {
        TrStatus::Variant
    } else if edition == TrEdition::Displaced {
        // Same word, different word order in the TR
        TrStatus::Present
    } else {
        TrStatus::Absent
    }
}

/// Append a word to its verse, numbering positions in reading order.
fn push_word(
    verses: &mut HashMap<VerseRef, InterlinearVerse>,
    strongs_index: &mut StrongsIndex,
    word_ref: &WordRef,
    language: OriginalLanguage,
    mut word: OriginalWord,
) {
    let verse_ref = VerseRef::new(word_ref.book, word_ref.chapter, word_ref.verse);
    if let Some(ref s) = word.strongs_number {
        strongs_index.add_occurrence(s, verse_ref.clone());
    }
    let interlinear = verses.entry(verse_ref).or_insert_with(|| InterlinearVerse {
        book: word_ref.book.to_string(),
        chapter: word_ref.chapter,
        verse_number: word_ref.verse,
        language,
        original_words: Vec::new(),
    });
    word.position = interlinear.original_words.len() as u32 + 1;
    interlinear.original_words.push(word);
}

/// True for word rows ("Gen.1.1#01=L…"), false for headers and summary lines.
fn is_word_row(line: &str) -> bool {
    let Some(first) = line.split('\t').next() else {
        return false;
    };
    first.contains('#') && first.contains('.') && !line.starts_with('#')
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
        let line = line.map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        if !is_word_row(&line) {
            continue;
        }

        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 6 {
            continue;
        }

        // Fields: Ref, Hebrew, Transliteration, English, dStrongs, Grammar
        let Some(word_ref) = parse_reference(fields[0]) else {
            continue;
        };

        // "X" words are reconstructed from the LXX and are not in the Hebrew the KJV translated.
        // Rows with no Hebrew mark a Ketiv word that the Qere (followed by the KJV) omits.
        if word_ref.word_type.starts_with('X') || fields[1].trim().is_empty() {
            continue;
        }

        let word = OriginalWord {
            position: 0,
            original_text: clean_hebrew_text(fields[1]),
            transliteration: clean_hebrew_transliteration(fields[2]),
            english_gloss: fields[3].trim().to_string(),
            strongs_number: extract_strongs_number(fields[4]),
            morphology: Some(fields[5].trim())
                .filter(|m| !m.is_empty())
                .map(str::to_string),
        };
        push_word(verses, strongs_index, &word_ref, OriginalLanguage::Hebrew, word);
        word_count += 1;
    }

    Ok(word_count)
}

/// Load Greek NT data from TAGNT TSV file, keeping the text behind the KJV (TR).
pub fn load_greek_nt(
    path: &Path,
    verses: &mut HashMap<VerseRef, InterlinearVerse>,
    strongs_index: &mut StrongsIndex,
) -> Result<usize, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open {}: {}", path.display(), e))?;
    let reader = BufReader::new(file);
    let mut word_count = 0;

    for line in reader.lines() {
        let line = line.map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        if !is_word_row(&line) {
            continue;
        }

        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 5 {
            continue;
        }

        // Fields: Ref, Greek(translit), English, dStrongs=Grammar, DictForm=Gloss, editions, variants
        let Some(word_ref) = parse_reference(fields[0]) else {
            continue;
        };

        let editions = fields.get(5).copied().unwrap_or("");
        let status = tr_status(&word_ref.word_type, editions);
        if status == TrStatus::Absent {
            continue;
        }

        let (strongs, morphology) = match fields[3].split_once('=') {
            Some((s, m)) => (extract_strongs_number(s), Some(m.trim().to_string())),
            None => (extract_strongs_number(fields[3]), None),
        };
        let mut word = OriginalWord {
            position: 0,
            original_text: clean_greek_text(fields[1]),
            transliteration: extract_greek_transliteration(fields[1]),
            english_gloss: strip_verse_marker(fields[2]).to_string(),
            strongs_number: strongs,
            morphology,
        };

        if status == TrStatus::Variant {
            match tr_variant(fields.get(6).copied().unwrap_or("")) {
                Some(tr_word) => word = tr_word,
                None => continue,
            }
        }

        push_word(verses, strongs_index, &word_ref, OriginalLanguage::Greek, word);
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

/// The Old Testament's Aramaic verses (most of Daniel 2:4-7:28 and Ezra 4:8-6:18 and
/// 7:12-26, Jeremiah 10:11, and two words of Genesis 31:47): those whose words are
/// mostly Aramaic. TAHOT's morphology codes begin "H" for a Hebrew word and "A" for an
/// Aramaic one; a verse where the language changes (Daniel 2:4) takes the language of
/// most of its words.
fn mark_aramaic(verses: &mut HashMap<VerseRef, InterlinearVerse>) {
    for v in verses.values_mut() {
        let lang = |c: char| v.original_words.iter().filter(|w| w.morphology.as_deref().is_some_and(|m| m.starts_with(c))).count();
        if lang('A') > lang('H') {
            v.language = OriginalLanguage::Aramaic;
        }
    }
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

    mark_aramaic(&mut extended.interlinear_ot);

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

    // Concordance in canonical verse order (words arrive in file order, which differs
    // from KJV order where verse boundaries differ, e.g. Philippians 1:16-17)
    extended.rebuild_strongs_index();

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
        let r = parse_reference("Gen.1.1#01=L").unwrap();
        assert_eq!((r.book, r.chapter, r.verse), ("Genesis", 1, 1));
        assert_eq!(r.word_type, "L");

        // Hebrew versification in brackets: keep the English (KJV) ref
        let r = parse_reference("Gen.31.55(32.1)#01=L").unwrap();
        assert_eq!((r.book, r.chapter, r.verse), ("Genesis", 31, 55));

        // Psalm superscription is verse 0
        let r = parse_reference("Psa.3.0(3.1)#01=L").unwrap();
        assert_eq!((r.book, r.chapter, r.verse), ("Psalms", 3, 0));

        let r = parse_reference("1Sa.1.1#01=Q(K)").unwrap();
        assert_eq!(r.book, "First Samuel");
        assert_eq!(r.word_type, "Q(K)");
    }

    #[test]
    fn test_parse_reference_greek() {
        let r = parse_reference("Mat.1.1#01=NKO").unwrap();
        assert_eq!((r.book, r.chapter, r.verse), ("Matthew", 1, 1));

        // Square brackets carry the KJV reference
        let r = parse_reference("Rev.12.18[13.1]#01=NKO").unwrap();
        assert_eq!((r.book, r.chapter, r.verse), ("Revelation", 13, 1));
        let r = parse_reference("3Jn.1.15[1.14]#02=NKO").unwrap();
        assert_eq!((r.book, r.chapter, r.verse), ("Third John", 1, 14));

        // Round (NA) and curly (other editions) brackets are not KJV refs
        let r = parse_reference("Mrk.12.15(12.14)#03=NKO").unwrap();
        assert_eq!((r.chapter, r.verse), (12, 15));
        let r = parse_reference("Rom.16.25{14.24}#01=NKO").unwrap();
        assert_eq!((r.chapter, r.verse), (16, 25));

        assert!(parse_reference("Xyz.1.1#01=L").is_none());
        assert!(parse_reference("# Mat.1.1").is_none());
    }

    #[test]
    fn test_tr_status() {
        assert_eq!(tr_status("NKO", ""), TrStatus::Present);
        assert_eq!(tr_status("k", ""), TrStatus::Present);
        assert_eq!(tr_status("NK(o)", ""), TrStatus::Present);
        assert_eq!(tr_status("N(K)O", "NA28+NA27"), TrStatus::Variant);
        assert_eq!(tr_status("N(k)(o)", "NA28"), TrStatus::Variant);
        // Displaced TR on a KJV-variant word: fused into a neighbour, handled as a variant
        assert_eq!(tr_status("N(K)O", "NA28+TR»1+Byz"), TrStatus::Variant);
        assert_eq!(tr_status("N(k)O", "NA28+TR+Byz"), TrStatus::Present);
        assert_eq!(tr_status("NO", "NA28+TR»1"), TrStatus::Present);
        assert_eq!(tr_status("no", "NA28+NA27"), TrStatus::Absent);
        assert_eq!(tr_status("NO", "NA28"), TrStatus::Absent);
        assert_eq!(tr_status("o", ""), TrStatus::Absent);
    }

    #[test]
    fn test_tr_variant() {
        let v = "βληθῇ (T=blēthēa) may be cast - G0906=V-APS-3S in: TR«3+Byz«3";
        let w = tr_variant(v).unwrap();
        assert_eq!(w.original_text, "βληθῇ");
        assert_eq!(w.transliteration, "blēthēa");
        assert_eq!(w.english_gloss, "may be cast");
        assert_eq!(w.strongs_number.as_deref(), Some("G0906"));
        assert_eq!(w.morphology.as_deref(), Some("V-APS-3S"));

        // Picks the TR alternative when several are listed
        let v = "κατέλιπόν (t=katelipon) I left behind - G2641=V-2AAI-1S in: TR+Byz ¦ ἀπέλειπόν (o=apeleipon) I was leaving - G0620=V-IAI-1S in: Tyn+WH";
        assert_eq!(tr_variant(v).unwrap().original_text, "κατέλιπόν");
        let v = "ἀπέλειπόν (o=apeleipon) I was leaving - G0620=V-IAI-1S in: Tyn+WH";
        assert!(tr_variant(v).is_none());
    }

    #[test]
    fn test_strip_verse_marker() {
        assert_eq!(strip_verse_marker("[13.1] And"), "And");
        assert_eq!(strip_verse_marker("{14.24} To Him"), "To Him");
        assert_eq!(strip_verse_marker("[the] book"), "[the] book");
        assert_eq!(strip_verse_marker("(obj.)"), "(obj.)");
        assert_eq!(strip_verse_marker("do cast [it]"), "do cast [it]");
    }

    #[test]
    fn test_clean_greek_text() {
        assert_eq!(clean_greek_text("αὐτῶν.¶ (autōn)"), "αὐτῶν.");
        assert_eq!(clean_greek_text("[[Ἀναστὰς (Anastas)"), "Ἀναστὰς");
        assert_eq!(clean_greek_text("ἀμήν.¶]] (amēn)"), "ἀμήν.");
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
        // Root is a particle: take the braced tag, not the prefix or suffix
        assert_eq!(
            extract_strongs_number("H9002/{H9005}/H9033"),
            Some("H9005".to_string())
        );
        assert_eq!(
            extract_strongs_number(r"H9004/{H9005}\H9014"),
            Some("H9005".to_string())
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
