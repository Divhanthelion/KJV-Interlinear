//! Translations: `data/library/bibles.toml` → `data/library/bibles/<id>/`.
//!
//! Each book is written as the source USFM with only two changes, both proven
//! harmless by parsing the result and comparing it with the source:
//! eBible's automatic Strong's tags are removed (`\w word|strong="H1234"\w*` → `word`),
//! and line endings and trailing spaces are normalized.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io::Read;

use kjv_library::usfm::{self, Book, Inline, NotePart, Options};
use serde::Deserialize;

use crate::{cache, library, sources};

#[derive(Debug, Clone, Deserialize)]
pub struct Entry {
    pub id: String,
    pub abbr: String,
    pub name: String,
    pub year: String,
    pub group: String,
    pub ebible: String,
    pub licence: String,
    pub credit: String,
    pub about: String,
    #[serde(default)]
    pub heading_markers: Vec<String>,
}

#[derive(Deserialize)]
struct Catalogue {
    bible: Vec<Entry>,
}

pub fn catalogue() -> Result<Vec<Entry>, String> {
    let path = library().join("bibles.toml");
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {}", path.display(), e))?;
    let c: Catalogue = toml::from_str(&text).map_err(|e| format!("{}: {}", path.display(), e))?;
    let mut seen = std::collections::HashSet::new();
    for b in &c.bible {
        if !seen.insert(b.id.clone()) {
            return Err(format!("bibles.toml: duplicate id {:?}", b.id));
        }
        if !b.id.chars().all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit()) {
            return Err(format!("bibles.toml: id {:?} must be lowercase letters and digits", b.id));
        }
        if !["pd", "cc-by-4.0", "cc-by-sa-4.0", "cc-by-nd-4.0", "cc-by-nc-nd-4.0"].contains(&b.licence.as_str()) {
            return Err(format!("bibles.toml: {} has unknown licence {:?}", b.id, b.licence));
        }
    }
    Ok(c.bible)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Write,
    /// Rebuild in memory and report any difference from data/library/
    Check,
}

