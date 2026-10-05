//! Every jot and tittle of the KJV text, checked character by character:
//!
//! 1. The text files match the eBible.org 1769 source they were made from
//!    (`KJV_SOURCE=path/to/eng-kjv_vpl.txt`; CI downloads it, see ci.yml).
//! 2. The text files match the reviewed fingerprint in `kjv-text.lock`: every
//!    chapter's SHA-256 and the count of every character, so no change slips in
//!    unnoticed; the same goes for the SHA-256 of each STEP and red-letter file
//!    in data/. After a deliberate, reviewed change, rewrite it locally (never
//!    in CI) with `KJV_UPDATE_LOCK=1 cargo test -p kjv-core --test text_fidelity`.
//! 3. Typography holds everywhere: spacing, punctuation, capitals, parentheses.
//! 4. Every verse reaches the app unchanged: the files as read here (independently
//!    of the app's parser) equal the embedded bundle, the chapter view's segments
//!    (with red letter and search marks), search results, and copied text.
//!
//! The screen itself is checked by tests/ui/smoke.mjs, which reads every chapter
//! as drawn and compares it with these files.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use kjv_core::api::{self, ChapterOptions, Scope};
use kjv_core::bundle::DataBundle;
use kjv_core::original_languages::loader::{GREEK_FILES, HEBREW_FILES, LEXICON_FILES};
use sha2::{Digest, Sha256};

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn lock_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/kjv-text.lock")
}

/// eBible/USFM book codes in canonical order, as used by eng-kjv_vpl.txt.
const CODES: [&str; 66] = [
    "GEN", "EXO", "LEV", "NUM", "DEU", "JOS", "JDG", "RUT", "1SA", "2SA", "1KI", "2KI", "1CH", "2CH", "EZR", "NEH",
    "EST", "JOB", "PSA", "PRO", "ECC", "SOL", "ISA", "JER", "LAM", "EZE", "DAN", "HOS", "JOE", "AMO", "OBA", "JON",
    "MIC", "NAH", "HAB", "ZEP", "HAG", "ZEC", "MAL", "MAT", "MAR", "LUK", "JOH", "ACT", "ROM", "1CO", "2CO", "GAL",
    "EPH", "PHI", "COL", "1TH", "2TH", "1TI", "2TI", "TIT", "PHM", "HEB", "JAM", "1PE", "2PE", "1JO", "2JO", "3JO",
    "JUD", "REV",
];

/// The KJV's own verses that run on into the next without punctuation.
const RUN_ON_VERSES: [&str; 7] = [
    "Genesis 23:17",
    "First Chronicles 21:11",
    "Second Chronicles 30:18",
    "Psalms 96:12",
    "Psalms 98:8",
    "Romans 11:7",
    "Colossians 1:21",
];

// ---------------------------------------------------------------- the files, read plainly

struct Line {
    chapter: u32,
    /// 0 for a Psalm title
    verse: u32,
    text: String,
}

struct Book {
    name: String,
    /// The file exactly as stored
    raw: String,
    lines: Vec<Line>,
}

impl Book {
    fn at(&self, l: &Line) -> String {
        format!("{} {}:{}", self.name, l.chapter, l.verse)
    }
}

