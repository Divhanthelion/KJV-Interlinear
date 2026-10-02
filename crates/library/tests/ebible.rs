//! Every verse of every cached eBible translation, parsed from USFM, against eBible's
//! own plain-text edition (VPL) of the same translation. Two independent conversions
//! agreeing word for word is the check that our USFM reading loses nothing.
//!
//! Reads `.cache/sources/ebible/<id>_usfm.zip` and `<id>_vpl.zip`; skips when absent.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use kjv_library::usfm::{self, Options};

fn cache() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/sources/ebible")
}

/// eBible's VPL uses older book codes in places.
fn usfm_code(vpl: &str) -> &str {
    match vpl {
        "SOL" => "SNG",
        "EZE" => "EZK",
        "JOE" => "JOL",
        "NAH" => "NAM",
        "MAR" => "MRK",
        "JOH" => "JHN",
        "PHI" => "PHP",
        "JAM" => "JAS",
        "1JO" => "1JN",
        "2JO" => "2JN",
        "3JO" => "3JN",
        "EPJ" => "LJE",
        "PRA" => "S3Y",
        "PRM" => "MAN",
        "4ES" => "2ES",
        "DNG" => "DAG",
        "PSX" => "PS2",
        other => other,
    }
}

fn options(id: &str) -> Options {
    match id {
        // Psalm 119's letters are marked \qc (a centred line) in these editions
        "engkjvcpb" | "eng-asv" | "engasvbt" => Options { heading_markers: vec!["qc".into()] },
        _ => Options::default(),
    }
}

fn read_zip(path: &Path) -> zip::ZipArchive<std::fs::File> {
    zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap()
}

