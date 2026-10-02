//! The real data/library/, packed into an archive and read back as the app will.

use std::path::Path;
use std::sync::OnceLock;

use kjv_library::view::Part;
use kjv_library::{Library, library::build};

fn library() -> &'static Library {
    static LIB: OnceLock<Library> = OnceLock::new();
    LIB.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let (bytes, _) = build::archive(&root, &|b| zstd::encode_all(b, 1).unwrap()).unwrap();
        Library::open(bytes).unwrap()
    })
}

fn text(parts: &[Part]) -> String {
    parts
        .iter()
        .map(|p| match p {
            // Printed verse labels ("36)") are drawn as labels, not words
            Part::Text { styles, .. } if styles.iter().any(|s| s == "vp" || s == "va") => " ",
            Part::Text { text, .. } => text.as_str(),
            Part::Break { .. } => " ",
            Part::Note { .. } => "",
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn every_translation_opens_and_every_chapter_draws() {
    let lib = library();
    assert_eq!(lib.bibles().len(), 44);
    let mut chapters = 0;
    for b in lib.bibles() {
        assert!(!b.books.is_empty(), "{} has no books", b.id);
        for book in &b.books {
            for c in 1..=book.chapters as u32 {
                // Some editions number chapters with gaps (Greek Esther A-F); skip absent ones
                if let Ok(view) = lib.chapter(&b.id, &book.code, c) {
                    assert!(!view.verses.is_empty() || view.title.is_some(), "{} {} {} is empty", b.id, book.code, c);
                    chapters += 1;
                }
            }
        }
    }
    assert!(chapters > 40_000, "{} chapters", chapters);
}

#[test]
fn known_verses() {
    let lib = library();
    let verse = |id: &str, code: &str, c: u32, v: &str| {
        let view = lib.chapter(id, code, c).unwrap();
        text(&view.verses.iter().find(|x| x.number == v).unwrap().parts)
    };
    assert_eq!(verse("kjv", "JHN", 11, "35"), "Jesus wept.");
    assert!(verse("web", "JHN", 3, "16").starts_with("For God so loved the world"));
    assert!(verse("dra", "PSA", 22, "1").contains("The Lord ruleth me"), "DRA Psalm 22 is KJV Psalm 23");
    assert!(verse("kjv", "TOB", 1, "1").starts_with("The book of the words of Tobit"));
    let ps23 = lib.chapter("kjv", "PSA", 23).unwrap();
    assert_eq!(text(&ps23.title.unwrap().parts), "A Psalm of David.");
    // Words of Jesus are marked
    let wept = lib.chapter("web", "JHN", 11, ).unwrap();
    let v43 = wept.verses.iter().find(|v| v.number == "43").unwrap();
    assert!(v43.parts.iter().any(|p| matches!(p, Part::Text { styles, .. } if styles.iter().any(|s| s == "wj"))));
}

/// What the reader draws for every verse of every translation reads exactly as the
/// verse's plain text, which is checked against eBible's own edition (ebible.rs).
#[test]
fn every_drawn_verse_reads_as_its_text() {
    let lib = library();
    let mut checked = 0;
    let mut problems = Vec::new();
    for b in lib.bibles() {
        for entry in &b.books {
            let book = lib.book(&b.id, &entry.code).unwrap();
            let plain: std::collections::HashMap<(u32, String, bool), String> = kjv_library::usfm::verses(&book)
                .into_iter()
                .map(|v| ((v.chapter, v.number, v.title), v.text))
                .collect();
            for ch in &book.chapters {
                let view = kjv_library::view::chapter(&book, ch.number).unwrap();
                let drawn = view
                    .title
                    .iter()
                    .map(|t| (true, t))
                    .chain(view.verses.iter().map(|v| (false, v)));
                for (title, v) in drawn {
                    let want = plain.get(&(ch.number, v.number.clone(), title)).cloned().unwrap_or_default();
                    let have = text(&v.parts);
                    if want != have && problems.len() < 20 {
                        problems.push(format!("{} {} {}:{}\n  text:  {}\n  drawn: {}", b.id, entry.code, ch.number, v.number, want, have));
                    }
                    checked += 1;
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    // Verses and Psalm titles across the 44 translations
    assert_eq!(checked, 1_155_017);
}
