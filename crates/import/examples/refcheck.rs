//! Compares each OSIS `<reference osisRef>` with what the reference parser makes of the text
//! the reader sees, to find references the source got wrong.
//!
//!     cargo run --release -p kjv-import --example refcheck -- mhc [samples]

use kjv_import::commentaries;
use kjv_import::markup::{Dialect, Stats, Tok, tokenize};
use std::collections::BTreeSet;

use kjv_library::reference::{END, Options, Range, from_osis, parse_with};
use kjv_sword::kjv;
use kjv_sword::Module;

/// The verses a list of ranges covers (None if a range leaves the KJV's chapters).
fn verses(rs: &[Range]) -> Option<BTreeSet<(&'static str, u32, u32)>> {
    let mut out = BTreeSet::new();
    for r in rs {
        let book = kjv::book(r.book)?;
        let last_chapter = book.verses.len() as u32;
        let last = |c: u32| book.verses.get(c as usize - 1).map(|&v| v as u32);
        let (end_c, end_v) = if r.end.0 == END { (last_chapter, last(last_chapter)?) } else { (r.end.0, if r.end.1 == END { last(r.end.0)? } else { r.end.1 }) };
        if end_c > last_chapter {
            return None;
        }
        for c in r.start.0..=end_c {
            let (a, b) = (if c == r.start.0 { r.start.1.max(1) } else { 1 }, if c == end_c { end_v } else { last(c)? });
            for v in a..=b {
                out.insert((r.book, c, v));
            }
        }
    }
    Some(out)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let id = args.first().expect("a commentary id");
    let samples: usize = args.get(1).map_or(15, |s| s.parse().unwrap());
    let entry = commentaries::catalogue().unwrap().into_iter().find(|e| &e.id == id).expect("a known id");
    let module = Module::open_zip(&kjv_import::cache().join(&entry.source)).unwrap();
    let ex = module.read().unwrap();
    let (mut agree, mut differ, mut text_unparsable, mut osis_bad, mut none, mut outside) = (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    let opts = Options { jud_is_judges: entry.jud_is_judges, ..Options::default() };
    let mut shown = 0;
    let texts = ex.entries.iter().map(|e| (e.book, e.chapter, e.verse, e.text.as_str())).chain(ex.orphans.iter().map(|o| ("orphan", 0, 0, o.text.as_str())));
    for (book, c, v, raw) in texts {
        let toks = tokenize(raw, Dialect::Osis, &mut Stats::default()).unwrap();
        let mut i = 0;
        while i < toks.len() {
            if let Tok::Open(name, attrs) = &toks[i]
                && name == "reference"
            {
                {
                    let mut text = String::new();
                    let mut j = i + 1;
                    while j < toks.len() && !matches!(&toks[j], Tok::Close(n) if n == "reference") {
                        if let Tok::Text(t) = &toks[j] {
                            text.push_str(t);
                        }
                        j += 1;
                    }
                    let osis = attrs.iter().find(|(k, _)| k == "osisRef").map(|(_, v)| v.as_str());
                    match osis {
                        None => none += 1,
                        Some(o) => match from_osis(o) {
                            Err(_) => osis_bad += 1,
                            Ok(a) => match parse_with(&text, opts) {
                                Err(_) => text_unparsable += 1,
                                Ok(b) => {
                                    let (va, vb) = (verses(&a), verses(&b));
                                    if va.is_none() || vb.is_none() {
                                        outside += 1;
                                        if shown < samples * 3 && std::env::var("OUTSIDE").is_ok() {
                                            shown += 1;
                                            println!("OUTSIDE {book} {c}:{v}: osisRef={o:?} text={text:?}  (osis in KJV: {}, text in KJV: {})", va.is_some(), vb.is_some());
                                        }
                                    } else if va == vb {
                                        agree += 1;
                                    } else {
                                        differ += 1;
                                        if shown < samples {
                                            shown += 1;
                                            println!("{book} {c}:{v}: osisRef={o:?} text={text:?}");
                                        }
                                    }
                                }
                            },
                        },
                    }
                }
            }
            i += 1;
        }
    }
    println!("same verses {agree}, CONFLICT {differ}, outside the KJV's verses {outside}, text not parseable {text_unparsable}, osisRef unusable {osis_bad}, no osisRef {none}");
}
