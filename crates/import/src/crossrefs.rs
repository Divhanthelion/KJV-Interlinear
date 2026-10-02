//! Cross-references: `data/library/crossrefs.toml` -> `data/library/crossrefs/<id>/`.
//!
//! OpenBible.info's `cross_references.txt` (from, to, votes; OSIS references in the
//! ESV's numbering) becomes one file per book, `<BOOK>.tsv`, of
//! `chapter:verse \t to \t votes` with each verse's references most-voted first and
//! `to` written as note markup writes it (`ROM.5.8`, `2CO.5.19-2CO.5.21`). Decisions,
//! each counted in `index.toml`:
//!
//! * **Unhelpful references.** Those with negative votes (more readers found them
//!   unhelpful than helpful) are left out.
//! * **Numbering.** The ESV's 3 John 1:15 is the second half of the KJV's 1:14 and is
//!   numbered so; a reference that then repeats one already there keeps the higher
//!   votes. Every other reference must name verses the KJV has, or the build fails.
//!
//! The Treasury is a commentary already (data/library/commentaries/tsk/); its entry
//! here names that commentary and nothing is built for it.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::io::Read;

use kjv_library::{books, usfm};
use serde::Deserialize;

use crate::bibles::Mode;
use crate::{cache, commentaries, library, sources};

#[derive(Debug, Clone, Deserialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub short: String,
    pub licence: String,
    pub credit: String,
    pub about: String,
    /// The references are this commentary's notes (the Treasury)
    #[serde(default)]
    pub commentary: Option<String>,
    /// Path under `.cache/sources/`, for a collection imported here
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Deserialize)]
struct Catalogue {
    crossrefs: Vec<Entry>,
}

pub fn catalogue() -> Result<Vec<Entry>, String> {
    let path = library().join("crossrefs.toml");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {}", path.display(), e))?;
    let c: Catalogue = toml::from_str(&text).map_err(|e| format!("{}: {}", path.display(), e))?;
    let comms = commentaries::catalogue()?;
    let mut seen = HashSet::new();
    for e in &c.crossrefs {
        if !seen.insert(e.id.clone()) {
            return Err(format!("crossrefs.toml: duplicate id {:?}", e.id));
        }
        if !e.id.chars().all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit()) {
            return Err(format!("crossrefs.toml: id {:?} must be lowercase letters and digits", e.id));
        }
        if !["pd", "cc-by-4.0"].contains(&e.licence.as_str()) {
            return Err(format!("crossrefs.toml: {} has licence {:?}", e.id, e.licence));
        }
        match (&e.commentary, &e.source, &e.url) {
            (Some(c), None, None) => {
                if !comms.iter().any(|x| &x.id == c) {
                    return Err(format!("crossrefs.toml: {} names commentary {:?}, which isn't in commentaries.toml", e.id, c));
                }
            }
            (None, Some(_), Some(_)) => {}
            _ => return Err(format!("crossrefs.toml: {} needs either `commentary` or `source` and `url`", e.id)),
        }
    }
    Ok(c.crossrefs)
}

/// OSIS book names (as OpenBible.info writes them) and our codes.
const OSIS: &[(&str, &str)] = &[
    ("Gen", "GEN"), ("Exod", "EXO"), ("Lev", "LEV"), ("Num", "NUM"), ("Deut", "DEU"), ("Josh", "JOS"),
    ("Judg", "JDG"), ("Ruth", "RUT"), ("1Sam", "1SA"), ("2Sam", "2SA"), ("1Kgs", "1KI"), ("2Kgs", "2KI"),
    ("1Chr", "1CH"), ("2Chr", "2CH"), ("Ezra", "EZR"), ("Neh", "NEH"), ("Esth", "EST"), ("Job", "JOB"),
    ("Ps", "PSA"), ("Prov", "PRO"), ("Eccl", "ECC"), ("Song", "SNG"), ("Isa", "ISA"), ("Jer", "JER"),
    ("Lam", "LAM"), ("Ezek", "EZK"), ("Dan", "DAN"), ("Hos", "HOS"), ("Joel", "JOL"), ("Amos", "AMO"),
    ("Obad", "OBA"), ("Jonah", "JON"), ("Mic", "MIC"), ("Nah", "NAM"), ("Hab", "HAB"), ("Zeph", "ZEP"),
    ("Hag", "HAG"), ("Zech", "ZEC"), ("Mal", "MAL"), ("Matt", "MAT"), ("Mark", "MRK"), ("Luke", "LUK"),
    ("John", "JHN"), ("Acts", "ACT"), ("Rom", "ROM"), ("1Cor", "1CO"), ("2Cor", "2CO"), ("Gal", "GAL"),
    ("Eph", "EPH"), ("Phil", "PHP"), ("Col", "COL"), ("1Thess", "1TH"), ("2Thess", "2TH"), ("1Tim", "1TI"),
    ("2Tim", "2TI"), ("Titus", "TIT"), ("Phlm", "PHM"), ("Heb", "HEB"), ("Jas", "JAS"), ("1Pet", "1PE"),
    ("2Pet", "2PE"), ("1John", "1JN"), ("2John", "2JN"), ("3John", "3JN"), ("Jude", "JUD"), ("Rev", "REV"),
];