pub fn build(ids: &[String], mode: Mode) -> Result<(), String> {
    let all = catalogue()?;
    let chosen: Vec<&Entry> = if ids.is_empty() {
        all.iter().collect()
    } else {
        ids.iter()
            .map(|id| all.iter().find(|b| &b.id == id).ok_or_else(|| format!("no translation {:?}", id)))
            .collect::<Result<_, _>>()?
    };
    let pinned = sources::load()?;
    let mut problems = Vec::new();
    for b in chosen {
        let files = convert(b, &pinned)?;
        let dir = library().join("bibles").join(&b.id);
        match mode {
            Mode::Write => {
                if dir.exists() {
                    fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
                }
                fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                for (name, text) in &files {
                    fs::write(dir.join(name), text).map_err(|e| e.to_string())?;
                }
                println!("{:<11} {} files", b.id, files.len());
            }
            Mode::Check => {
                let mut on_disk: BTreeMap<String, String> = BTreeMap::new();
                if let Ok(entries) = fs::read_dir(&dir) {
                    for e in entries.flatten() {
                        let name = e.file_name().to_string_lossy().into_owned();
                        let text = fs::read_to_string(e.path()).map_err(|e| e.to_string())?.replace("\r\n", "\n");
                        on_disk.insert(name, text);
                    }
                }
                for (name, text) in &files {
                    match on_disk.remove(name) {
                        Some(disk) if &disk == text => {}
                        Some(_) => problems.push(format!("bibles/{}/{} differs from what its source produces", b.id, name)),
                        None => problems.push(format!("bibles/{}/{} is missing", b.id, name)),
                    }
                }
                for name in on_disk.keys() {
                    problems.push(format!("bibles/{}/{} is not produced by its source", b.id, name));
                }
            }
        }
    }
    if problems.is_empty() {
        if mode == Mode::Check {
            println!("data/library/bibles matches its sources");
        }
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}

/// The files for one translation: one cleaned USFM file per book, plus `index.toml`.
fn convert(b: &Entry, pinned: &[sources::Source]) -> Result<BTreeMap<String, String>, String> {
    let rel = format!("ebible/{}_usfm.zip", b.ebible);
    let source = pinned.iter().find(|s| s.path == rel).ok_or_else(|| format!("{} is not pinned", rel))?;
    let file = fs::File::open(cache().join(&rel)).map_err(|e| format!("{}: {}", rel, e))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("{}: {}", rel, e))?;
    let options = Options { heading_markers: b.heading_markers.clone() };

    // Books in the source's order (its file names are numbered)
    let mut names: Vec<String> = zip.file_names().filter(|n| n.ends_with(".usfm")).map(str::to_string).collect();
    names.sort_by_key(|n| (n.split('-').next().and_then(|p| p.parse::<u32>().ok()).unwrap_or(u32::MAX), n.clone()));

    let mut files = BTreeMap::new();
    let mut index = String::new();
    let mut skipped = Vec::new();
    let mut totals = (0usize, 0usize);
    for name in names {
        let mut src = String::new();
        zip.by_name(&name).map_err(|e| e.to_string())?.read_to_string(&mut src).map_err(|e| format!("{}: {}", name, e))?;
        let original = usfm::parse(&src, &options).map_err(|e| format!("{} {}: {}", b.id, name, e))?;
        if usfm::is_peripheral(&original.code) {
            skipped.push(original.code);
            continue;
        }
        let cleaned = clean(&src);
        let reparsed = usfm::parse(&cleaned, &options).map_err(|e| format!("{} {} after cleaning: {}", b.id, name, e))?;
        let (a, c) = (normalized(&original), normalized(&reparsed));
        if a != c {
            let first = a
                .chapters
                .iter()
                .zip(&c.chapters)
                .flat_map(|(x, y)| x.blocks.iter().zip(&y.blocks).map(move |(p, q)| (x.number, p, q)))
                .find(|(_, p, q)| p != q)
                .map(|(ch, p, q)| {
                    let i = p.content.iter().zip(&q.content).take_while(|(x, y)| x == y).count();
                    format!(
                        "chapter {} ({} block, item {}):\n  source:  {:?}\n  cleaned: {:?}",
                        ch,
                        p.marker,
                        i,
                        p.content.get(i),
                        q.content.get(i)
                    )
                })
                .unwrap_or_else(|| "headers, introduction, or block counts differ".into());
            return Err(format!("{} {}: cleaning changed the book's content at {}", b.id, name, first));
        }
        if usfm::verses(&original) != usfm::verses(&reparsed) {
            return Err(format!("{} {}: cleaning changed a verse", b.id, name));
        }
        let verses = usfm::verses(&reparsed).iter().filter(|v| !v.title).count();
        totals.0 += reparsed.chapters.len();
        totals.1 += verses;
        writeln!(
            index,
            "\n[[book]]\ncode = \"{}\"\nname = {:?}\nchapters = {}\nverses = {}",
            reparsed.code,
            header(&reparsed, "h").unwrap_or(&reparsed.code),
            reparsed.chapters.len(),
            verses
        )
        .unwrap();
        // Chapters that aren't simply 1..n (Greek Esther from 10, selected chapters)
        let numbers: Vec<u32> = reparsed.chapters.iter().map(|c| c.number).collect();
        if numbers.iter().enumerate().any(|(i, &n)| n as usize != i + 1) {
            writeln!(index, "chapter_numbers = {:?}", numbers).unwrap();
        }
        files.insert(format!("{}.usfm", reparsed.code), cleaned);
    }

    let mut head = String::new();
    writeln!(head, "# Generated by kjv-import from data/library/bibles.toml and the pinned source. Do not edit.").unwrap();
    writeln!(head, "id = {:?}\nabbr = {:?}\nname = {:?}\nyear = {:?}\ngroup = {:?}", b.id, b.abbr, b.name, b.year, b.group).unwrap();
    writeln!(head, "licence = {:?}\ncredit = {:?}\nabout = {:?}", b.licence, b.credit, b.about).unwrap();
    writeln!(head, "source = {:?}\nsource_sha256 = {:?}", source.url, source.sha256).unwrap();
    writeln!(head, "heading_markers = {:?}", b.heading_markers).unwrap();
    writeln!(head, "skipped = {:?}  # front and back matter, glossaries, extra material", skipped).unwrap();
    writeln!(head, "chapters = {}\nverses = {}", totals.0, totals.1).unwrap();
    files.insert("index.toml".into(), head + &index);
    Ok(files)
}

