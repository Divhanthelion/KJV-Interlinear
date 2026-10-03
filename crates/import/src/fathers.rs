//! The Church Fathers' homilies and commentaries (`data/library/fathers.toml`), from the
//! Christian Classics Ethereal Library's ThML editions of the Nicene and Post-Nicene
//! Fathers (public-domain translations), one commentary per Father.
//!
//! Each series is the run of divisions (homilies, tractates, expositions) inside one
//! division of a volume. A unit is placed on the passage it expounds (see the catalogue's
//! header): the catalogue's `keys`, else the first `<scripCom>` the edition places in it
//! that names more than a whole book, else its title read as a passage; it is the book's
//! introduction, or skipped, only where the catalogue says so. Where a unit's title names
//! where it begins ("Matthew XXII. 1-14."), the edition's key must begin there too: the
//! keys disagree with the titles 26 times (Homily LXIX on Matthew keyed 21:1-14, "Psalm
//! XXXV" keyed Psalm 1), and each is settled in the catalogue, so a disagreement left
//! unsettled is a build error, as is a unit that can't be placed. Every unit's text is converted with
//! [`markup::convert_with`] ([`Dialect::Ccel`], styled by its volume's stylesheet) and
//! proven equal to the edition's ([`markup::prove`]).

use std::collections::{BTreeMap, HashSet};
use std::fs;

use kjv_library::books;
use kjv_library::reference::{self, Context, Options as RefOptions, Range};
use kjv_sword::kjv;
use serde::Deserialize;

use crate::commentaries::{Built, Entry, Note, Report, SourceInfo, built};
use crate::markup::{self, Dialect, Lookups, Options, arabic};
use crate::{cache, library, sources};

#[derive(Debug, Clone, Deserialize)]
pub struct Series {
    pub commentary: String,
    pub volume: String,
    pub parent: String,
    pub work: String,
    pub book: String,
    pub extent: String,
    #[serde(default)]
    pub introduction: Vec<String>,
    #[serde(default)]
    pub skip: Vec<String>,
    #[serde(default)]
    pub keys: BTreeMap<String, String>,
    #[serde(default)]
    pub letters: bool,
    /// Each unit is the whole Psalm its title names ("Psalm XIII")
    #[serde(default)]
    pub title_keys: bool,
}

#[derive(Deserialize)]
struct Catalogue {
    series: Vec<Series>,
}

pub fn catalogue() -> Result<Vec<Series>, String> {
    let path = library().join("fathers.toml");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {}", path.display(), e))?;
    let c: Catalogue = toml::from_str(&text).map_err(|e| format!("{}: {}", path.display(), e))?;
    for s in &c.series {
        if !["key", "until-next"].contains(&s.extent.as_str()) {
            return Err(format!("fathers.toml: {}: extent {:?} (key or until-next)", s.parent, s.extent));
        }
        if books::by_code(&s.book).is_none() {
            return Err(format!("fathers.toml: {}: unknown book {:?}", s.parent, s.book));
        }
    }
    Ok(c.series)
}

/// The volumes a commentary's series come from, for `sources.toml`: (path under
/// `.cache/sources/`, URL).
pub fn volumes(commentary: &str) -> Result<Vec<(String, String)>, String> {
    let mut out: Vec<(String, String)> = Vec::new();
    for s in catalogue()?.iter().filter(|s| s.commentary == commentary) {
        let v = (format!("ccel/{}.xml", s.volume), format!("https://www.ccel.org/ccel/schaff/{}.xml", s.volume));
        if !out.contains(&v) {
            out.push(v);
        }
    }
    Ok(out)
}

// ------------------------------------------------------------------ a volume's divisions

#[derive(Debug)]
struct Div {
    title: String,
    /// Byte range of its content (between its tags) in the volume
    content: (usize, usize),
    parent: Option<usize>,
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let start = tag.find(&format!(" {name}=\""))? + name.len() + 3;
    let len = tag[start..].find('"')?;
    Some(tag[start..start + len].replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\""))
}

