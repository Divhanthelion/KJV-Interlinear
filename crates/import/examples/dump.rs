//! Dumps everything `kjv-sword` reads from a module (entries, orphans, headings) as JSON
//! lines, plus its conf, for inspection with other tools.
//!
//!     cargo run --release -p kjv-import --example dump -- .cache/sources/crosswire/MHC.zip out.jsonl

use kjv_sword::Module;
use std::io::Write;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let m = Module::open_zip(std::path::Path::new(&a[1])).unwrap();
    let ex = m.read().unwrap();
    let mut out = std::io::BufWriter::new(std::fs::File::create(&a[2]).unwrap());
    let rep: std::collections::HashSet<usize> = ex.repeats.iter().map(|r| r.entry).collect();
    for (i, e) in ex.entries.iter().enumerate() {
        let v = serde_json::json!({"kind":"entry","i":i,"repeat":rep.contains(&i),"book":e.book,"c":e.chapter,"v":e.verse,"tc":e.to_chapter,"tv":e.to_verse,"text":e.text});
        writeln!(out, "{}", v).unwrap();
    }
    for o in &ex.orphans {
        let v = serde_json::json!({"kind":"orphan","after":o.after,"before":o.before,"block":o.block,"off":o.offset,"text":o.text});
        writeln!(out, "{}", v).unwrap();
    }
    for h in &ex.headings { writeln!(out, "{}", serde_json::json!({"kind":"heading","text":h.text})).unwrap(); }
    let mut conf = std::io::BufWriter::new(std::fs::File::create(format!("{}.conf.txt", a[2])).unwrap());
    for (k, v) in &m.conf.entries {
        writeln!(conf, "{}={}", k, v).unwrap();
    }
}