/// The 66 files, read with nothing but `split`, in the app's book order.
fn books() -> &'static [Book] {
    static BOOKS: OnceLock<Vec<Book>> = OnceLock::new();
    BOOKS.get_or_init(|| {
        let bible = &bundle().bible;
        assert_eq!(bible.books.len(), 66);
        bible
            .books
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let dir = if i < 39 { "old_testament" } else { "new_testament" };
                let path = root().join(dir).join(format!("{}.txt", b.name));
                // Git may check text out with CRLF line endings on Windows; the repository has LF
                let raw = std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
                    .replace("\r\n", "\n");
                assert!(!raw.starts_with('\u{feff}'), "{} starts with a byte-order mark", b.name);
                assert!(!raw.contains('\r'), "{} has a stray carriage return", b.name);
                assert!(raw.ends_with('\n') && !raw.ends_with("\n\n"), "{} must end with exactly one newline", b.name);
                let lines = raw
                    .lines()
                    .enumerate()
                    .map(|(n, line)| {
                        let bad = format!("{} line {} is not \"chapter:verse text\": {:?}", b.name, n + 1, line);
                        let (reference, text) = line.split_once(' ').expect(&bad);
                        let (c, v) = reference.split_once(':').expect(&bad);
                        Line { chapter: c.parse().expect(&bad), verse: v.parse().expect(&bad), text: text.to_string() }
                    })
                    .collect();
                Book { name: b.name.clone(), raw, lines }
            })
            .collect()
    })
}

/// The bundle exactly as the app embeds it: built, serialized, and read back.
fn bundle() -> &'static DataBundle {
    static BUNDLE: OnceLock<DataBundle> = OnceLock::new();
    BUNDLE.get_or_init(|| {
        let built = DataBundle::from_sources(root()).expect("bundle builds");
        DataBundle::from_bytes(&built.to_bytes().unwrap()).expect("bundle reads back")
    })
}

// ---------------------------------------------------------------- 1. the eBible source

/// The source verse as the text files store it: italics brackets and paragraph
/// marks removed, spaces collapsed (see NOTICE, section 1).
fn clean_source(text: &str) -> String {
    text.replace(['[', ']'], "").replace('\u{b6}', " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// "col 34: source ',' (U+002C), ours ';' (U+003B)" plus the text around it.
fn describe_difference(ours: &str, source: &str) -> String {
    let a: Vec<char> = ours.chars().collect();
    let b: Vec<char> = source.chars().collect();
    let i = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let show = |v: &[char]| match v.get(i) {
        Some(c) => format!("{:?} (U+{:04X})", c, *c as u32),
        None => "end of verse".to_string(),
    };
    let around = |v: &[char]| v[i.saturating_sub(30)..(i + 30).min(v.len())].iter().collect::<String>();
    format!(
        "character {}: source has {}, ours has {}\n      source: …{}…\n      ours:   …{}…",
        i + 1,
        show(&b),
        show(&a),
        around(&b),
        around(&a)
    )
}

#[test]
fn every_character_matches_the_ebible_source() {
    let Some(path) = std::env::var_os("KJV_SOURCE") else {
        // CI always supplies it; locally it's optional
        assert!(std::env::var_os("CI").is_none(), "CI must set KJV_SOURCE to eng-kjv_vpl.txt");
        eprintln!("skipped: set KJV_SOURCE to eBible's eng-kjv_vpl.txt to compare every character with the source");
        return;
    };
    let source = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{:?}: {}", path, e));
    let source = source.strip_prefix('\u{feff}').unwrap_or(&source);

    // (book index, chapter, verse) -> cleaned text, canonical books only
    let mut want: BTreeMap<(usize, u32, u32), String> = BTreeMap::new();
    for line in source.lines() {
        let mut parts = line.splitn(3, ' ');
        let (Some(code), Some(reference)) = (parts.next(), parts.next()) else { continue };
        let Some(book) = CODES.iter().position(|c| *c == code) else { continue }; // Apocrypha
        let (c, v) = reference.split_once(':').unwrap_or_else(|| panic!("source line {:?}", line));
        let key = (book, c.parse().unwrap(), v.parse().unwrap());
        assert!(want.insert(key, clean_source(parts.next().unwrap_or(""))).is_none(), "source repeats {}", line);
    }

    let mut have: BTreeMap<(usize, u32, u32), String> = BTreeMap::new();
    let mut names = Vec::new();
    for (i, book) in books().iter().enumerate() {
        names.push(book.name.as_str());
        for l in &book.lines {
            have.insert((i, l.chapter, l.verse), l.text.clone());
        }
    }
    // A Psalm title is the opening of verse 1 in the source
    let titles: Vec<_> = have.iter().filter(|(k, _)| k.2 == 0).map(|(k, t)| (*k, t.clone())).collect();
    for ((b, c, _), title) in &titles {
        have.remove(&(*b, *c, 0));
        let v1 = have.get_mut(&(*b, *c, 1)).unwrap();
        *v1 = format!("{} {}", title, v1);
    }

    let name = |k: &(usize, u32, u32)| format!("{} {}:{}", names[k.0], k.1, k.2);
    let mut problems = Vec::new();
    for k in want.keys().filter(|k| !have.contains_key(k)) {
        problems.push(format!("{} is in the source but not in the files", name(k)));
    }
    for k in have.keys().filter(|k| !want.contains_key(k)) {
        problems.push(format!("{} is in the files but not in the source", name(k)));
    }
    for (k, ours) in &have {
        if let Some(source) = want.get(k)
            && ours != source
        {
            problems.push(format!("{}: {}", name(k), describe_difference(ours, source)));
        }
    }
    assert_eq!(want.len(), 31_102, "the source should have 31,102 canonical verses");
    assert!(
        problems.is_empty(),
        "{} verses differ from the eBible source:\n{}",
        problems.len(),
        problems.iter().take(50).map(|p| format!("  {}", p)).collect::<Vec<_>>().join("\n")
    );
}

// ---------------------------------------------------------------- 2. the reviewed fingerprint

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// The data files the app is built from besides the KJV text.
fn data_files() -> Vec<String> {
    std::iter::once("words_of_jesus.json")
        .chain(HEBREW_FILES)
        .chain(GREEK_FILES)
        .chain(LEXICON_FILES)
        .map(|f| format!("data/{}", f))
        .collect()
}

/// SHA-256 of a file as the repository stores it: Git may check text out with
/// CRLF line endings on Windows, so CR before LF is left out.
fn file_sha256(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e));
    let mut h = Sha256::new();
    for (i, line) in bytes.split(|b| *b == b'\n').enumerate() {
        if i > 0 {
            h.update(b"\n");
        }
        h.update(line.strip_suffix(b"\r").unwrap_or(line));
    }
    hex(&h.finalize())
}