/// Every `<divN>` of the volume, in order, with its parent.
fn divisions(xml: &str) -> Result<Vec<Div>, String> {
    let mut out: Vec<Div> = Vec::new();
    let mut open: Vec<(usize, u8)> = Vec::new();
    let mut at = 0;
    while let Some(i) = xml[at..].find("<div").map(|i| i + at).into_iter().chain(xml[at..].find("</div").map(|i| i + at)).min() {
        let tag_end = xml[i..].find('>').ok_or("a tag never closed")? + i;
        let tag = &xml[i..=tag_end];
        at = tag_end + 1;
        let closing = tag.starts_with("</");
        let digits: String = tag.trim_start_matches("</").trim_start_matches('<').trim_start_matches("div").chars().take_while(|c| c.is_ascii_digit()).collect();
        let Ok(level) = digits.parse::<u8>() else { continue }; // <div class=…>: not a numbered division
        if closing {
            let (k, l) = open.pop().ok_or_else(|| format!("{tag} closes nothing"))?;
            if l != level {
                return Err(format!("{tag} closes a div{l}"));
            }
            out[k].content.1 = i;
        } else {
            out.push(Div { title: attribute(tag, "title").unwrap_or_default(), content: (tag_end + 1, 0), parent: open.last().map(|(k, _)| *k) });
            open.push((out.len() - 1, level));
        }
    }
    if !open.is_empty() {
        return Err("a division is never closed".into());
    }
    Ok(out)
}

/// The volume's stylesheet, from its `<style>`.
fn stylesheet(xml: &str) -> String {
    let head = &xml[..xml.find("<ThML.body>").unwrap_or(0)];
    head.find("<style").and_then(|i| {
        let open = head[i..].find('>')? + i + 1;
        let close = head[open..].find("</style>")? + open;
        Some(head[open..close].to_string())
    })
    .unwrap_or_default()
}

// ------------------------------------------------------------------ passages

/// Where a unit's title says it begins: the first chapter numeral followed by a verse
/// ("Matthew XXII. 1-14." (22, 1), "Homily I on Acts i. 1, 2." (1, 1), "Hebrews 1.6—8"
/// (1, 6), "Chapter III. 6–21." (3, 6), "… Gospel, Mark xiii. 32, …" (13, 32)), or a Psalm
/// ("Psalm XIII": (13, 0)).
pub fn title_start(title: &str) -> Option<(u32, u32)> {
    let normal = arabic(&title.replace(['—', '–'], "-"));
    let b = normal.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() && (i == 0 || !b[i - 1].is_ascii_alphanumeric()) {
            let c_end = i + normal[i..].find(|c: char| !c.is_ascii_digit()).unwrap_or(normal.len() - i);
            let rest = &normal[c_end..];
            let after = rest.strip_prefix(':').or_else(|| rest.strip_prefix('.')).map(str::trim_start);
            if let Some(after) = after
                && after.starts_with(|c: char| c.is_ascii_digit())
            {
                let v: String = after.chars().take_while(char::is_ascii_digit).collect();
                return Some((normal[i..c_end].parse().ok()?, v.parse().ok()?));
            }
            i = c_end;
        } else {
            i += 1;
        }
    }
    let n = normal.strip_prefix("Psalm ")?;
    let digits: String = n.chars().take_while(char::is_ascii_digit).collect();
    Some((digits.parse().ok()?, 0))
}

/// A key: the ranges an `osisRef` ("Bible:Matt.5.1-Matt.5.2") or a printed passage names.
fn read(osis: Option<&str>, printed: Option<&str>, book: &str) -> Option<Vec<Range>> {
    if let Some(r) = osis.filter(|r| !r.trim().is_empty())
        && let Ok(ranges) = reference::from_osis(r)
    {
        return Some(ranges);
    }
    let printed = printed?;
    let options = RefOptions { context: books::by_code(book).map(|b| Context { book: b.code, chapter: 0 }), ..RefOptions::default() };
    reference::parse_with(&arabic(printed), options).ok()
}

