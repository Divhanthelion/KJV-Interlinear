//! The cross-reference collections (data/library/crossrefs/ and the Treasury's notes),
//! read as the app reads them.

use std::path::Path;
use std::sync::OnceLock;

use kjv_library::alignment::Ref;
use kjv_library::crossrefs::lines;
use kjv_library::{Library, library::build};

fn library() -> &'static Library {
    static LIB: OnceLock<Library> = OnceLock::new();
    LIB.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let (bytes, _) = build::archive(&root, &|b| zstd::encode_all(b, 1).unwrap()).unwrap();
        Library::open(bytes).unwrap()
    })
}

/// "ROM.5.8", "2CO.5.19-2CO.5.21", or a whole chapter ("ECC.7", its first verse) -> its ends
fn ends(to: &str) -> Vec<Ref> {
    to.split('-')
        .map(|p| {
            let mut it = p.split('.');
            let (code, c, v) = (it.next().unwrap(), it.next().unwrap(), it.next().unwrap_or("1"));
            (code.to_string(), c.parse().unwrap(), v.to_string())
        })
        .collect()
}

#[test]
fn the_catalogue() {
    let lib = library();
    let ids: Vec<&str> = lib.crossrefs().iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["tsk", "openbible"]);
    for c in lib.crossrefs() {
        assert_eq!(c.books.len(), 66, "{} covers the 66 books", c.id);
        assert!(!c.credit.is_empty() && !c.about.is_empty());
    }
    assert_eq!(lib.crossrefs()[0].commentary.as_deref(), Some("tsk"));
}

#[test]
fn every_openbible_reference_names_kjv_verses() {
    let lib = library();
    let info = lib.crossrefs().iter().find(|c| c.id == "openbible").unwrap();
    let (mut count, mut problems) = (0, Vec::new());
    for code in &info.books {
        let refs = lib.crossref_book("openbible", code).unwrap();
        let mut last: Option<((u32, u32), i32)> = None;
        for x in refs.iter() {
            count += 1;
            if !lib.has_verse("kjv", &(code.clone(), x.from.0, x.from.1.to_string())) {
                problems.push(format!("{} {}:{} is not a KJV verse", code, x.from.0, x.from.1));
            }
            for e in ends(&x.to) {
                if !lib.has_verse("kjv", &e) {
                    problems.push(format!("{} {}:{} -> {}: not a KJV verse", code, x.from.0, x.from.1, x.to));
                }
            }
            // In verse order, most votes first, none unhelpful
            if let Some((from, votes)) = last {
                assert!(from < x.from || (from == x.from && votes >= x.votes), "{} {:?} out of order", code, x.from);
            }
            assert!(x.votes >= 0);
            last = Some((x.from, x.votes));
        }
    }
    assert_eq!(count, 343_545);
    assert!(problems.is_empty(), "{}", problems.iter().take(20).cloned().collect::<Vec<_>>().join("\n"));
}

#[test]
fn every_treasury_reference_names_kjv_verses_and_reading_it_as_lines_loses_nothing() {
    let lib = library();
    let info = lib.commentaries().iter().find(|c| c.id == "tsk").unwrap();
    let (mut places, mut problems) = (0usize, Vec::new());
    for code in &info.books {
        for note in lib.commentary_book("tsk", code).unwrap().iter() {
            // Every place in the markup is in the lines, in order
            let marked: Vec<String> = note
                .body
                .split("<ref to=\"")
                .skip(1)
                .flat_map(|s| s[..s.find('"').unwrap()].split_whitespace().map(str::to_string).collect::<Vec<_>>())
                .collect();
            let read = lines(&note.body);
            let listed: Vec<String> = read.iter().flat_map(|l| l.refs.iter().map(|t| t.to.clone())).collect();
            assert_eq!(listed, marked, "{} {:?}: places", code, note.from);
            // Every word outside the references is in the lines
            let words = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
            let mut outside = String::new();
            let mut skipping = false;
            for piece in note.body.split('<') {
                let (tag, text) = piece.split_once('>').unwrap_or(("", piece));
                if tag.starts_with("ref to=") {
                    skipping = true;
                } else if tag == "/ref" {
                    skipping = false;
                } else if tag == "br/" || tag == "p" || tag == "/p" {
                    outside.push(' ');
                }
                if !skipping {
                    outside.push_str(&text.replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&"));
                }
            }
            let said = read.iter().map(|l| l.text.as_str()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ");
            assert_eq!(words(&said), words(&outside), "{} {:?}: words", code, note.from);
            for to in &marked {
                places += 1;
                for e in ends(to) {
                    if !lib.has_verse("kjv", &e) {
                        problems.push(format!("{} {:?}: {} is not a KJV verse", code, note.from, to));
                    }
                }
            }
        }
    }
    assert_eq!(places, 387_002);
    assert!(problems.is_empty(), "{} of {} places aren't KJV verses:\n{}", problems.len(), places, problems.iter().take(30).cloned().collect::<Vec<_>>().join("\n"));
}

#[test]
fn references_from_a_verse() {
    let lib = library();
    // The Treasury: keywords with their places
    let tsk = lib.crossrefs_from("tsk", "JHN", 3, 16).unwrap();
    assert_eq!(tsk[0].text, "God.");
    assert_eq!(tsk[0].refs[0].to, "LUK.2.14");
    // OpenBible: one list, most helpful first
    let ob = lib.crossrefs_from("openbible", "JHN", 3, 16).unwrap();
    assert_eq!(ob.len(), 1);
    assert_eq!((ob[0].refs[0].to.as_str(), ob[0].refs[0].votes), ("ROM.5.8", Some(984)));
    assert_eq!(ob[0].refs.len(), 23);
    // ESV 3 John 1:15 is the KJV's 1:14
    assert!(lib.crossrefs_from("openbible", "3JN", 1, 14).unwrap()[0].refs.iter().any(|t| t.to == "GEN.43.23"));
    // Nothing from the Apocrypha
    assert!(lib.crossrefs_from("openbible", "TOB", 1, 1).unwrap().is_empty());
}