/// The lock file's contents for the text and data as they are now.
fn fingerprint() -> String {
    let mut whole = Sha256::new();
    let mut chars: BTreeMap<char, usize> = BTreeMap::new();
    let (mut verses, mut titles, mut total_chars) = (0, 0, 0);
    let mut chapters = String::new();
    for book in books() {
        whole.update(book.raw.as_bytes());
        let mut by_chapter: BTreeMap<u32, Vec<&Line>> = BTreeMap::new();
        for l in &book.lines {
            by_chapter.entry(l.chapter).or_default().push(l);
            for c in l.text.chars() {
                *chars.entry(c).or_default() += 1;
                total_chars += 1;
            }
            if l.verse == 0 { titles += 1 } else { verses += 1 }
        }
        for (c, lines) in by_chapter {
            let mut h = Sha256::new();
            let mut n = 0;
            for l in &lines {
                h.update(format!("{}:{} {}\n", l.chapter, l.verse, l.text).as_bytes());
                n += l.text.chars().count();
            }
            let count = lines.iter().filter(|l| l.verse > 0).count();
            let title = if lines.iter().any(|l| l.verse == 0) { " +title" } else { "" };
            writeln!(
                chapters,
                "{} {}\t{} verses{}\t{} chars\t{}",
                book.name,
                c,
                count,
                title,
                n,
                &hex(&h.finalize())[..16]
            )
            .unwrap();
        }
    }
    let mut out = String::new();
    out.push_str("# KJV text and data fingerprint, checked by crates/core/tests/text_fidelity.rs.\n");
    out.push_str("# Any change to any character of old_testament/, new_testament/, or the data/ files\n");
    out.push_str("# fails the tests until this file is rewritten (KJV_UPDATE_LOCK=1) and the change reviewed.\n\n");
    writeln!(out, "all\t{} verses\t{} titles\t{} chars\t{}\n", verses, titles, total_chars, hex(&whole.finalize()))
        .unwrap();
    for file in data_files() {
        writeln!(out, "file\t{}\t{}", file, file_sha256(&root().join(&file))).unwrap();
    }
    out.push('\n');
    for (c, n) in &chars {
        let shown = if *c == ' ' { "space".to_string() } else { c.to_string() };
        writeln!(out, "char\tU+{:04X}\t{}\t{}", *c as u32, shown, n).unwrap();
    }
    out.push('\n');
    out.push_str(&chapters);
    out
}