/// The verse before (c, v) in the KJV, if any.
fn before(book: &str, (c, v): (u32, u32)) -> Option<(u32, u32)> {
    if v > 1 {
        return Some((c, v - 1));
    }
    let b = kjv::book(book)?;
    let c = c.checked_sub(1).filter(|c| *c > 0)?;
    Some((c, u32::from(*b.verses.get(c as usize - 1)?)))
}

/// A range's first and last verse in the KJV (a whole chapter from its first verse to its last).
fn span(r: &Range) -> Option<((u32, u32), (u32, u32))> {
    let b = kjv::book(r.book)?;
    let last = |c: u32| b.verses.get(c.checked_sub(1)? as usize).map(|&v| u32::from(v));
    let chapters = b.verses.len() as u32;
    let start = (r.start.0, r.start.1.max(1));
    let end_c = if r.end.0 == reference::END { chapters } else { r.end.0 };
    let end_v = if r.end.0 == reference::END || r.end.1 == reference::END { last(end_c)? } else { r.end.1 };
    Some((start, (end_c, end_v)))
}

/// The verses of Psalm 119's section `k` (0 Aleph … 21 Tau): eight each.
fn letter(k: usize) -> ((u32, u32), (u32, u32)) {
    let first = 8 * k as u32 + 1;
    ((119, first), (119, first + 7))
}

// ------------------------------------------------------------------ conversion

