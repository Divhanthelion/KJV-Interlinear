//! The red-letter data and Psalm titles, checked against the USFM they came from:
//! eBible.org's 1769 KJV (`KJV_USFM=path/to/eng-kjv_usfm`, the extracted
//! eng-kjv_usfm.zip). Every `\wj` span must be in data/words_of_jesus.json and
//! every span there must be a `\wj` span, verse by verse and in order; every `\d`
//! line must be the verse-0 line of its Psalm in old_testament/Psalms.txt.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use kjv_core::parsing::CANONICAL_BOOKS;
use kjv_core::red_letter::RedLetterIndex;

/// USFM book codes (`\id`) in canonical order.
const USFM_CODES: [&str; 66] = [
    "GEN", "EXO", "LEV", "NUM", "DEU", "JOS", "JDG", "RUT", "1SA", "2SA", "1KI", "2KI", "1CH", "2CH", "EZR", "NEH",
    "EST", "JOB", "PSA", "PRO", "ECC", "SNG", "ISA", "JER", "LAM", "EZK", "DAN", "HOS", "JOL", "AMO", "OBA", "JON",
    "MIC", "NAM", "HAB", "ZEP", "HAG", "ZEC", "MAL", "MAT", "MRK", "LUK", "JHN", "ACT", "ROM", "1CO", "2CO", "GAL",
    "EPH", "PHP", "COL", "1TH", "2TH", "1TI", "2TI", "TIT", "PHM", "HEB", "JAS", "1PE", "2PE", "1JN", "2JN", "3JN",
    "JUD", "REV",
];

/// (book index, chapter, verse)
type Key = (usize, u32, u32);

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// USFM text as the KJV files store it (NOTICE, sections 1 and 3): word and
/// divine-name markup dropped with its attributes, supplied words (`\add`, printed
/// in italics) kept without their markers, footnotes removed, ¶ removed, and
/// whitespace collapsed.
fn plain(usfm: &str) -> String {
    let mut out = String::new();
    let mut rest = usfm;
    loop {
        let (text, after) = match rest.find('\\') {
            Some(i) => (&rest[..i], Some(&rest[i + 1..])),
            None => (rest, None),
        };
        // `\w word|strong="G1234"\w*`: the attributes run from '|' to the closing marker
        out.push_str(text.split_once('|').map_or(text, |(word, _)| word));
        let Some(after) = after else { break };
        // A marker is `+`?, letters and digits, then `*` if it closes; text may follow
        // a closing marker directly (`\+w give|strong="G4369"\+w*n.`)
        let name_end =
            after.char_indices().skip(1).find(|(_, c)| !c.is_ascii_alphanumeric()).map_or(after.len(), |(i, _)| i);
        let len = if after[name_end..].starts_with('*') { name_end + 1 } else { name_end };
        let marker = &after[..len];
        rest = &after[len..];
        if marker == "f" {
            let end = rest.find("\\f*").unwrap_or_else(|| panic!("unclosed footnote in {:?}", usfm));
            rest = &rest[end + 3..];
        } else if !marker.ends_with('*') {
            // An opening marker's delimiting space belongs to the marker
            rest = rest.strip_prefix(' ').unwrap_or(rest);
        }
    }
    out.replace('\u{b6}', " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The `\wj … \wj*` spans of one verse line, in order.
fn wj_spans(line: &str) -> Vec<String> {
    let mut spans = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find("\\wj ") {
        let body = &rest[start + 4..];
        let end = body.find("\\wj*").unwrap_or_else(|| panic!("unclosed \\wj in {:?}", line));
        spans.push(plain(&body[..end]));
        rest = &body[end + 4..];
    }
    spans
}

struct Usfm {
    /// Verses with words of Christ -> spans
    red: BTreeMap<Key, Vec<String>>,
    /// (book index, chapter) -> `\d` title
    titles: BTreeMap<(usize, u32), String>,
}

fn read_usfm(dir: &Path) -> Usfm {
    let mut usfm = Usfm { red: BTreeMap::new(), titles: BTreeMap::new() };
    let mut seen = [false; 66];
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {}", dir.display(), e))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "usfm"))
        .collect();
    files.sort();
    for path in files {
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e));
        let (mut book, mut chapter) = (None, 0);
        for line in text.lines() {
            let line = line.trim_start_matches('\u{feff}').trim_end();
            let at = || format!("{} {:?}", path.display(), line);
            if let Some(id) = line.strip_prefix("\\id ") {
                // Front matter and the Apocrypha have no place in the 66 books
                book = USFM_CODES.iter().position(|c| id.starts_with(c));
                if let Some(b) = book {
                    assert!(!seen[b], "{} appears twice", USFM_CODES[b]);
                    seen[b] = true;
                }
            }
            let Some(b) = book else { continue };
            if let Some(c) = line.strip_prefix("\\c ") {
                chapter = c.trim().parse().unwrap_or_else(|_| panic!("{}", at()));
            } else if let Some(title) = line.strip_prefix("\\d ") {
                assert!(usfm.titles.insert((b, chapter), plain(title)).is_none(), "second title at {}", at());
            } else if let Some(v) = line.strip_prefix("\\v ") {
                let (n, body) = v.split_once(' ').unwrap_or((v, ""));
                let verse: u32 = n.parse().unwrap_or_else(|_| panic!("{}", at()));
                let spans = wj_spans(body);
                if !spans.is_empty() {
                    assert!(usfm.red.insert((b, chapter, verse), spans).is_none(), "repeated verse at {}", at());
                }
            } else {
                assert!(!line.contains("\\wj"), "words of Christ outside a verse line: {}", at());
            }
        }
    }
    let missing: Vec<&str> = USFM_CODES.iter().zip(seen).filter(|(_, s)| !s).map(|(c, _)| *c).collect();
    assert!(missing.is_empty(), "no USFM file for {:?}", missing);
    usfm
}

