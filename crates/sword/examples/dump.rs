//! Dump a module's notes as JSON Lines.
//!
//! `cargo run -p kjv-sword --example dump -- <zip> [BOOK [chapter]]`
//!
//! Each line is `{"book","chapter","verse","to_chapter","to_verse","text"}`. With
//! `--orphans` the unindexed block regions are printed instead (see `Orphan`), and with
//! `--report` the per-testament counts and the encoding.

use std::path::PathBuf;
use std::process::ExitCode;

use kjv_sword::Module;
use serde_json::json;

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut orphans = false;
    let mut report = false;
    args.retain(|a| match a.as_str() {
        "--orphans" => {
            orphans = true;
            false
        }
        "--report" => {
            report = true;
            false
        }
        _ => true,
    });
    let Some(zip) = args.first() else {
        eprintln!("usage: dump <module.zip> [BOOK [chapter]] [--orphans] [--report]");
        return ExitCode::from(2);
    };
    let book = args.get(1).map(|b| b.to_ascii_uppercase());
    let chapter: Option<u32> = args.get(2).and_then(|c| c.parse().ok());

    let result = Module::open_zip(&PathBuf::from(zip)).and_then(|m| m.read());
    let extraction = match result {
        Ok(x) => x,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let keep =
        |b: &str, c: u32| book.as_deref().is_none_or(|w| w == b) && chapter.is_none_or(|w| w == c);

    if report {
        println!("{:#?}", extraction.reports);
        println!(
            "encoding: {:?} (detected: {})",
            extraction.encoding, extraction.encoding_detected
        );
        println!(
            "repeats: {}  orphans: {}",
            extraction.repeats.len(),
            extraction.orphans.len()
        );
    } else if orphans {
        for o in &extraction.orphans {
            let around = |i: Option<usize>| {
                i.map(|i| {
                    let e = &extraction.entries[i];
                    format!(
                        "{} {}:{}-{}:{}",
                        e.book, e.chapter, e.verse, e.to_chapter, e.to_verse
                    )
                })
            };
            println!(
                "{}",
                json!({
                    "testament": format!("{:?}", o.testament), "block": o.block,
                    "offset": o.offset, "len": o.len,
                    "after": around(o.after), "before": around(o.before), "text": o.text,
                })
            );
        }
    } else {
        for e in extraction
            .entries
            .iter()
            .filter(|e| keep(e.book, e.chapter))
        {
            println!(
                "{}",
                json!({
                    "book": e.book, "chapter": e.chapter, "verse": e.verse,
                    "to_chapter": e.to_chapter, "to_verse": e.to_verse, "text": e.text,
                })
            );
        }
    }
    ExitCode::SUCCESS
}