type Place = (&'static str, u32, u32);

#[derive(Debug, Default)]
struct Report {
    rows: usize,
    kept: usize,
    unhelpful: usize,
    renumbered: usize,
    merged: usize,
}

/// "Rom.5.8" -> ("ROM", 5, 8), renumbered into the KJV's numbering.
fn place(osis: &str, report: &mut Report) -> Result<Place, String> {
    let mut it = osis.split('.');
    let (Some(book), Some(c), Some(v), None) = (it.next(), it.next(), it.next(), it.next()) else {
        return Err(format!("bad reference {:?}", osis));
    };
    let code = OSIS.iter().find(|(o, _)| *o == book).map(|(_, c)| *c).ok_or_else(|| format!("unknown book in {:?}", osis))?;
    let c: u32 = c.parse().map_err(|_| format!("bad chapter in {:?}", osis))?;
    let v: u32 = v.parse().map_err(|_| format!("bad verse in {:?}", osis))?;
    if (code, c, v) == ("3JN", 1, 15) {
        report.renumbered += 1;
        return Ok((code, 1, 14));
    }
    Ok((code, c, v))
}

fn write_place(p: Place) -> String {
    format!("{}.{}.{}", p.0, p.1, p.2)
}

/// Every verse of the KJV (from data/library/bibles/kjv/), as the app numbers them.
fn kjv_verses() -> Result<HashSet<(String, u32, u32)>, String> {
    let dir = library().join("bibles/kjv");
    let mut out = HashSet::new();
    for e in fs::read_dir(&dir).map_err(|e| format!("{}: {}", dir.display(), e))?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(code) = name.strip_suffix(".usfm") else { continue };
        let src = fs::read_to_string(e.path()).map_err(|e| e.to_string())?.replace("\r\n", "\n");
        let book = usfm::parse(&src, &usfm::Options::default()).map_err(|e| format!("kjv {}: {}", code, e))?;
        for v in usfm::verses(&book) {
            let (lo, hi) = v.number.split_once('-').unwrap_or((&v.number, &v.number));
            let (lo, hi): (u32, u32) = (lo.parse().unwrap_or(0), hi.parse().unwrap_or(0));
            for n in lo..=hi {
                out.insert((code.to_string(), v.chapter, n));
            }
        }
    }
    Ok(out)
}

struct Built {
    files: BTreeMap<String, String>,
}