/// A passage a unit is on: (book, from, to).
type Passage = (&'static str, (u32, u32), (u32, u32));

/// What a unit was placed on.
enum Place {
    /// One or more passages
    Passages(Vec<Passage>),
    Introduction,
    Skip,
}

pub fn convert(entry: &Entry, pinned: &[sources::Source]) -> Result<Built, String> {
    let all = catalogue()?;
    let series: Vec<&Series> = all.iter().filter(|s| s.commentary == entry.id).collect();
    if series.is_empty() {
        return Err(format!("{}: no series in fathers.toml", entry.id));
    }
    let mut report = Report::default();
    let mut notes: Vec<Note> = Vec::new();
    let mut volumes_read: Vec<String> = Vec::new();
    let mut by_volume: BTreeMap<&str, (String, Vec<Div>, Lookups)> = BTreeMap::new();
    for s in &series {
        if by_volume.contains_key(s.volume.as_str()) {
            continue;
        }
        let rel = format!("ccel/{}.xml", s.volume);
        let pin = pinned.iter().find(|p| p.path == rel).ok_or_else(|| format!("{rel} is not pinned"))?;
        volumes_read.push(format!("{} ({})", s.volume, &pin.sha256[..16]));
        let xml = fs::read_to_string(cache().join(&rel)).map_err(|e| format!("{rel}: {e}"))?;
        let divs = divisions(&xml).map_err(|e| format!("{rel}: {e}"))?;
        let lookups = Lookups { styles: markup::ccel_styles(&stylesheet(&xml)), ..Lookups::default() };
        by_volume.insert(&s.volume, (xml, divs, lookups));
    }
    for s in &series {
        let (xml, divs, lookups) = &by_volume[s.volume.as_str()];
        let ctx = |m: &str| format!("{} {} {:?}: {m}", entry.id, s.volume, s.parent);
        let parents: Vec<usize> = (0..divs.len()).filter(|&k| divs[k].title == s.parent).collect();
        let [parent] = parents[..] else { return Err(ctx(&format!("{} divisions so titled", parents.len()))) };
        let units: Vec<usize> = (0..divs.len()).filter(|&k| divs[k].parent == Some(parent)).collect();
        if s.letters && units.len() != 22 {
            return Err(ctx(&format!("{} sections, not Psalm 119's 22", units.len())));
        }
        let book = books::by_code(&s.book).unwrap().code;
        // Each unit's place, before extents
        let mut placed: Vec<(usize, Place)> = Vec::new();
        let mut used: HashSet<&str> = HashSet::new();
        for (k, &u) in units.iter().enumerate() {
            let d = &divs[u];
            let raw = &xml[d.content.0..d.content.1];
            let title = d.title.as_str();
            let place = if s.introduction.iter().any(|t| t == title) {
                used.insert(title);
                Place::Introduction
            } else if title == "Title Page." {
                Place::Skip
            } else if s.skip.iter().any(|t| t == title) {
                used.insert(title);
                Place::Skip
            } else if s.letters {
                let (from, to) = letter(k);
                Place::Passages(vec![("PSA", from, to)])
            } else if s.title_keys {
                let (c, _) = title_start(title).ok_or_else(|| ctx(&format!("{title:?} names no Psalm")))?;
                let r = Range::chapter(book, c).map_err(|e| ctx(&format!("{title:?}: {e:?}")))?;
                let (from, to) = span(&r).ok_or_else(|| ctx(&format!("{title:?}: no such Psalm")))?;
                Place::Passages(vec![(r.book, from, to)])
            } else {
                let ranges = match s.keys.get(title) {
                    Some(over) => {
                        used.insert(title);
                        reference::from_osis(over).map_err(|e| ctx(&format!("{title:?}: {e:?}")))?
                    }
                    None => {
                        // The edition's keys, in order; the first naming more than a whole book
                        let mut keys: Vec<Vec<Range>> = Vec::new();
                        let mut at = 0;
                        while let Some(i) = raw[at..].find("<scripCom") {
                            let tag_end = raw[at + i..].find('>').map(|e| at + i + e).unwrap_or(raw.len());
                            let tag = &raw[at + i..tag_end];
                            if let Some(r) = read(attribute(tag, "osisRef").as_deref(), attribute(tag, "passage").as_deref(), book) {
                                keys.push(r);
                            }
                            at = tag_end;
                        }
                        let key = keys.iter().find(|r| !r.iter().all(|x| x.is_whole_book())).or(keys.first()).cloned();
                        // The edition's key must begin where the title says the unit does
                        if let (Some(k), Some((c, v))) = (&key, title_start(title))
                            && let Some(first) = k.first()
                            && (first.start.0 != c || (v > 0 && first.start.1 > 0 && first.start.1 != v))
                        {
                            return Err(ctx(&format!(
                                "unit {title:?}: the edition's key {} disagrees with its title; settle it in keys",
                                k.iter().map(|r| r.osis()).collect::<Vec<_>>().join(" ")
                            )));
                        }
                        match key.or_else(|| read(None, Some(title.split(';').next().unwrap_or(title)), book)) {
                            Some(r) => r,
                            None => return Err(ctx(&format!("unit {title:?} has no passage (add it to keys, introduction, or skip)"))),
                        }
                    }
                };
                let mut passages = Vec::new();
                for r in &ranges {
                    let (from, to) = span(r).ok_or_else(|| ctx(&format!("{title:?}: {} isn't in the KJV", r.osis())))?;
                    passages.push((r.book, from, to));
                }
                Place::Passages(passages)
            };
            placed.push((u, place));
        }
        for t in s.introduction.iter().chain(&s.skip).chain(s.keys.keys()) {
            if !used.contains(t.as_str()) {
                return Err(ctx(&format!("{t:?} names no unit")));
            }
        }
        // Extents: a unit runs to the verse before the next unit's start
        if s.extent == "until-next" {
            let starts: Vec<Option<(u32, u32)>> = placed
                .iter()
                .map(|(_, p)| match p {
                    Place::Passages(v) => v.first().filter(|x| x.0 == book).map(|x| x.1),
                    _ => None,
                })
                .collect();
            for k in 0..placed.len() {
                let next = starts[k + 1..].iter().flatten().next().copied();
                let title = divs[placed[k].0].title.clone();
                if let Place::Passages(v) = &mut placed[k].1 {
                    let Some(first) = v.first_mut() else { continue };
                    if first.0 != book {
                        return Err(ctx(&format!("{title:?} is on {}, not {}", first.0, book)));
                    }
                    let end = match next {
                        Some(n) if n > first.1 => before(book, n).unwrap_or(first.2),
                        Some(_) => first.2,
                        None => {
                            let b = kjv::book(book).unwrap();
                            (b.verses.len() as u32, u32::from(*b.verses.last().unwrap()))
                        }
                    };
                    first.2 = first.2.max(end);
                    v.truncate(1);
                }
            }
        }
        for (u, place) in placed {
            let d = &divs[u];
            let raw = &xml[d.content.0..d.content.1];
            report.entries += 1;
            let label = |m: &str| ctx(&format!("{:?}: {m}", d.title));
            let passages = match place {
                Place::Skip => {
                    *report.stats.ignored.entry(format!("units skipped (see fathers.toml): {}", s.work)).or_default() += 1;
                    continue;
                }
                Place::Introduction => vec![(book, (0, 0), (0, 0))],
                Place::Passages(v) => v,
            };
            report.source_bytes += raw.len();
            let options = Options { dialect: Dialect::Ccel, jud_is_judges: false, context: None };
            let (body, stats) = markup::convert_with(raw, options, lookups).map_err(|m| label(&m))?;
            report.stats.add(stats);
            let proof = markup::prove(raw, &body, Dialect::Ccel).map_err(|m| label(&format!("the converted text differs from the edition's: {m}")))?;
            if body.is_empty() {
                report.empty.push((book, (0, 0), (0, 0)));
                continue;
            }
            for (b, from, to) in passages {
                report.chars += proof.chars;
                report.stats.structural_gaps += proof.structural_gaps;
                report.body_bytes += body.len() as u64;
                let slot = report.per_book.entry(b).or_default();
                slot.0 += 1;
                slot.1 += proof.chars;
                notes.push(Note { book: b, from, to, body: body.clone() });
            }
        }
    }
    notes.sort_by_key(|n| (books::order(n.book), n.from, n.to));
    report.notes = notes.len();
    let source = sources::Source {
        path: "ccel".into(),
        url: "https://www.ccel.org/ccel/schaff/".into(),
        size: 0,
        sha256: volumes_read.join(", "),
        retrieved: String::new(),
    };
    let info = SourceInfo {
        module: format!("Christian Classics Ethereal Library ThML: {}", series.iter().map(|s| format!("{} ({})", s.work, s.volume)).collect::<Vec<_>>().join("; ")),
        version: None,
        text_source: None,
        markup: "CCEL ThML".into(),
        encoding: "UTF-8".into(),
    };
    built(entry, &source, info, &notes, report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printed_passages() {
        assert_eq!(arabic("Matt. V. 1, 2"), "Matt. 5:1, 2");
        assert_eq!(arabic("Psalm XI"), "Psalm 11");
        assert_eq!(arabic("Philippians i. 8-11"), "Philippians 1:8-11");
        assert_eq!(arabic("1 John V. 7, 8"), "1 John 5:7, 8");
        assert_eq!(title_start("Matthew XXII. 1-14."), Some((22, 1)));
        assert_eq!(title_start("Homily I on Acts i. 1, 2."), Some((1, 1)));
        assert_eq!(title_start("Hebrews 1.6—8"), Some((1, 6)));
        assert_eq!(title_start("Ephesians 1:1--2"), Some((1, 1)));
        assert_eq!(title_start("Chapter III. 6–21."), Some((3, 6)));
        assert_eq!(title_start("On the words of the Gospel, Mark xiii. 32, ‘But of that day’"), Some((13, 32)));
        assert_eq!(title_start("Psalm XIII"), Some((13, 0)));
        assert_eq!(title_start("Homily II"), None);
        assert_eq!(title_start("Argument."), None);
        assert_eq!(letter(0), ((119, 1), (119, 8)));
        assert_eq!(letter(21), ((119, 169), (119, 176)));
        assert_eq!(before("MAT", (5, 1)), Some((4, 25)));
        assert_eq!(before("MAT", (5, 17)), Some((5, 16)));
    }
}
