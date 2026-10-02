//! Align every translation with the KJV book by book and report how verses
//! correspond: `cargo run --release -p kjv-library --example align_report [ids…] [--out dir]`.
//! With --out, writes one TSV per translation of every group that isn't a plain
//! same-number match.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use kjv_library::align::{How, align, refine, weigh};
use kjv_library::usfm::{self, Options};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// (chapter, number, title?, text) of every verse of `id`'s book `code`, in order.
fn verses(id: &str, code: &str) -> Option<Vec<(u32, String, bool, String)>> {
    let dir = root().join("data/library/bibles").join(id);
    let src = fs::read_to_string(dir.join(format!("{}.usfm", code))).ok()?;
    let index = fs::read_to_string(dir.join("index.toml")).unwrap();
    let markers: Vec<String> = index
        .lines()
        .find_map(|l| l.strip_prefix("heading_markers = "))
        .map(|v| v.trim_matches(['[', ']']).split(',').map(|s| s.trim().trim_matches('"').to_string()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    let book = usfm::parse(&src, &Options { heading_markers: markers }).ok()?;
    Some(usfm::verses(&book).into_iter().map(|v| (v.chapter, v.number, v.title, v.text)).collect())
}

/// The KJV book a native book is aligned with in order, and the KJV books whose
/// leftover verses it may also match (Douay-Rheims Daniel 13 is KJV Susanna).
fn pools(code: &str) -> (&str, &'static [&'static str]) {
    match code {
        "EST" => ("EST", &["ESG"]),
        "ESG" => ("ESG", &["EST"]),
        "DAN" => ("DAN", &["S3Y", "SUS", "BEL"]),
        "DAG" => ("DAN", &["S3Y", "SUS", "BEL"]),
        "S3Y" | "SUS" | "BEL" => (code, &["DAN"]),
        "LJE" => ("BAR", &[]),
        other => (other, &[]),
    }
}

fn label(v: &(u32, String, bool, String)) -> String {
    format!("{}:{}", v.0, v.1)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = args.iter().position(|a| a == "--out").map(|i| PathBuf::from(&args[i + 1]));
    let out_str = out.as_ref().map(|o| o.to_string_lossy().into_owned());
    let mut ids: Vec<String> = args.iter().filter(|a| !a.starts_with("--") && Some(a.as_str()) != out_str.as_deref()).cloned().collect();
    if ids.is_empty() {
        ids = fs::read_dir(root().join("data/library/bibles")).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|i| i != "kjv").collect();
        ids.sort();
    }
    let codes: Vec<&str> = kjv_library::books::BOOKS.iter().map(|b| b.code).collect();
    for id in &ids {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut rows = String::new();
        let mut per_book: BTreeMap<&str, usize> = BTreeMap::new();
        for code in &codes {
            let (primary, related) = pools(code);
            let (Some(a), Some(first)) = (verses(id, code), verses("kjv", primary)) else { continue };
            let in_order = first.len();
            let mut k: Vec<(String, (u32, String, bool, String))> = first.into_iter().map(|v| (primary.to_string(), v)).collect();
            for r in related {
                k.extend(verses("kjv", r).unwrap_or_default().into_iter().map(|v| (r.to_string(), v)));
            }
            let ta: Vec<String> = a.iter().map(|v| v.3.clone()).collect();
            let tk: Vec<String> = k.iter().map(|v| v.1.3.clone()).collect();
            let (pa, pk) = weigh(&ta, &tk);
            let same = |i: usize, j: usize| k[j].0 == *code && a[i].0 == k[j].1.0 && a[i].1 == k[j].1.1 && a[i].2 == k[j].1.2;
            let first_pass = align(&pa, &pk[..in_order], &same);
            for g in refine(&pa, &pk, first_pass, &same) {
                let la: Vec<String> = g.a.iter().map(|&i| label(&a[i])).collect();
                let lk: Vec<String> = g
                    .b
                    .iter()
                    .map(|&j| if k[j].0 == *code { label(&k[j].1) } else { format!("{} {}", k[j].0, label(&k[j].1)) })
                    .collect();
                let identical = g.a.len() == 1 && g.b.len() == 1 && same(g.a[0], g.b[0]);
                let kind = match (g.a.len(), g.b.len()) {
                    (_, 0) => "theirs-only".to_string(),
                    (0, _) => "kjv-only".to_string(),
                    _ if identical => format!("same/{:?}", g.how),
                    (1, 1) => format!("renumbered/{:?}", g.how),
                    _ => format!("joined/{:?}", g.how),
                };
                *counts.entry(kind.clone()).or_default() += 1;
                if !(identical && matches!(g.how, How::Content | How::Framed)) {
                    *per_book.entry(code).or_default() += 1;
                    rows.push_str(&format!("{}\t{}\t{}\t{:.2}\t{}\n", code, la.join("+"), lk.join("+"), g.score, kind));
                }
            }
        }
        let mut busiest: Vec<_> = per_book.iter().collect();
        busiest.sort_by(|x, y| y.1.cmp(x.1));
        println!("{:<11} {:?}", id, counts);
        println!("            {}", busiest.iter().take(8).map(|(c, n)| format!("{} {}", c, n)).collect::<Vec<_>>().join(", "));
        if let Some(dir) = &out {
            fs::create_dir_all(dir).unwrap();
            fs::write(dir.join(format!("{}.tsv", id)), rows).unwrap();
        }
    }
}