fn header<'a>(book: &'a Book, name: &str) -> Option<&'a str> {
    book.headers.iter().find(|(n, _)| n == name).map(|(_, v)| v.trim())
}

/// The source with eBible's word tags removed and its whitespace tidied.
pub fn clean(src: &str) -> String {
    let src = src.strip_prefix('\u{feff}').unwrap_or(src).replace("\r\n", "\n");
    let mut out = String::with_capacity(src.len() / 2);
    let mut rest = src.as_str();
    while let Some(at) = rest.find('\\') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let plus = rest.starts_with("\\+w ");
        if plus || rest.starts_with("\\w ") {
            let open = if plus { 4 } else { 3 };
            let close = if plus { "\\+w*" } else { "\\w*" };
            if let Some(end) = rest[open..].find(close) {
                let inner = &rest[open..open + end];
                // Only plain words with attributes; anything with markup inside stays as is
                if !inner.contains('\\') {
                    out.push_str(inner.split('|').next().unwrap());
                    rest = &rest[open + end + close.len()..];
                    continue;
                }
            }
        }
        out.push('\\');
        rest = &rest[1..];
    }
    out.push_str(rest);
    let mut tidy = String::with_capacity(out.len());
    for line in out.lines() {
        tidy.push_str(line.trim_end());
        tidy.push('\n');
    }
    tidy
}

/// A book with word tags and whitespace differences removed, for comparing a source
/// with its cleaned copy.
fn normalized(book: &Book) -> Book {
    let mut b = book.clone();
    b.headers = b.headers.iter().map(|(n, v)| (n.clone(), squash(v))).collect();
    let blocks = b.intro.iter_mut().chain(b.chapters.iter_mut().flat_map(|c| c.blocks.iter_mut()));
    for block in blocks {
        let mut content: Vec<Inline> = Vec::new();
        for inline in block.content.drain(..) {
            let inline = match inline {
                Inline::Text { text, styles } => Inline::Text { text, styles: styles.into_iter().filter(|s| s != "w").collect() },
                Inline::Note { marker, caller, parts } => Inline::Note {
                    marker,
                    caller,
                    parts: parts.into_iter().map(|p| NotePart { marker: p.marker, text: squash(&p.text) }).collect(),
                },
                other => other,
            };
            match (content.last_mut(), inline) {
                (Some(Inline::Text { text: a, styles: sa }), Inline::Text { text: b, styles: sb }) if *sa == sb => a.push_str(&b),
                (_, inline) => content.push(inline),
            }
        }
        for inline in &mut content {
            if let Inline::Text { text, .. } = inline {
                *text = squash(text);
            }
        }
        // Space at either end of a paragraph separates nothing (the break already does)
        if let Some(Inline::Text { text, .. }) = content.first_mut() {
            *text = text.trim_start().to_string();
        }
        if let Some(Inline::Text { text, .. }) = content.last_mut() {
            *text = text.trim_end().to_string();
        }
        // A lone space between a note and a verse marker separates nothing; the exact
        // plain text of every verse is compared separately
        content.retain(|i| !matches!(i, Inline::Text { text, .. } if text.trim().is_empty()));
        block.content = content;
    }
    b
}

/// Whitespace runs as single spaces, ends kept (they separate words).
fn squash(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            space = true;
        } else {
            if space {
                out.push(' ');
                space = false;
            }
            out.push(c);
        }
    }
    if space {
        out.push(' ');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleaning_removes_word_tags_only() {
        let src = "\\id GEN\r\n\\c 1  \r\n\\p\r\n\\v 1 \\w In|strong=\"H7225\"\\w* \\wj \\+w the|strong=\"H1\"\\+w* beginning\\wj* \\w x|a=\"\\b\"\\w*  \r\n";
        assert_eq!(
            clean(src),
            "\\id GEN\n\\c 1\n\\p\n\\v 1 In \\wj the beginning\\wj* \\w x|a=\"\\b\"\\w*\n"
        );
    }
}
