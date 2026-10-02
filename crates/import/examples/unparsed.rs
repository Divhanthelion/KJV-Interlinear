//! Lists every scripture reference a commentary cites that could not be converted to a
//! `to` (distinct forms with counts, most common first).
//!
//!     cargo run --release -p kjv-import --example unparsed -- tsk [limit]

use std::collections::BTreeMap;

use kjv_import::{commentaries, sources};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let id = args.first().expect("a commentary id");
    let limit: usize = args.get(1).map_or(60, |s| s.parse().expect("a number"));
    let entry = commentaries::catalogue().unwrap().into_iter().find(|e| &e.id == id).expect("a known id");
    let pinned = sources::load().unwrap();
    let built = commentaries::convert(&entry, &pinned).unwrap();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for u in &built.report.stats.unparsed {
        *counts.entry(u.as_str()).or_default() += 1;
    }
    let mut v: Vec<_> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    println!("{} resolved, {} unparsed ({} distinct)", built.report.stats.refs_resolved, built.report.stats.unparsed.len(), v.len());
    for (s, n) in v.into_iter().take(limit) {
        println!("{n:>6}  {s:?}");
    }
    println!("-- corrected from the shown text: {}", built.report.stats.refs_corrected.len());
    for s in built.report.stats.refs_corrected.iter().take(limit) {
        println!("         {s:?}");
    }
    println!("-- withheld (osisRef contradicted by a shown list): {}", built.report.stats.refs_withheld.len());
    for s in built.report.stats.refs_withheld.iter().take(limit) {
        println!("         {s:?}");
    }
    println!("-- impossible (a chapter or verse the KJV does not have): {}", built.report.stats.refs_impossible.len());
    for s in built.report.stats.refs_impossible.iter().take(limit) {
        println!("         {s:?}");
    }
}