fn openbible(entry: &Entry, pinned: &[sources::Source]) -> Result<Built, String> {
    let rel = entry.source.as_deref().unwrap();
    let pin = pinned.iter().find(|s| s.path == rel).ok_or_else(|| format!("{} is not pinned", rel))?;
    let file = fs::File::open(cache().join(rel)).map_err(|e| format!("{}: {}", rel, e))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("{}: {}", rel, e))?;
    let mut text = String::new();
    zip.by_name("cross_references.txt")
        .map_err(|e| format!("{}: cross_references.txt: {}", rel, e))?
        .read_to_string(&mut text)
        .map_err(|e| format!("{}: {}", rel, e))?;
    let mut lines = text.lines();
    let header = lines.next().unwrap_or("");
    if !header.starts_with("From Verse\tTo Verse\tVotes") {
        return Err(format!("{}: unexpected header {:?}", rel, header));
    }
    let kjv = kjv_verses()?;
    let mut report = Report::default();
    let mut problems = Vec::new();
    // (from, to) -> votes, merging any repeat the renumbering makes
    let mut refs: HashMap<(Place, Place, Place), i32> = HashMap::new();
    for line in lines {
        report.rows += 1;
        let fields: Vec<&str> = line.split('\t').collect();
        let [from, to, votes] = fields[..] else {
            return Err(format!("{}: bad line {:?}", rel, line));
        };
        let votes: i32 = votes.parse().map_err(|_| format!("{}: bad votes in {:?}", rel, line))?;
        if votes < 0 {
            report.unhelpful += 1;
            continue;
        }
        let from = place(from, &mut report)?;
        let (start, end) = match to.split_once('-') {
            Some((a, b)) => (place(a, &mut report)?, place(b, &mut report)?),
            None => {
                let p = place(to, &mut report)?;
                (p, p)
            }
        };
        for p in [from, start, end] {
            if !kjv.contains(&(p.0.to_string(), p.1, p.2)) {
                problems.push(format!("{}: {} is not a KJV verse", line, write_place(p)));
            }
        }
        match refs.get_mut(&(from, start, end)) {
            Some(v) => {
                report.merged += 1;
                *v = (*v).max(votes);
            }
            None => {
                refs.insert((from, start, end), votes);
            }
        }
    }
    if !problems.is_empty() {
        return Err(format!("{} references name verses the KJV hasn't:\n{}", problems.len(), problems.join("\n")));
    }
    report.kept = refs.len();
    // Per book: by verse, most votes first, then in the Bible's order
    let order = |p: &Place| (books::order(p.0).unwrap_or(usize::MAX), p.1, p.2);
    let mut rows: Vec<(Place, Place, Place, i32)> = refs.into_iter().map(|((f, s, e), v)| (f, s, e, v)).collect();
    rows.sort_by(|a, b| {
        (order(&a.0), std::cmp::Reverse(a.3), order(&a.1), order(&a.2)).cmp(&(order(&b.0), std::cmp::Reverse(b.3), order(&b.1), order(&b.2)))
    });
    let mut files: BTreeMap<String, String> = BTreeMap::new();
    for (from, start, end, votes) in &rows {
        let file = files.entry(format!("{}.tsv", from.0)).or_insert_with(|| "# chapter:verse\tto\tvotes (most first)\n".to_string());
        let to = if start == end { write_place(*start) } else { format!("{}-{}", write_place(*start), write_place(*end)) };
        writeln!(file, "{}:{}\t{}\t{}", from.1, from.2, to, votes).unwrap();
    }
    let mut index = String::new();
    writeln!(index, "# Generated by kjv-import from data/library/crossrefs.toml and the pinned source. Do not edit.").unwrap();
    writeln!(index, "id = {:?}", entry.id).unwrap();
    writeln!(index, "source = {:?}", entry.url.as_deref().unwrap_or("")).unwrap();
    writeln!(index, "source_sha256 = {:?}", pin.sha256).unwrap();
    writeln!(index, "books = {}", files.len()).unwrap();
    writeln!(index, "references = {}", report.kept).unwrap();
    writeln!(index, "# of {} rows: {} left out as unhelpful (negative votes); {} places renumbered (3 John 1:15 -> 1:14), {} repeats merged",
        report.rows, report.unhelpful, report.renumbered, report.merged).unwrap();
    files.insert("index.toml".into(), index);
    Ok(Built { files })
}

pub fn build(mode: Mode) -> Result<(), String> {
    let pinned = sources::load()?;
    let mut problems = Vec::new();
    for e in catalogue()?.iter().filter(|e| e.source.is_some()) {
        let built = match e.id.as_str() {
            "openbible" => openbible(e, &pinned)?,
            other => return Err(format!("crossrefs.toml: no importer for {:?}", other)),
        };
        let dir = library().join("crossrefs").join(&e.id);
        match mode {
            Mode::Write => {
                if dir.exists() {
                    fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
                }
                fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                for (name, text) in &built.files {
                    fs::write(dir.join(name), text).map_err(|e| e.to_string())?;
                }
                let index = &built.files["index.toml"];
                println!("{:<11} {}", e.id, index.lines().filter(|l| l.starts_with("references") || l.starts_with("# of")).collect::<Vec<_>>().join("; "));
            }
            Mode::Check => {
                let mut on_disk: BTreeMap<String, String> = BTreeMap::new();
                if let Ok(entries) = fs::read_dir(&dir) {
                    for f in entries.flatten() {
                        let text = fs::read_to_string(f.path()).map_err(|err| err.to_string())?;
                        on_disk.insert(f.file_name().to_string_lossy().into_owned(), text.replace("\r\n", "\n"));
                    }
                }
                for (name, text) in &built.files {
                    match on_disk.remove(name) {
                        Some(disk) if &disk == text => {}
                        Some(_) => problems.push(format!("crossrefs/{}/{} differs from what its source produces", e.id, name)),
                        None => problems.push(format!("crossrefs/{}/{} is missing", e.id, name)),
                    }
                }
                for name in on_disk.keys() {
                    problems.push(format!("crossrefs/{}/{} is not produced by its source", e.id, name));
                }
            }
        }
    }
    if problems.is_empty() {
        if mode == Mode::Check {
            println!("data/library/crossrefs matches its sources");
        }
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}