/// The lock records a reviewed text, so it is rewritten locally and the change
/// committed for review; CI only ever checks it.
fn refuse_lock_update_in_ci(ci: bool) {
    assert!(!ci, "KJV_UPDATE_LOCK is refused when CI is set: rewrite kjv-text.lock locally and commit it for review");
}

#[test]
#[should_panic(expected = "KJV_UPDATE_LOCK is refused when CI is set")]
fn lock_is_never_rewritten_in_ci() {
    refuse_lock_update_in_ci(true);
}

#[test]
fn text_matches_the_reviewed_fingerprint() {
    let now = fingerprint();
    if std::env::var_os("KJV_UPDATE_LOCK").is_some() {
        refuse_lock_update_in_ci(std::env::var_os("CI").is_some());
        std::fs::write(lock_path(), &now).unwrap();
        eprintln!("wrote {}", lock_path().display());
        return;
    }
    let locked = std::fs::read_to_string(lock_path())
        .expect("kjv-text.lock is missing; create it with KJV_UPDATE_LOCK=1")
        .replace("\r\n", "\n");
    if now != locked {
        let old: Vec<&str> = locked.lines().collect();
        let changed: Vec<String> = now
            .lines()
            .filter(|l| !old.contains(l))
            .map(|l| format!("  now:  {}", l))
            .chain(old.iter().filter(|l| !now.lines().any(|n| n == **l)).map(|l| format!("  was:  {}", l)))
            .take(60)
            .collect();
        panic!(
            "The KJV text or data changed. If this was deliberate and reviewed, run\n  KJV_UPDATE_LOCK=1 cargo test -p kjv-core --test text_fidelity\n{}",
            changed.join("\n")
        );
    }
}

// ---------------------------------------------------------------- 3. typography

