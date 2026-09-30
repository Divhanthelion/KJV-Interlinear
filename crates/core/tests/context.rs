//! Scripture context for AI chat, checked against the real bundled data.

use std::path::Path;
use std::sync::OnceLock;

use kjv_core::bundle::DataBundle;
use kjv_core::context::{self, ContextOptions, Scope};

fn data() -> &'static DataBundle {
    static DATA: OnceLock<DataBundle> = OnceLock::new();
    DATA.get_or_init(|| {
        DataBundle::from_sources(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")))
            .expect("bundle builds")
    })
}

fn plain() -> ContextOptions {
    ContextOptions::default()
}

fn verse(book: &str, chapter: u32, verse: u32) -> Scope {
    Scope::Verse { book: book.into(), chapter, verse }
}

#[test]
fn one_verse() {
    let c = context::build(data(), &verse("John", 3, 16), &plain()).unwrap();
    assert_eq!(c.label, "John 3:16");
    assert_eq!(
        c.text,
        "# John\n## John 3\n16 For God so loved the world, that he gave his only begotten Son, \
         that whosoever believeth in him should not perish, but have everlasting life.\n"
    );
    assert_eq!(c.verses, 1);
}

#[test]
fn psalm_titles_come_first_and_are_marked() {
    let c = context::build(data(), &Scope::Chapter { book: "Psalms".into(), chapter: 51 }, &plain()).unwrap();
    assert_eq!(c.label, "Psalm 51");
    let lines: Vec<&str> = c.text.lines().collect();
    assert_eq!(lines[1], "## Psalm 51");
    assert!(lines[2].starts_with("(title) To the chief Musician"), "{}", lines[2]);
    assert!(lines[3].starts_with("1 Have mercy upon me, O God"), "{}", lines[3]);
    // 19 verses plus the title
    assert_eq!(c.verses, 20);
    let title = context::build(data(), &verse("Psalms", 51, 0), &plain()).unwrap();
    assert_eq!(title.label, "Psalm 51 (title)");
}

#[test]
fn verse_ranges_are_inclusive_in_either_order() {
    let a = Scope::Verses { book: "Romans".into(), chapter: 8, from: 28, to: 30 };
    let b = Scope::Verses { book: "Romans".into(), chapter: 8, from: 30, to: 28 };
    let (a, b) = (context::build(data(), &a, &plain()).unwrap(), context::build(data(), &b, &plain()).unwrap());
    assert_eq!(a.label, "Romans 8:28–30");
    assert_eq!(a.verses, 3);
    assert_eq!(a.text, b.text);
}

#[test]
fn books_follow_canonical_order_and_get_a_range_label() {
    let law = Scope::Books {
        books: ["Deuteronomy", "Genesis", "Numbers", "Leviticus", "Exodus"].map(String::from).to_vec(),
    };
    let c = context::build(data(), &law, &plain()).unwrap();
    assert_eq!(c.label, "Genesis–Deuteronomy");
    let headings: Vec<&str> = c.text.lines().filter(|l| l.starts_with("# ")).collect();
    assert_eq!(headings, ["# Genesis", "# Exodus", "# Leviticus", "# Numbers", "# Deuteronomy"]);

    let some = Scope::Books { books: vec!["Esther".into(), "Ruth".into()] };
    assert_eq!(context::build(data(), &some, &plain()).unwrap().label, "Ruth, Esther");

    let nt_but_acts_and_revelation = Scope::Books {
        books: data().bible.books[39..65].iter().map(|b| b.name.clone()).filter(|n| n != "Acts").collect(),
    };
    let label = context::build(data(), &nt_but_acts_and_revelation, &plain()).unwrap().label;
    assert_eq!(label, "Matthew–John, Romans–Jude");
}

#[test]
fn whole_bible_has_every_verse_and_psalm_title() {
    let c = context::build(data(), &Scope::Bible, &plain()).unwrap();
    let titles = data()
        .bible
        .books
        .iter()
        .flat_map(|b| &b.chapters)
        .filter(|ch| ch.superscription.is_some())
        .count();
    assert_eq!(c.verses, 31102 + titles);
    assert_eq!(c.text.lines().filter(|l| l.starts_with("# ")).count(), 66);
    assert_eq!(c.text.lines().filter(|l| l.starts_with("## ")).count(), 1189);
    assert!(c.text.ends_with("21 The grace of our Lord Jesus Christ be with you all. Amen.\n"));
}

#[test]
fn original_words_follow_each_verse() {
    let options = ContextOptions { original: true };
    let c = context::build(data(), &verse("Genesis", 1, 1), &options).unwrap();
    let hebrew = c.text.lines().nth(3).unwrap();
    assert!(hebrew.starts_with("   Hebrew: "), "{}", hebrew);
    assert!(hebrew.contains("H430 God"), "{}", hebrew);
    assert_eq!(hebrew.matches(" | ").count(), 6, "7 words: {}", hebrew);

    let c = context::build(data(), &verse("John", 1, 1), &options).unwrap();
    assert!(c.text.lines().nth(3).unwrap().starts_with("   Greek: "));
}

#[test]
fn unknown_places_are_errors() {
    assert!(context::build(data(), &verse("John", 22, 1), &plain()).unwrap_err().contains("no chapter 22"));
    assert!(context::build(data(), &verse("John", 3, 99), &plain()).unwrap_err().contains("no verse 99"));
    let bad = Scope::Books { books: vec!["Genesis".into(), "Hezekiah".into()] };
    assert!(context::build(data(), &bad, &plain()).unwrap_err().contains("Hezekiah"));
    assert_eq!(context::build(data(), &Scope::None, &plain()).unwrap().text, "");
}

/// Writes sample contexts for measuring real tokenizers:
/// `cargo test --release -p kjv-core --test context -- --ignored dump`
#[test]
#[ignore]
fn dump() {
    let dir = std::env::var("CONTEXT_DUMP_DIR").unwrap_or_else(|_| std::env::temp_dir().display().to_string());
    let cases = [
        ("bible", Scope::Bible, false),
        ("nt", Scope::Books { books: data().bible.books[39..].iter().map(|b| b.name.clone()).collect() }, false),
        ("genesis-original", Scope::Book { book: "Genesis".into() }, true),
        ("john-original", Scope::Book { book: "John".into() }, true),
    ];
    for (name, scope, original) in cases {
        let c = context::build(data(), &scope, &ContextOptions { original }).unwrap();
        let path = Path::new(&dir).join(format!("{}.txt", name));
        std::fs::write(&path, &c.text).unwrap();
        println!("{} chars={} estimate={} -> {}", name, c.text.chars().count(), c.tokens, path.display());
    }
}
