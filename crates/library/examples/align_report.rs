//! Summarize how translations align with the KJV, by kind of match:
//! `cargo run --release -p kjv-library --example align_report [ids…]`.
//! (The tables themselves are made by `kjv-import align`.)

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use kjv_library::align::{self, Verse};
use kjv_library::usfm::{self, Options};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn verses(id: &str, code: &str) -> Option<Vec<Verse>> {
    let dir = root().join("data/library/bibles").join(id);
    let src = fs::read_to_string(dir.join(format!("{}.usfm", code))).ok()?;
    let index = fs::read_to_string(dir.join("index.toml")).unwrap();
    let markers: Vec<String> = index
        .lines()
        .find_map(|l| l.strip_prefix("heading_markers = "))
        .map(|v| v.trim_matches(['[', ']']).split(',').map(|s| s.trim().trim_matches('"').to_string()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    let book = usfm::parse(&src, &Options { heading_markers: markers }).ok()?;
    Some(usfm::verses(&book).into_iter().map(|v| Verse { chapter: v.chapter, number: v.number, title: v.title, text: v.text }).collect())
}

fn main() {
    let mut ids: Vec<String> = std::env::args().skip(1).collect();
    if ids.is_empty() {
        ids = fs::read_dir(root().join("data/library/bibles")).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|i| i != "kjv").collect();
        ids.sort();
    }
    let kjv = |code: &str| verses("kjv", code);
    for id in &ids {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for b in kjv_library::books::BOOKS {
            let Some(native) = verses(id, b.code) else { continue };
            for row in align::align_book(b.code, &native, &kjv) {
                let kind = match (row.native.len(), row.kjv.len()) {
                    (0, _) => "kjv-only".to_string(),
                    (_, 0) => "unmatched".to_string(),
                    _ if row.is_same_number() => format!("same/{:?}", row.how),
                    (1, 1) => format!("renumbered/{:?}", row.how),
                    _ => format!("joined/{:?}", row.how),
                };
                *counts.entry(kind).or_default() += 1;
            }
        }
        println!("{:<11} {:?}", id, counts);
    }
}
