//! Commentaries: `data/library/commentaries.toml` -> `data/library/commentaries/<id>/`.
//!
//! Each source is a SWORD module (read through `kjv-sword`). Every note is converted to the
//! library's note markup (see [`crate::markup`]) and proven to have exactly the source's
//! text. Decisions, each proven or counted in the build:
//!
//! * **Orphans.** Some modules (MHC, KD, JFB) hold text no verse index points at. Each
//!   orphan region follows, in the module's own byte order, the note whose range contains
//!   it, so it is appended to that note: the note for a range then contains everything
//!   printed for those verses. Every orphan must follow a note in the same book; every
//!   orphan byte lands in exactly one note.
//! * **Repeats.** Wesley and TSK repeat the previous book's last record at some books'
//!   first slots. The copies are dropped after checking they are identical to the original.
//! * **Cross-chapter ranges.** Wesley links 191 notes across a chapter boundary
//!   (Psalm 22:31 to 23:1). The notes discuss only their first verse; every such range
//!   ends exactly one verse before the next note begins, so the link is a gap fill. Such
//!   ranges are trimmed to the end of their first chapter and listed in `index.toml`.
//! * **Nothing silently fixed.** Control characters and U+FFFD in the sources are kept.

use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;
use std::fs;

use kjv_sword::kjv;
use kjv_sword::{Encoding, Extraction, Module};
use serde::{Deserialize, Serialize};

use crate::markup::{self, Dialect, Options, Stats};
use crate::bibles::Mode;
use crate::{cache, library, sources};

#[derive(Debug, Clone, Deserialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub author: String,
    pub year: String,
    pub tradition: String,
    pub coverage: String,
    pub licence: String,
    pub credit: String,
    pub about: String,
    /// Path under `.cache/sources/`
    pub source: String,
    pub url: String,
    /// "Jud" in scripture references means Judges (Wesley, TSK)
    #[serde(default)]
    pub jud_is_judges: bool,
    /// Trim notes linked across a chapter boundary to the end of their first chapter
    #[serde(default)]
    pub trim_cross_chapter: bool,
    /// Scripture references with no book are in the note's own book (TSK, Wesley)
    #[serde(default)]
    pub relative_refs: bool,
    /// How the source is read: a SWORD module (the default), or "tyndale" (the Tyndale
    /// Open Study Notes' XML, see [`crate::tyndale`])
    #[serde(default)]
    pub format: Option<String>,
    /// Which works of a source holding several this entry is: Tyndale "notes" (study
    /// notes and book introductions) or "articles" (profiles and themes)
    #[serde(default)]
    pub part: Option<String>,
}

#[derive(Deserialize)]
struct Catalogue {
    commentary: Vec<Entry>,
}

pub fn catalogue() -> Result<Vec<Entry>, String> {
    let path = library().join("commentaries.toml");
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {}", path.display(), e))?;
    let c: Catalogue = toml::from_str(&text).map_err(|e| format!("{}: {}", path.display(), e))?;
    let mut seen = HashSet::new();
    for b in &c.commentary {
        if !seen.insert(b.id.clone()) {
            return Err(format!("commentaries.toml: duplicate id {:?}", b.id));
        }
        if !b.id.chars().all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit()) {
            return Err(format!("commentaries.toml: id {:?} must be lowercase letters and digits", b.id));
        }
        let licence = match b.format.as_deref() {
            None => "pd",
            Some("tyndale") => "cc-by-sa-4.0",
            Some(other) => return Err(format!("commentaries.toml: {} has unknown format {:?}", b.id, other)),
        };
        if b.licence != licence {
            return Err(format!("commentaries.toml: {} has licence {:?}; its source is {:?}", b.id, b.licence, licence));
        }
    }
    Ok(c.commentary)
}

/// One converted note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub book: &'static str,
    pub from: (u32, u32),
    pub to: (u32, u32),
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trimmed {
    pub book: &'static str,
    pub from: (u32, u32),
    pub was_to: (u32, u32),
    pub to: (u32, u32),
}