fn words(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

type Key = (String, u32, String);

fn check(id: &str) -> Result<usize, String> {
    let mut usfm_zip = read_zip(&cache().join(format!("{}_usfm.zip", id)));
    let mut ours: BTreeMap<Key, (String, Vec<String>)> = BTreeMap::new();
    for i in 0..usfm_zip.len() {
        let mut f = usfm_zip.by_index(i).unwrap();
        if !f.name().ends_with(".usfm") {
            continue;
        }
        let mut src = String::new();
        f.read_to_string(&mut src).unwrap();
        let book = usfm::parse(&src, &options(id)).map_err(|e| format!("{} {}: {}", id, f.name(), e))?;
        if usfm::is_peripheral(&book.code) {
            continue;
        }
        let mut titles: BTreeMap<u32, (String, Vec<String>)> = BTreeMap::new();
        for v in usfm::verses(&book) {
            if v.title {
                titles.insert(v.chapter, (v.text, v.asides));
                continue;
            }
            let (mut text, mut asides) = (v.text, v.asides);
            // The VPL runs a Psalm title into verse 1
            if v.number.split('-').next() == Some("1")
                && let Some((t, a)) = titles.remove(&v.chapter)
            {
                text = words(&format!("{} {}", t, text));
                asides.extend(a);
            }
            let key = (book.code.clone(), v.chapter, v.number);
            match ours.get_mut(&key) {
                Some(existing) => return Err(format!("{} {:?} appears twice ({:?})", id, key, existing.0)),
                None => {
                    ours.insert(key, (text, asides));
                }
            }
        }
    }

    let mut vpl_zip = read_zip(&cache().join(format!("{}_vpl.zip", id)));
    let mut vpl_text = String::new();
    vpl_zip.by_name(&format!("{}_vpl.txt", id)).unwrap().read_to_string(&mut vpl_text).unwrap();
    let mut theirs: BTreeMap<Key, String> = BTreeMap::new();
    for line in vpl_text.trim_start_matches('\u{feff}').lines() {
        let mut parts = line.splitn(3, ' ');
        let (Some(code), Some(reference)) = (parts.next(), parts.next()) else { continue };
        let Some((c, v)) = reference.split_once(':') else { continue };
        let Ok(c) = c.parse() else { continue };
        theirs.insert((usfm_code(code).to_string(), c, v.to_string()), words(parts.next().unwrap_or("")));
    }

    // eBible's VPL of the Catholic WEB leaves out Genesis; its text is WEB Updated's,
    // so that book is checked against WEB Updated's VPL instead
    let missing_books: Vec<String> = {
        let have: std::collections::BTreeSet<&str> = theirs.keys().map(|k| k.0.as_str()).collect();
        let mut v: Vec<String> = ours.keys().map(|k| k.0.clone()).filter(|b| !have.contains(b.as_str())).collect();
        v.dedup();
        v
    };
    if let Some(sibling) = match id {
        "eng-web-c" => Some("engwebu"),
        _ => None,
    } {
        let mut zip = read_zip(&cache().join(format!("{}_vpl.zip", sibling)));
        let mut text = String::new();
        zip.by_name(&format!("{}_vpl.txt", sibling)).unwrap().read_to_string(&mut text).unwrap();
        for line in text.trim_start_matches('\u{feff}').lines() {
            let mut parts = line.splitn(3, ' ');
            let (Some(code), Some(reference)) = (parts.next(), parts.next()) else { continue };
            if !missing_books.iter().any(|b| b == usfm_code(code)) {
                continue;
            }
            let Some((c, v)) = reference.split_once(':') else { continue };
            let Ok(c) = c.parse() else { continue };
            theirs.insert((usfm_code(code).to_string(), c, v.to_string()), words(parts.next().unwrap_or("")));
        }
    }

    // The VPL lists a bridged verse ("15-16") under its first number
    let bridged: Vec<Key> = ours.keys().filter(|k| k.2.contains('-') && !theirs.contains_key(*k)).cloned().collect();
    for k in bridged {
        let first = (k.0.clone(), k.1, k.2.split('-').next().unwrap().to_string());
        if theirs.contains_key(&first) && !ours.contains_key(&first) {
            let v = ours.remove(&k).unwrap();
            ours.insert(first, v);
        }
    }

    let mut problems = Vec::new();
    for k in theirs.keys().filter(|k| !ours.contains_key(*k)) {
        problems.push(format!("missing {:?}", k));
    }
    for k in ours.keys().filter(|k| !theirs.contains_key(*k)) {
        problems.push(format!("extra {:?}", k));
    }
    for (k, (text, asides)) in &ours {
        let Some(vpl) = theirs.get(k) else { continue };
        // The VPL marks supplied words [like this]; ours keeps them as styled text
        let mut want = words(&vpl.replace(['[', ']'], ""));
        let have = words(&text.replace(['[', ']'], ""));
        // It also runs headings that sit inside a verse (speaker labels, acrostic
        // letters) into the verse text; those are headings to us
        for a in asides {
            if !want.contains(a.as_str()) || have.contains(a.as_str()) {
                continue;
            }
            want = want.replacen(a.as_str(), " ", 1);
        }
        let want = words(&want);
        // And it puts a space before "Selah" set with \qs, where USFM has none
        let same = want == have || want.replace(" Selah", "Selah") == have.replace(" Selah", "Selah");
        if !same {
            let i = want.chars().zip(have.chars()).take_while(|(a, b)| a == b).count();
            let around = |s: &str| s.chars().skip(i.saturating_sub(30)).take(70).collect::<String>();
            problems.push(format!("{:?} at {}\n    vpl:  …{}\n    ours: …{}", k, i + 1, around(&want), around(&have)));
        }
    }
    if problems.is_empty() {
        Ok(ours.len())
    } else {
        let n = problems.len();
        Err(format!("{}: {} problems\n  {}", id, n, problems.into_iter().take(15).collect::<Vec<_>>().join("\n  ")))
    }
}

#[test]
fn every_cached_translation_matches_its_plain_text_edition() {
    let dir = cache();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("skipped: no {}", dir.display());
        return;
    };
    let mut ids: Vec<String> = entries
        .filter_map(|e| e.ok()?.file_name().into_string().ok()?.strip_suffix("_usfm.zip").map(str::to_string))
        .filter(|id| dir.join(format!("{}_vpl.zip", id)).exists())
        .collect();
    ids.sort();
    let mut failures = Vec::new();
    for id in &ids {
        match check(id) {
            Ok(n) => eprintln!("{}: {} verses match", id, n),
            Err(e) => failures.push(e),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