fn name(k: &Key) -> String {
    format!("{} {}:{}", CANONICAL_BOOKS[k.0], k.1, k.2)
}

#[test]
fn words_of_christ_match_the_usfm_source() {
    let Some(dir) = std::env::var_os("KJV_USFM") else {
        eprintln!("skipped: set KJV_USFM to the extracted eng-kjv_usfm.zip to compare the red letter with its source");
        return;
    };
    let usfm = read_usfm(Path::new(&dir));

    let index = RedLetterIndex::load(&root().join("data/words_of_jesus.json")).expect("red-letter data loads");
    let mut ours: BTreeMap<Key, Vec<String>> = BTreeMap::new();
    for (book, chapter, verse) in index.keys() {
        let b = CANONICAL_BOOKS
            .iter()
            .position(|n| *n == book)
            .unwrap_or_else(|| panic!("red-letter book {:?} is not one of the 66", book));
        ours.insert((b, chapter, verse), index.get(&book, chapter, verse).unwrap().to_vec());
    }

    let mut problems = Vec::new();
    for k in usfm.red.keys().filter(|k| !ours.contains_key(k)) {
        problems.push(format!("{}: \\wj in the source, no red letter here: {:?}", name(k), usfm.red[k]));
    }
    for k in ours.keys().filter(|k| !usfm.red.contains_key(k)) {
        problems.push(format!("{}: red letter here, no \\wj in the source: {:?}", name(k), ours[k]));
    }
    for (k, spans) in &ours {
        if let Some(source) = usfm.red.get(k)
            && spans != source
        {
            problems.push(format!("{}:\n      source: {:?}\n      ours:   {:?}", name(k), source, spans));
        }
    }
    assert_eq!(usfm.red.values().map(Vec::len).sum::<usize>(), 2038, "the source has 2,038 \\wj spans");
    assert!(
        problems.is_empty(),
        "{} red-letter verses differ from the USFM source:\n{}",
        problems.len(),
        problems.iter().take(50).map(|p| format!("  {}", p)).collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn psalm_titles_match_the_usfm_source() {
    let Some(dir) = std::env::var_os("KJV_USFM") else {
        eprintln!(
            "skipped: set KJV_USFM to the extracted eng-kjv_usfm.zip to compare the Psalm titles with their source"
        );
        return;
    };
    let usfm = read_usfm(Path::new(&dir));
    let psalms = CANONICAL_BOOKS.iter().position(|b| *b == "Psalms").unwrap();
    assert!(usfm.titles.keys().all(|(b, _)| *b == psalms), "only Psalms have \\d titles");
    let source: BTreeMap<u32, &str> = usfm.titles.iter().map(|((_, c), t)| (*c, t.as_str())).collect();

    let path = root().join("old_testament/Psalms.txt");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e));
    let ours: BTreeMap<u32, &str> = text
        .lines()
        .filter_map(|line| {
            let (reference, title) = line.split_once(' ')?;
            let (c, v) = reference.split_once(':')?;
            (v == "0").then(|| (c.parse().unwrap(), title))
        })
        .collect();

    assert_eq!(source.len(), 116, "the source has 116 Psalm titles");
    let mut problems = Vec::new();
    for c in source.keys().chain(ours.keys()).collect::<std::collections::BTreeSet<_>>() {
        match (source.get(c), ours.get(c)) {
            (Some(s), Some(o)) if s == o => {}
            (s, o) => problems.push(format!("Psalm {}:\n      source: {:?}\n      ours:   {:?}", c, s, o)),
        }
    }
    assert!(
        problems.is_empty(),
        "{} Psalm titles differ from the USFM source:\n{}",
        problems.len(),
        problems.join("\n")
    );
}