/// A source entry with no text (only structure): its book, start, and end.
pub type EmptyNote = (&'static str, (u32, u32), (u32, u32));

#[derive(Debug, Default)]
pub struct Report {
    pub notes: usize,
    pub per_book: BTreeMap<&'static str, (usize, u64)>,
    pub chars: u64,
    pub body_bytes: u64,
    pub entries: usize,
    pub repeats_dropped: usize,
    pub orphans_merged: usize,
    pub orphan_bytes: usize,
    pub source_bytes: usize,
    pub empty: Vec<EmptyNote>,
    pub trimmed: Vec<Trimmed>,
    pub stats: Stats,
}

pub struct Options2 {
    pub dialect: Dialect,
    pub jud_is_judges: bool,
    pub trim_cross_chapter: bool,
    pub relative_refs: bool,
}

/// Converts everything a module read yielded. Pure: no files.
pub fn convert_extraction(label: &str, ex: &Extraction, opts: &Options2) -> Result<(Vec<Note>, Report), String> {
    let mut report = Report::default();
    let n = ex.entries.len();

    // Repeats: drop, after checking the copy is identical to the original.
    let mut repeated = vec![false; n];
    for r in &ex.repeats {
        let (copy, first) = (&ex.entries[r.entry], &ex.entries[r.first]);
        if copy.text != first.text {
            return Err(format!("{label}: a repeat at {} {}:{} differs from its original", copy.book, copy.chapter, copy.verse));
        }
        if repeated[r.entry] {
            return Err(format!("{label}: entry {} is listed as a repeat twice", r.entry));
        }
        repeated[r.entry] = true;
        report.repeats_dropped += 1;
    }
    for r in &ex.repeats {
        if repeated[r.first] {
            return Err(format!("{label}: a repeat's original is itself a repeat"));
        }
    }

    // Orphans: each is appended to the entry it follows.
    let mut appended: Vec<Vec<&kjv_sword::Orphan>> = vec![Vec::new(); n];
    for o in &ex.orphans {
        let after = o.after.ok_or_else(|| {
            format!("{label}: an orphan region (block {}, offset {}, {} bytes) follows no entry: {:.80}", o.block, o.offset, o.len, o.text)
        })?;
        if repeated[after] {
            return Err(format!("{label}: an orphan follows a repeated entry"));
        }
        let host = &ex.entries[after];
        if let Some(before) = o.before
            && ex.entries[before].book != host.book
        {
            return Err(format!(
                "{label}: an orphan after {} {}:{} is followed by an entry of {}",
                host.book, host.chapter, host.verse, ex.entries[before].book
            ));
        }
        appended[after].push(o);
        report.orphans_merged += 1;
        report.orphan_bytes += o.text.len();
    }
    for list in &mut appended {
        list.sort_by_key(|o| (o.testament as usize, o.block, o.offset));
    }

    let mut notes: Vec<Note> = Vec::new();
    let mut source_bytes = 0usize;
    for (i, e) in ex.entries.iter().enumerate() {
        if repeated[i] {
            continue;
        }
        report.entries += 1;
        let mut raw = e.text.clone();
        for o in &appended[i] {
            raw.push_str(&o.text);
        }
        source_bytes += raw.len();
        let ctx = |what: &str| format!("{label} {} {}:{}: {what}", e.book, e.chapter, e.verse);
        let (body, stats) =
            markup::convert(&raw, Options {
                dialect: opts.dialect,
                jud_is_judges: opts.jud_is_judges,
                context: opts.relative_refs.then_some((e.book, e.chapter)),
            }).map_err(|m| ctx(&m))?;
        report.stats.add(stats);
        let from = (e.chapter, e.verse);
        let mut to = (e.to_chapter, e.to_verse);
        if body.is_empty() {
            // Only structure: no text to keep. The proof still runs (it must see no text).
            let proof = markup::prove(&raw, &body, opts.dialect).map_err(|m| ctx(&m))?;
            debug_assert_eq!(proof.chars, 0);
            report.empty.push((e.book, from, to));
            continue;
        }
        let proof = markup::prove(&raw, &body, opts.dialect).map_err(|m| ctx(&format!("the converted note's text differs from the source's: {m}")))?;
        report.chars += proof.chars;
        report.stats.structural_gaps += proof.structural_gaps;
        if opts.trim_cross_chapter && to.0 != from.0 {
            let book = kjv::book(e.book).ok_or_else(|| ctx("unknown book"))?;
            let last = *book
                .verses
                .get(from.0 as usize - 1)
                .ok_or_else(|| ctx("a chapter the KJV versification does not have"))? as u32;
            let new = (from.0, last);
            report.trimmed.push(Trimmed { book: e.book, from, was_to: to, to: new });
            to = new;
        }
        report.body_bytes += body.len() as u64;
        let slot = report.per_book.entry(e.book).or_default();
        slot.0 += 1;
        slot.1 += proof.chars;
        notes.push(Note { book: e.book, from, to, body });
    }

    // Accounting: every distinct entry once, every orphan once.
    let entry_bytes: usize = ex.entries.iter().enumerate().filter(|(i, _)| !repeated[*i]).map(|(_, e)| e.text.len()).sum();
    if source_bytes != entry_bytes + report.orphan_bytes {
        return Err(format!(
            "{label}: {} source bytes went into notes but the module has {} entry bytes and {} orphan bytes",
            source_bytes, entry_bytes, report.orphan_bytes
        ));
    }
    if report.orphans_merged != ex.orphans.len() {
        return Err(format!("{label}: {} orphans merged of {}", report.orphans_merged, ex.orphans.len()));
    }
    if notes.len() + report.empty.len() != report.entries {
        return Err(format!("{label}: {} notes + {} empty != {} entries", notes.len(), report.empty.len(), report.entries));
    }
    // Canonical order: each book's notes together, starts strictly increasing within a book.
    let mut seen_books: HashSet<&str> = HashSet::new();
    for (i, n) in notes.iter().enumerate() {
        match i.checked_sub(1).map(|j| &notes[j]) {
            Some(prev) if prev.book == n.book => {
                if prev.from >= n.from {
                    return Err(format!("{label}: notes out of order in {}: {:?} then {:?}", n.book, prev.from, n.from));
                }
            }
            _ => {
                if !seen_books.insert(n.book) {
                    return Err(format!("{label}: the notes of {} are not contiguous", n.book));
                }
            }
        }
    }
    report.source_bytes = source_bytes;
    report.notes = notes.len();
    Ok((notes, report))
}

// ------------------------------------------------------------------------------------
// Files
// ------------------------------------------------------------------------------------

fn pos(p: (u32, u32)) -> String {
    format!("{}:{}", p.0, p.1)
}

/// A JSON string with every control character (C0, DEL, C1) and U+2028/9 escaped, so the
/// file shows exactly which odd characters a source holds.
fn json_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || (0x7f..=0x9f).contains(&(c as u32)) || c == '\u{2028}' || c == '\u{2029}' => {
                write!(out, "\\u{:04x}", c as u32).unwrap();
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// One book's file: a JSON object per line.
pub fn book_file(notes: &[&Note]) -> String {
    let mut out = String::new();
    for n in notes {
        out.push_str("{\"from\":");
        json_string(&pos(n.from), &mut out);
        if n.to != n.from {
            out.push_str(",\"to\":");
            json_string(&pos(n.to), &mut out);
        }
        out.push_str(",\"body\":");
        json_string(&n.body, &mut out);
        out.push_str("}\n");
    }
    out
}

#[derive(Serialize)]
struct BookCount {
    code: String,
    notes: usize,
    characters: u64,
}

#[derive(Serialize)]
struct TrimmedRange {
    book: String,
    from: String,
    was_to: String,
    to: String,
}

#[derive(Serialize)]
struct Index {
    id: String,
    name: String,
    author: String,
    year: String,
    tradition: String,
    coverage: String,
    licence: String,
    credit: String,
    about: String,
    source: String,
    source_sha256: String,
    module: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    module_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    module_text_source: Option<String>,
    markup: String,
    encoding: String,
    notes: usize,
    books: usize,
    /// Non-whitespace characters of note text (proven equal to the source's)
    characters: u64,
    body_bytes: u64,
    source_entries: usize,
    repeats_dropped: usize,
    orphans_merged: usize,
    orphan_bytes: usize,
    notes_without_text: usize,
    refs_resolved: u64,
    refs_unparsed: usize,
    refs_corrected: usize,
    refs_withheld: usize,
    refs_impossible: usize,
    #[serde(skip_serializing_if = "is_zero")]
    refs_renumbered: u64,
    footnotes: u64,
    structural_gaps: u64,
    bare_ampersands: u64,
    literal_markup: Vec<String>,
    unparsed_examples: Vec<String>,
    corrected_examples: Vec<String>,
    withheld: Vec<String>,
    impossible_examples: Vec<String>,
    ignored: BTreeMap<String, u64>,
    dropped_data: BTreeMap<String, u64>,
    #[serde(rename = "book")]
    book_counts: Vec<BookCount>,
    #[serde(rename = "trimmed")]
    trimmed: Vec<TrimmedRange>,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

fn examples(unparsed: &[String]) -> Vec<String> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for u in unparsed {
        *counts.entry(u.as_str()).or_default() += 1;
    }
    let mut v: Vec<(&str, usize)> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    v.into_iter().take(25).map(|(s, c)| format!("{s} ({c})")).collect()
}

/// Everything one commentary produces: relative path -> content.
pub struct Built {
    pub files: BTreeMap<String, String>,
    pub report: Report,
}

/// What a source was read as, for `index.toml`.
pub struct SourceInfo {
    pub module: String,
    pub version: Option<String>,
    pub text_source: Option<String>,
    pub markup: String,
    pub encoding: String,
}

pub fn convert(entry: &Entry, pinned: &[sources::Source]) -> Result<Built, String> {
    if entry.format.as_deref() == Some("tyndale") {
        return crate::tyndale::convert(entry, pinned);
    }
    let source = pinned.iter().find(|s| s.path == entry.source).ok_or_else(|| format!("{} is not pinned", entry.source))?;
    let module = Module::open_zip(&cache().join(&entry.source)).map_err(|e| format!("{}: {}", entry.source, e))?;
    let ex = module.read().map_err(|e| format!("{}: {}", entry.source, e))?;
    let dialect = match module.conf.get("SourceType") {
        Some("OSIS") => Dialect::Osis,
        Some("ThML") => Dialect::Thml,
        other => return Err(format!("{}: SourceType {:?} is not handled", entry.source, other)),
    };
    let opts = Options2 { dialect, jud_is_judges: entry.jud_is_judges, trim_cross_chapter: entry.trim_cross_chapter, relative_refs: entry.relative_refs };
    let (notes, report) = convert_extraction(&entry.id, &ex, &opts)?;
    let info = SourceInfo {
        module: module.name.clone(),
        version: module.conf.get("Version").map(str::to_string),
        text_source: module.conf.get("TextSource").map(str::to_string),
        markup: match dialect {
            Dialect::Osis => "OSIS".into(),
            Dialect::Thml => "ThML".into(),
            Dialect::Tyndale => unreachable!("a SWORD module"),
        },
        encoding: match ex.encoding {
            Encoding::Utf8 => "UTF-8".into(),
            Encoding::Latin1 => "Latin-1".into(),
        },
    };
    built(entry, source, info, &notes, report)
}

/// The files for a commentary: one per book, and `index.toml`.
pub fn built(entry: &Entry, source: &sources::Source, info: SourceInfo, notes: &[Note], report: Report) -> Result<Built, String> {
    let mut files = BTreeMap::new();
    let mut by_book: BTreeMap<&str, Vec<&Note>> = BTreeMap::new();
    for n in notes {
        by_book.entry(n.book).or_default().push(n);
    }
    for (code, list) in &by_book {
        files.insert(format!("{code}.jsonl"), book_file(list));
    }
    let index = Index {
        id: entry.id.clone(),
        name: entry.name.clone(),
        author: entry.author.clone(),
        year: entry.year.clone(),
        tradition: entry.tradition.clone(),
        coverage: entry.coverage.clone(),
        licence: entry.licence.clone(),
        credit: entry.credit.clone(),
        about: entry.about.clone(),
        source: source.url.clone(),
        source_sha256: source.sha256.clone(),
        module: info.module,
        module_version: info.version,
        module_text_source: info.text_source,
        markup: info.markup,
        encoding: info.encoding,
        notes: report.notes,
        books: report.per_book.len(),
        characters: report.chars,
        body_bytes: report.body_bytes,
        source_entries: report.entries,
        repeats_dropped: report.repeats_dropped,
        orphans_merged: report.orphans_merged,
        orphan_bytes: report.orphan_bytes,
        notes_without_text: report.empty.len(),
        refs_resolved: report.stats.refs_resolved,
        refs_unparsed: report.stats.unparsed.len(),
        refs_corrected: report.stats.refs_corrected.len(),
        refs_withheld: report.stats.refs_withheld.len(),
        refs_impossible: report.stats.refs_impossible.len(),
        refs_renumbered: report.stats.renumbered,
        footnotes: report.stats.footnotes,
        structural_gaps: report.stats.structural_gaps,
        bare_ampersands: report.stats.bare_ampersands,
        literal_markup: report.stats.literal_markup.clone(),
        unparsed_examples: examples(&report.stats.unparsed),
        corrected_examples: examples(&report.stats.refs_corrected),
        withheld: report.stats.refs_withheld.clone(),
        impossible_examples: examples(&report.stats.refs_impossible),
        ignored: report.stats.ignored.clone(),
        dropped_data: report.stats.dropped_data.clone(),
        book_counts: report.per_book.iter().map(|(c, (n, ch))| BookCount { code: (*c).to_string(), notes: *n, characters: *ch }).collect(),
        trimmed: report
            .trimmed
            .iter()
            .map(|t| TrimmedRange { book: t.book.to_string(), from: pos(t.from), was_to: pos(t.was_to), to: pos(t.to) })
            .collect(),
    };
    let body = toml::to_string(&index).map_err(|e| format!("index.toml: {e}"))?;
    files.insert(
        "index.toml".into(),
        format!("# Generated by kjv-import from data/library/commentaries.toml and the pinned source. Do not edit.\n{body}"),
    );
    Ok(Built { files, report })
}

fn summary(id: &str, r: &Report) -> String {
    format!(
        "{:<8} {:>6} notes in {:>2} books, {:>9} chars; {} orphans merged ({} bytes), {} repeats dropped, {} trimmed, {} empty; refs {} resolved ({} corrected) / {} unparsed / {} withheld / {} impossible; {} footnotes",
        id,
        r.notes,
        r.per_book.len(),
        r.chars,
        r.orphans_merged,
        r.orphan_bytes,
        r.repeats_dropped,
        r.trimmed.len(),
        r.empty.len(),
        r.stats.refs_resolved,
        r.stats.refs_corrected.len(),
        r.stats.unparsed.len(),
        r.stats.refs_withheld.len(),
        r.stats.refs_impossible.len(),
        r.stats.footnotes
    )
}

/// Builds (or checks) the chosen commentaries (all when `ids` is empty).
pub fn build(ids: &[String], mode: Mode) -> Result<(), String> {
    let all = catalogue()?;
    let chosen: Vec<&Entry> = if ids.is_empty() {
        all.iter().collect()
    } else {
        ids.iter()
            .map(|id| all.iter().find(|b| &b.id == id).ok_or_else(|| format!("no commentary {:?}", id)))
            .collect::<Result<_, _>>()?
    };
    let pinned = sources::load()?;
    let mut problems = Vec::new();
    // Modules are independent: convert them side by side.
    let results: Vec<(&Entry, Result<Built, String>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = chosen.iter().map(|e| (*e, scope.spawn(|| convert(e, &pinned)))).collect();
        handles
            .into_iter()
            .map(|(e, h)| (e, h.join().unwrap_or_else(|_| Err(format!("{}: the conversion panicked", e.id)))))
            .collect()
    });
    for (b, built) in results {
        let built = built.map_err(|e| format!("{}: {}", b.id, e))?;
        let dir = library().join("commentaries").join(&b.id);
        match mode {
            Mode::Write => {
                if dir.exists() {
                    fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
                }
                fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                for (name, text) in &built.files {
                    fs::write(dir.join(name), text).map_err(|e| e.to_string())?;
                }
                println!("{}", summary(&b.id, &built.report));
            }
            Mode::Check => {
                let mut on_disk: BTreeMap<String, String> = BTreeMap::new();
                if let Ok(entries) = fs::read_dir(&dir) {
                    for e in entries.flatten() {
                        let name = e.file_name().to_string_lossy().into_owned();
                        let bytes = fs::read(e.path()).map_err(|e| e.to_string())?;
                        let text = String::from_utf8(bytes).map_err(|_| format!("commentaries/{}/{} is not UTF-8", b.id, name))?;
                        // Git on Windows may have checked the file out with CRLF; a line
                        // ending is the only difference tolerated
                        on_disk.insert(name, text.replace("\r\n", "\n"));
                    }
                }
                for (name, text) in &built.files {
                    match on_disk.remove(name) {
                        Some(disk) if &disk == text => {}
                        Some(disk) => problems.push(format!(
                            "commentaries/{}/{} differs from what its source produces ({} vs {} bytes, first difference at byte {})",
                            b.id,
                            name,
                            disk.len(),
                            text.len(),
                            disk.bytes().zip(text.bytes()).take_while(|(x, y)| x == y).count()
                        )),
                        None => problems.push(format!("commentaries/{}/{} is missing", b.id, name)),
                    }
                }
                for name in on_disk.keys() {
                    problems.push(format!("commentaries/{}/{} is not produced by its source", b.id, name));
                }
            }
        }
    }
    if problems.is_empty() {
        if mode == Mode::Check {
            println!("data/library/commentaries matches its sources");
        }
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}

/// Prints each module's element inventory (every element with its attributes, counted),
/// including the text of orphans.
pub fn inventory(ids: &[String]) -> Result<(), String> {
    let all = catalogue()?;
    for e in all.iter().filter(|e| (ids.is_empty() || ids.contains(&e.id)) && e.format.is_none()) {
        let module = Module::open_zip(&cache().join(&e.source)).map_err(|x| format!("{}: {}", e.source, x))?;
        let ex = module.read().map_err(|x| format!("{}: {}", e.source, x))?;
        let dialect = if module.conf.get("SourceType") == Some("ThML") { Dialect::Thml } else { Dialect::Osis };
        let mut counts = BTreeMap::new();
        let texts = ex.entries.iter().map(|e| e.text.as_str()).chain(ex.orphans.iter().map(|o| o.text.as_str()));
        for t in texts {
            markup::inventory(t, dialect, &mut counts).map_err(|m| format!("{}: {m}", e.id))?;
        }
        println!("== {} ({:?})", e.id, dialect);
        for (k, v) in counts {
            println!("{v:>9}  {k}");
        }
    }
    Ok(())
}