#[test]
fn typography_is_consistent_in_every_verse() {
    let mut problems = Vec::new();
    for book in books() {
        let mut depth = 0i32; // parentheses may close in a later verse of the chapter
        let mut chapter = 0;
        for l in &book.lines {
            let at = book.at(l);
            let t = l.text.as_str();
            if l.chapter != chapter {
                if depth != 0 {
                    problems.push(format!("{} {}: unclosed parenthesis", book.name, chapter));
                }
                (depth, chapter) = (0, l.chapter);
            }
            let mut fail = |what: &str| problems.push(format!("{}: {}: {}", at, what, t));
            if t.is_empty() || t != t.trim() || t.contains("  ") {
                fail("empty, padded, or double-spaced");
            }
            if !t.starts_with(|c: char| c.is_uppercase() || c == '(') {
                fail("doesn't start with a capital");
            }
            if !t.ends_with(['.', ',', ';', ':', '?', '!', ')', '’']) && !RUN_ON_VERSES.contains(&at.as_str()) {
                fail("doesn't end with punctuation");
            }
            if t.chars().any(|c| c.is_ascii_digit() || c.is_control() || c == '\'' || c == '"') {
                fail("digit, control character, or straight quote");
            }
            let chars: Vec<char> = t.chars().collect();
            for (i, w) in chars.windows(2).enumerate() {
                let (a, b) = (w[0], w[1]);
                if a == ' ' && ",;:.?!)".contains(b) {
                    fail(&format!("space before {:?} at {}", b, i + 2));
                }
                if ",;:.?!".contains(a) && !(b == ' ' || b == '’' || b == ')') {
                    fail(&format!("no space after {:?} at {}", a, i + 1));
                }
                if (a == '(' && b == ' ') || (a.is_alphanumeric() && b == '(') {
                    fail(&format!("spacing around '(' at {}", i + 1));
                }
                if a == '-' && !(i > 0 && chars[i - 1].is_alphabetic() && b.is_alphabetic()) {
                    fail(&format!("hyphen not between letters at {}", i + 1));
                }
            }
            for c in t.chars() {
                depth += match c {
                    '(' => 1,
                    ')' => -1,
                    _ => 0,
                };
                if !(0..=1).contains(&depth) {
                    fail("parentheses out of order");
                    depth = depth.clamp(0, 1);
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Verse numbering, ordering, and the 116 Psalm titles, straight from the files.
#[test]
fn every_chapter_and_verse_is_present_once_in_order() {
    let (mut verses, mut chapters, mut titles) = (0, 0, 0);
    for book in books() {
        let mut prev = (0, 0); // (chapter, verse) of the line before
        for l in &book.lines {
            let at = book.at(l);
            let next_in_chapter = l.chapter == prev.0 && l.verse == prev.1 + 1;
            let starts_chapter = l.chapter == prev.0 + 1 && l.verse <= 1;
            assert!(next_in_chapter || starts_chapter, "{} follows {}:{}", at, prev.0, prev.1);
            if l.verse == 0 {
                assert_eq!(book.name, "Psalms", "{}: only Psalms have titles", at);
                titles += 1;
            } else {
                verses += 1;
            }
            if starts_chapter {
                chapters += 1;
            }
            prev = (l.chapter, l.verse);
        }
    }
    assert_eq!((verses, chapters, titles), (31_102, 1_189, 116));
}

// ---------------------------------------------------------------- 4. the files reach the app unchanged

#[test]
fn every_verse_reaches_the_app_unchanged() {
    let data = bundle();
    let options = ChapterOptions { red_letter: true, query: Some("the".into()), original: false };
    let joined = |segments: &[kjv_core::text::Segment]| segments.iter().map(|s| s.text.as_str()).collect::<String>();
    let mut problems = Vec::new();
    let mut checked = 0;

    for (bi, book) in books().iter().enumerate() {
        let app_book = &data.bible.books[bi];
        assert_eq!(app_book.name, book.name);
        let in_files = book.lines.len();
        let in_app: usize =
            app_book.chapters.iter().map(|c| c.verses.len() + usize::from(c.superscription.is_some())).sum();
        assert_eq!(in_app, in_files, "{}: the app has {} verses and titles, the file {}", book.name, in_app, in_files);

        let mut by_chapter: BTreeMap<u32, Vec<&Line>> = BTreeMap::new();
        for l in &book.lines {
            by_chapter.entry(l.chapter).or_default().push(l);
        }
        for (c, lines) in by_chapter {
            let view = api::chapter(data, &book.name, c, &options).unwrap();
            let mut copied = format!("{} KJV\n", view.heading);
            for l in lines {
                let at = book.at(l);
                let shown = if l.verse == 0 {
                    view.title.as_ref().map(|t| joined(&t.segments))
                } else {
                    view.verses.iter().find(|v| v.number == l.verse).map(|v| joined(&v.segments))
                };
                let stored = api::verse_text(data, &book.name, c, l.verse);
                if stored != Some(l.text.as_str()) {
                    problems.push(format!("{}: stored in the app as {:?}", at, stored));
                }
                if shown.as_deref() != Some(l.text.as_str()) {
                    problems.push(format!("{}: drawn as {:?}", at, shown));
                }
                if l.verse == 0 {
                    copied.push_str(&format!("{}\n", l.text));
                } else {
                    copied.push_str(&format!("{} {}\n", l.verse, l.text));
                    if api::copy_verse(data, &book.name, c, l.verse).as_deref()
                        != Some(&*format!("{} KJV\n{}", api::reference(&book.name, c, l.verse), l.text))
                    {
                        problems.push(format!("{}: copied verse differs", at));
                    }
                }
                checked += 1;
            }
            if api::copy_chapter(data, &book.name, c).as_deref() != Some(copied.as_str()) {
                problems.push(format!("{} {}: copied chapter differs", book.name, c));
            }
        }
    }
    assert_eq!(checked, 31_102 + 116);
    assert!(problems.is_empty(), "{}", problems.iter().take(50).cloned().collect::<Vec<_>>().join("\n"));
}

#[test]
fn search_results_show_each_verse_unchanged() {
    let data = bundle();
    let text: BTreeMap<(String, u32, u32), &str> = books()
        .iter()
        .flat_map(|b| b.lines.iter().map(move |l| ((b.name.clone(), l.chapter, l.verse), l.text.as_str())))
        .collect();
    // Common words reach nearly every verse; Æ and ’ are the text's rarest characters
    for query in ["the", "and", "lord", "Cæsar", "’s", "-"] {
        let results = api::search(data, query, Scope::All, None, usize::MAX);
        assert!(results.total > 0, "{:?} finds nothing", query);
        for hit in &results.hits {
            let shown: String = hit.segments.iter().map(|s| s.text.as_str()).collect();
            let want = text[&(hit.book.clone(), hit.chapter, hit.verse)];
            assert_eq!(shown, want, "{} as a search result for {:?}", hit.reference, query);
        }
    }
}

// ---------------------------------------------------------------- 5. Hebrew and Greek glyphs

/// Code points with a glyph in a TrueType font (cmap formats 4 and 12).
fn font_coverage(path: &Path) -> std::collections::HashSet<u32> {
    let f = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e));
    let u16_at = |o: usize| u16::from_be_bytes([f[o], f[o + 1]]) as usize;
    let u32_at = |o: usize| u32::from_be_bytes([f[o], f[o + 1], f[o + 2], f[o + 3]]) as usize;
    let cmap = (0..u16_at(4))
        .map(|i| 12 + 16 * i)
        .find(|&r| &f[r..r + 4] == b"cmap")
        .map(|r| u32_at(r + 8))
        .expect("font has a cmap table");
    let mut subtables: Vec<(usize, usize)> = (0..u16_at(cmap + 2))
        .map(|i| cmap + 4 + 8 * i)
        .map(|r| (u16_at(cmap + u32_at(r + 4)), cmap + u32_at(r + 4)))
        .collect();
    subtables.sort_by_key(|(format, _)| std::cmp::Reverse(*format == 12));
    let mut have = std::collections::HashSet::new();
    let (format, t) = *subtables.iter().find(|(format, _)| *format == 12 || *format == 4).expect("a Unicode cmap");
    if format == 12 {
        for g in 0..u32_at(t + 12) {
            let r = t + 16 + 12 * g;
            have.extend(u32_at(r) as u32..=u32_at(r + 4) as u32);
        }
    } else {
        let segs = u16_at(t + 6) / 2;
        let (ends, starts) = (t + 14, t + 16 + 2 * segs);
        let (deltas, range_offsets) = (starts + 2 * segs, starts + 4 * segs);
        for s in 0..segs {
            for c in u16_at(starts + 2 * s)..=u16_at(ends + 2 * s) {
                let ro = u16_at(range_offsets + 2 * s);
                let glyph = if ro == 0 {
                    (c + u16_at(deltas + 2 * s)) & 0xffff
                } else {
                    let g = u16_at(range_offsets + 2 * s + ro + 2 * (c - u16_at(starts + 2 * s)));
                    if g == 0 { 0 } else { (g + u16_at(deltas + 2 * s)) & 0xffff }
                };
                if glyph != 0 && c != 0xffff {
                    have.insert(c as u32);
                }
            }
        }
    }
    have
}

/// The `@font-face` rules in ui/styles.css: family -> (font file, unicode-range).
fn font_faces() -> BTreeMap<String, (PathBuf, Vec<(u32, u32)>)> {
    let css = std::fs::read_to_string(root().join("ui/styles.css")).unwrap();
    let value = |block: &str, name: &str| {
        let start =
            block.find(&format!("{}:", name)).unwrap_or_else(|| panic!("@font-face without {}", name)) + name.len() + 1;
        block[start..block[start..].find(';').unwrap() + start].trim().to_string()
    };
    css.split("@font-face")
        .skip(1)
        .map(|rest| {
            let block = &rest[..rest.find('}').unwrap()];
            let family = value(block, "font-family").trim_matches('"').to_string();
            let src = value(block, "src");
            let file = src.split('"').nth(1).expect("src: url(\"…\")");
            let ranges = value(block, "unicode-range")
                .split(',')
                .map(|r| {
                    let r = r.trim().trim_start_matches("U+");
                    let (a, b) = r.split_once('-').unwrap_or((r, r));
                    (u32::from_str_radix(a, 16).unwrap(), u32::from_str_radix(b, 16).unwrap())
                })
                .collect();
            (family, (root().join("ui").join(file), ranges))
        })
        .collect()
}

/// Every letter, vowel point, accent, and breathing mark in the Hebrew and Greek
/// words is drawn by the font the app ships for it, not left to whatever the
/// device happens to have (or to an empty box).
#[test]
fn bundled_fonts_draw_every_hebrew_and_greek_mark() {
    use kjv_core::models::OriginalLanguage;
    let faces = font_faces();
    let ext = &bundle().extended;
    for (family, languages) in [
        ("Noto Sans Hebrew", [OriginalLanguage::Hebrew, OriginalLanguage::Aramaic].as_slice()),
        ("Noto Sans Greek", [OriginalLanguage::Greek].as_slice()),
    ] {
        let (file, ranges) = faces.get(family).unwrap_or_else(|| panic!("styles.css has no @font-face for {}", family));
        let glyphs = font_coverage(file);
        // The reader itself: alef, and alpha with psili and oxia, are in; Hangul is not
        let probe = if family.contains("Hebrew") { 0x05D0 } else { 0x1F04 };
        assert!(glyphs.contains(&probe) && !glyphs.contains(&0xAC00), "{} cmap misread", family);
        // character -> (count, first place it appears)
        let mut used: BTreeMap<char, (usize, String)> = BTreeMap::new();
        for iv in ext.interlinear_ot.values().chain(ext.interlinear_nt.values()) {
            if !languages.contains(&iv.language) {
                continue;
            }
            for w in &iv.original_words {
                // Spaces (in a few multi-part words) need no glyph of their own
                for c in w.original_text.trim().chars().filter(|c| *c != ' ') {
                    used.entry(c)
                        .or_insert_with(|| (0, format!("{} {}:{}", iv.book, iv.chapter, iv.verse_number)))
                        .0 += 1;
                }
            }
        }
        let problems: Vec<String> = used
            .iter()
            .filter(|(c, _)| {
                let cp = **c as u32;
                !(ranges.iter().any(|&(a, b)| (a..=b).contains(&cp)) && glyphs.contains(&cp))
            })
            .map(|(c, (n, at))| {
                let cp = *c as u32;
                let why = if !ranges.iter().any(|&(a, b)| (a..=b).contains(&cp)) {
                    "outside its unicode-range"
                } else {
                    "no glyph in the font"
                };
                format!("  U+{:04X} {:?} ×{} (first in {}): {}", cp, c, n, at, why)
            })
            .collect();
        assert!(used.len() > 30, "{} has only {} distinct characters", family, used.len());
        assert!(problems.is_empty(), "{} ({}) can't draw:\n{}", family, file.display(), problems.join("\n"));
    }
}
