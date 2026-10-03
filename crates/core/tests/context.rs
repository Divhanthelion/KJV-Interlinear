//! Context for the study assistant, checked against the real bundled data and library.

use std::path::Path;
use std::sync::OnceLock;

use kjv_core::bundle::DataBundle;
use kjv_core::context::{self, PassageSpec, Spec};
use kjv_library::books::{BOOKS, Section};
use kjv_library::{Library, library::build};

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn data() -> &'static DataBundle {
    static DATA: OnceLock<DataBundle> = OnceLock::new();
    DATA.get_or_init(|| DataBundle::from_sources(root()).expect("bundle builds"))
}

fn lib() -> &'static Library {
    static LIB: OnceLock<Library> = OnceLock::new();
    LIB.get_or_init(|| {
        let (bytes, _) = build::archive(root(), &|b| zstd::encode_all(b, 1).unwrap()).unwrap();
        Library::open(bytes).unwrap()
    })
}

fn passage(bible: &str, refs: &str) -> PassageSpec {
    PassageSpec { bible: bible.into(), refs: refs.into(), translations: None, commentaries: None, crossrefs: None, original: None }
}

fn spec(passages: Vec<PassageSpec>) -> Spec {
    Spec { passages, translations: vec!["kjv".into()], ..Default::default() }
}

fn build(s: &Spec) -> context::Built {
    context::build(data(), lib(), s, None).unwrap()
}

fn codes(section: Section) -> Vec<&'static str> {
    BOOKS.iter().filter(|b| b.section == section).map(|b| b.code).collect()
}

/// Every book of the Protestant canon, as one passage's refs
fn canon() -> String {
    [codes(Section::Old), codes(Section::New)].concat().join(" ")
}

/// The lines of the first element starting `open`, up to its end tag
fn inside<'a>(text: &'a str, open: &str, close: &str) -> &'a str {
    let at = text.find(open).unwrap_or_else(|| panic!("no {open} in {text}"));
    let rest = &text[at + open.len()..];
    &rest[..rest.find(close).unwrap()]
}

#[test]
fn one_verse() {
    let c = build(&spec(vec![passage("kjv", "JHN.3.16")]));
    assert_eq!(c.label, "John 3:16");
    assert_eq!(
        c.text,
        "<context>\n<passage ref=\"John 3:16\">\n\
         <bible translation=\"King James Version\" abbr=\"KJV\" year=\"1611 (1769 text)\" ref=\"John 3:16\">\n\
         ## John 3\n16 For God so loved the world, that he gave his only begotten Son, \
         that whosoever believeth in him should not perish, but have everlasting life.\n\
         </bible>\n</passage>\n</context>\n"
    );
    assert_eq!(c.verses, 1);
    assert_eq!(c.passages.len(), 1);
    assert_eq!(c.passages[0].parts.len(), 1);
    // Nothing attached: no text at all
    let none = build(&spec(vec![]));
    assert_eq!((none.text.as_str(), none.label.as_str()), ("", ""));
    assert!(context::instructions(lib(), &none).contains("No passage is attached"));
    assert!(context::instructions(lib(), &c).contains("The reader has attached John 3:16 below, inside <context>."));
}

#[test]
fn psalm_titles_come_first_and_are_marked() {
    let c = build(&spec(vec![passage("kjv", "PSA.51")]));
    assert_eq!(c.label, "Psalm 51");
    let lines: Vec<&str> = c.text.lines().collect();
    assert_eq!(lines[3], "## Psalm 51");
    assert!(lines[4].starts_with("(title) To the chief Musician"), "{}", lines[4]);
    assert!(lines[5].starts_with("1 Have mercy upon me, O God"), "{}", lines[5]);
    // 19 verses plus the title
    assert_eq!(c.verses, 20);
    // A verse range leaves the title out
    let some = build(&spec(vec![passage("kjv", "PSA.51.1-PSA.51.2")]));
    assert_eq!(some.label, "Psalm 51:1–2");
    assert!(!some.text.contains("(title)"));
}

#[test]
fn typed_references_become_passages() {
    let parse = |text: &str, bible: &str| context::parse(data(), lib(), text, bible);
    let labels = |text: &str| parse(text, "kjv").unwrap().into_iter().map(|p| p.label).collect::<Vec<_>>();
    assert_eq!(labels("Rom 8:28-30"), ["Romans 8:28–30"]);
    assert_eq!(labels("Gen 1:1, 3, 5-7"), ["Genesis 1:1, 3, 5–7"]);
    assert_eq!(labels("Matt 5-7"), ["Matthew 5–7"]);
    assert_eq!(labels("Ps 1-3"), ["Psalms 1–3"]);
    assert_eq!(labels("Jude"), ["Jude"]);
    assert_eq!(labels("Ps 51:1-3; 52"), ["Psalm 51:1–3", "Psalm 52"]);
    assert_eq!(labels("Rom 8:28-9:5"), ["Romans 8:28–9:5"]);
    assert_eq!(labels("Luke 2:14; Rom 5:1-2; Micah 6"), ["Luke 2:14", "Romans 5:1–2", "Micah 6"]);
    let p = parse("Luke 2:14", "web").unwrap();
    assert_eq!((p[0].bible.as_str(), p[0].refs.as_str()), ("web", "LUK.2.14"));
    // In the numbering of the translation being read, which says so where it differs
    let dra = parse("Psalm 22", "dra").unwrap();
    assert_eq!((dra[0].bible.as_str(), dra[0].label.as_str()), ("dra", "Psalm 22 (DRA numbering)"));
    // A book the translation being read hasn't: the KJV's (it has the Apocrypha)
    let tobit = parse("Tobit 1", "web").unwrap();
    assert_eq!((tobit[0].bible.as_str(), tobit[0].label.as_str()), ("kjv", "Tobit 1"));
    assert_eq!(parse("John 3:99", "web").unwrap_err(), "WEB has no John 3:99");
    assert_eq!(parse("Hezekiah 1:1", "kjv").unwrap_err(), "No book called “Hezekiah”");
    assert!(parse("", "kjv").is_err());
}

#[test]
fn labels_for_books() {
    let label = |refs: &str| build(&spec(vec![passage("kjv", refs)])).label;
    assert_eq!(label("GEN EXO LEV NUM DEU"), "Genesis–Deuteronomy");
    assert_eq!(label("RUT EST"), "Ruth, Esther");
    assert_eq!(label(&canon()), "The whole Bible");
    let nt = codes(Section::New);
    assert_eq!(label(&nt.join(" ")), "The New Testament");
    assert_eq!(label(&codes(Section::Old).join(" ")), "The Old Testament");
    let some: Vec<&str> = nt.iter().copied().filter(|c| *c != "ACT" && *c != "REV").collect();
    assert_eq!(label(&some.join(" ")), "Matthew–John, Romans–Jude");
}

#[test]
fn whole_bible_has_every_verse_and_psalm_title() {
    let c = build(&spec(vec![passage("kjv", &canon())]));
    let titles = data().bible.books.iter().flat_map(|b| &b.chapters).filter(|ch| ch.superscription.is_some()).count();
    assert_eq!(c.verses, 31102 + titles);
    assert_eq!(c.text.lines().filter(|l| l.starts_with("## ")).count(), 1189);
    assert!(c.text.ends_with("21 The grace of our Lord Jesus Christ be with you all. Amen.\n</bible>\n</passage>\n</context>\n"));
    // Every verse reads exactly as the app's KJV has it, in order
    let mut at = 0;
    for b in &data().bible.books {
        for ch in &b.chapters {
            for v in ch.superscription.iter().chain(&ch.verses) {
                let line = if v.verse_number == 0 { format!("\n(title) {}\n", v.text) } else { format!("\n{} {}\n", v.verse_number, v.text) };
                let found = c.text[at..].find(&line).unwrap_or_else(|| panic!("{} {}:{}", b.name, ch.number, v.verse_number));
                at += found + 1;
            }
        }
    }
    // The size stops counting at the cap, and says so
    let mut big = spec(vec![passage("kjv", &canon())]);
    big.translations = vec!["kjv".into(), "web".into(), "bsb".into()];
    big.commentaries = vec!["mhc".into()];
    let size = context::size(data(), lib(), &big).unwrap();
    assert!(size.capped);
    assert!(size.tokens * 38 / 10 >= context::SIZE_CAP / 2, "{}", size.tokens);
}

#[test]
fn other_translations_are_found_verse_by_verse() {
    // The Douay-Rheims' Psalm 22 is the KJV's 23
    let mut s = spec(vec![passage("dra", "PSA.22")]);
    s.translations = vec!["dra".into(), "kjv".into()];
    let c = build(&s);
    assert_eq!(c.label, "Psalm 22 (DRA numbering)");
    assert!(c.text.starts_with("<context>\n<passage ref=\"Psalm 22 (DRA numbering)\" numbering=\"DRA\">\n"));
    assert!(c.text.contains("abbr=\"DRA\" year=\"1899 (Challoner revision)\" ref=\"Psalm 22\">\n## Psalm 22\n1 A psalm for David. The Lord ruleth me"));
    assert!(c.text.contains(
        "abbr=\"KJV\" year=\"1611 (1769 text)\" ref=\"Psalm 23\">\n## Psalm 23\n(title) A Psalm of David.\n1 The LORD is my shepherd; I shall not want.\n"
    ));

    // The WEB prints Romans 16:25-27 as 14:24-26
    let mut s = spec(vec![passage("web", "ROM.14")]);
    s.translations = vec!["web".into(), "kjv".into()];
    let c = build(&s);
    assert_eq!(c.label, "Romans 14 (WEB numbering)");
    assert!(c.text.contains("ref=\"Romans 14; 16:25–27\">\n## Romans 14\n1 Him that is weak"));
    assert!(c.text.contains("\n## Romans 16\n25 Now to him that is of power to stablish you"));

    // A verse a translation leaves out, and a translation without the book
    let mut s = spec(vec![passage("kjv", "MAT.17.20-MAT.17.22")]);
    s.translations = vec!["kjv".into(), "bsb".into(), "jps".into()];
    let c = build(&s);
    assert!(c.text.contains("\n22 When they gathered together in Galilee"));
    assert!(c.text.contains("\n(Not in this translation: Matthew 17:21 in the KJV.)\n</bible>"));
    assert!(c.text.contains("abbr=\"JPS\" year=\"1917\">\n(Not in this translation.)\n</bible>"));
    let parts = &c.passages[0].parts;
    assert_eq!(parts.iter().map(|p| (p.id.as_str(), p.empty)).collect::<Vec<_>>(), [("kjv", false), ("bsb", false), ("jps", true)]);
    let instructions = context::instructions(lib(), &c);
    assert!(
        instructions.contains("King James Version (KJV, 1611 (1769 text)), Berean Standard Bible (BSB, 2023), and JPS Tanakh (JPS, 1917)"),
        "{}",
        instructions
    );
}

#[test]
fn commentaries_give_each_note_once_with_introductions_for_whole_chapters() {
    let mut s = spec(vec![passage("kjv", "LUK.2.10"), passage("kjv", "LUK.2.14")]);
    s.commentaries = vec!["mhc".into(), "jfb".into()];
    let c = build(&s);
    assert_eq!(c.text.matches("<note on=\"Luke 2:8-20\">").count(), 2, "on both passages");
    assert!(c.text.contains("<note on=\"Luke 2:8-20\">(Given above, with Luke 2:10.)</note>"));
    assert!(c.text.contains("<commentary name=\"Matthew Henry's Complete Commentary\" author=\"Matthew Henry\" year=\"1706–1721\">"));
    let instructions = context::instructions(lib(), &c);
    assert!(instructions.contains("Matthew Henry's Complete Commentary (Matthew Henry, 1706–1721;"), "{}", instructions);

    // Introductions come with whole chapters: the book's with its first
    let notes_on = |refs: &str, id: &str| {
        let mut s = spec(vec![passage("kjv", refs)]);
        s.commentaries = vec![id.into()];
        build(&s).text.lines().filter(|l| l.starts_with("<note on=")).map(String::from).collect::<Vec<_>>()
    };
    let rom1 = notes_on("ROM.1", "tyndale");
    assert_eq!(rom1.iter().filter(|l| l.starts_with("<note on=\"Romans (introduction)\">")).count(), 2, "{:?}", rom1);
    assert!(notes_on("ROM.1.1-ROM.1.7", "tyndale").iter().all(|l| !l.contains("introduction")));
    assert!(notes_on("ROM.2", "mhc").first().is_some_and(|l| l.starts_with("<note on=\"Romans 2 (introduction)\">")));
    assert!(notes_on("ROM.2.1-ROM.2.3", "mhc").iter().all(|l| !l.contains("introduction")));
    // None on the passage: said so
    let mut s = spec(vec![passage("kjv", "GEN.1.1")]);
    s.commentaries = vec!["chrysostom".into()];
    let c = build(&s);
    assert!(c.text.contains("(No notes on this passage.)\n</commentary>"));
    assert!(c.passages[0].parts[1].empty);
}

#[test]
fn cross_references_with_and_without_their_words() {
    let mut s = spec(vec![passage("kjv", "LUK.2.14")]);
    s.crossrefs = vec!["openbible".into(), "tsk".into()];
    s.crossref_limit = 3;
    s.crossref_text = true;
    let c = build(&s);
    let open = inside(&c.text, "<crossrefs name=\"OpenBible.info Cross References\" numbering=\"KJV\">\n", "</crossrefs>");
    let lines: Vec<&str> = open.lines().collect();
    assert_eq!(lines[0], "Luke 2:14");
    assert_eq!(lines.len(), 4, "{:?}", lines);
    assert!(lines[1].starts_with("- Romans 5:1: Therefore being justified by faith"), "{}", lines[1]);
    // The Treasury: up to the limit for each word it gives places for
    let tsk = inside(&c.text, "<crossrefs name=\"Treasury of Scripture Knowledge\" numbering=\"KJV\">\n", "</crossrefs>");
    assert!(tsk.starts_with("Luke 2:14\nGlory.\n- Luke 19:38: Saying, Blessed be the King"), "{}", tsk);
    assert!(tsk.lines().filter(|l| !l.starts_with("- ") && *l != "Luke 2:14").count() >= 3, "several words: {}", tsk);
    assert!(context::instructions(lib(), &c).contains("places elsewhere in Scripture that bear on it, with their words."));

    s.crossref_text = false;
    let c = build(&s);
    assert!(c.text.contains("\nLuke 2:14\nRomans 5:1; Luke 19:38; Luke 1:79\n"), "{}", c.text);
    assert!(c.text.contains("\nGlory. Luke 19:38; Psalm 69:34; Psalm 69:35\n"), "{}", c.text);
}

#[test]
fn original_words_follow_each_kjv_verse_or_stand_alone() {
    let mut s = spec(vec![passage("kjv", "GEN.1.1")]);
    s.original = true;
    let c = build(&s);
    let hebrew = c.text.lines().find(|l| l.starts_with("   Hebrew: ")).unwrap();
    assert!(hebrew.contains("H430 God"), "{}", hebrew);
    assert_eq!(hebrew.matches(" | ").count(), 6, "7 words: {}", hebrew);
    assert!(context::instructions(lib(), &c).contains("Under each KJV verse"));

    // Without the KJV, they come in a block of their own, in its numbering
    let mut s = spec(vec![passage("dra", "PSA.22.1")]);
    s.translations = vec!["dra".into()];
    s.original = true;
    let c = build(&s);
    assert!(c.text.contains("<original numbering=\"KJV\" ref=\"Psalm 23:title–1\">\n## Psalm 23\n(title) Hebrew: "), "{}", c.text);
    assert!(context::instructions(lib(), &c).contains("Inside <original>"));
}

#[test]
fn full_definitions_follow_the_passages_once_each() {
    let mut words = spec(vec![passage("kjv", "JHN.1")]);
    words.original = true;
    let mut full = words.clone();
    full.definitions = true;
    let (words, full) = (build(&words), build(&full));
    let base = words.text.strip_suffix("</context>\n").unwrap();
    assert!(full.text.starts_with(base), "the passages come first, unchanged");
    let defs = &full.text[base.len()..];
    assert!(defs.starts_with("<definitions>\n"), "{}", &defs[..80]);
    // λόγος (John 1:1) once, with its whole entry
    assert_eq!(defs.lines().filter(|l| l.starts_with("## G3056 ")).count(), 1);
    let heading = defs.lines().find(|l| l.starts_with("## G3056 ")).unwrap_or("");
    assert!(heading.ends_with(" · logos · G:N-M · word"), "{}", heading);
    assert!(defs.contains("the Divine Word or Logos: Jhn.1:1, 14"));
    let used: std::collections::HashSet<&str> = words
        .text
        .split([' ', '|', '\n'])
        .filter(|w| w.len() > 1 && w.starts_with('G') && w[1..].chars().all(|c| c.is_ascii_digit()))
        .collect();
    let headed: std::collections::HashSet<&str> =
        defs.lines().filter_map(|l| l.strip_prefix("## ")).filter_map(|l| l.split(' ').next()).collect();
    assert_eq!(headed, used, "one entry for each number");
    assert_eq!(defs.lines().filter(|l| l.starts_with("## ")).count(), used.len());
    assert!(context::instructions(lib(), &full).contains("inside <definitions>"));
    // Definitions need the original-language words to refer to
    let mut alone = spec(vec![passage("kjv", "JHN.1")]);
    alone.definitions = true;
    assert!(!build(&alone).text.contains("<definitions>"));
}

#[test]
fn per_passage_choices_override_the_rest() {
    let mut s = spec(vec![passage("kjv", "LUK.2.14"), passage("kjv", "ROM.5.1")]);
    s.commentaries = vec!["jfb".into()];
    s.passages[1].translations = Some(vec!["web".into()]);
    s.passages[1].commentaries = Some(vec![]);
    let c = build(&s);
    assert!(c.text.split("<passage ref=\"Romans 5:1\">").next().unwrap().contains("<commentary"));
    let second = c.text.split("<passage ref=\"Romans 5:1\">").nth(1).unwrap();
    assert!(second.contains("abbr=\"WEB\""));
    assert!(!second.contains("abbr=\"KJV\""));
    assert!(!second.contains("<commentary"));
    assert_eq!(c.passages[1].parts.len(), 1);
    // Unknown sources are errors; a passage that can't be given is reported, not fatal
    let mut bad = spec(vec![passage("kjv", "JHN.3.16")]);
    bad.commentaries = vec!["haydock".into()];
    assert!(context::build(data(), lib(), &bad, None).unwrap_err().contains("haydock"));
    let missing = build(&spec(vec![passage("web", "TOB.1"), passage("kjv", "JHN.3.16")]));
    assert_eq!(missing.passages[0].problem.as_deref(), Some("WEB has no Tobit"));
    assert_eq!(missing.passages[1].label, "John 3:16");
    assert_eq!(missing.label, "Tobit 1; John 3:16");
}

/// Writes sample contexts for measuring real tokenizers:
/// `cargo test --release -p kjv-core --test context -- --ignored dump`
#[test]
#[ignore]
fn dump() {
    let dir = std::env::var("CONTEXT_DUMP_DIR").unwrap_or_else(|_| std::env::temp_dir().display().to_string());
    let cases = [
        ("bible", canon(), false),
        ("nt", codes(Section::New).join(" "), false),
        ("genesis-original", "GEN".to_string(), true),
        ("john-original", "JHN".to_string(), true),
    ];
    for (name, refs, original) in cases {
        let mut s = spec(vec![passage("kjv", &refs)]);
        s.original = original;
        let c = build(&s);
        let path = Path::new(&dir).join(format!("{}.txt", name));
        std::fs::write(&path, &c.text).unwrap();
        println!("{} chars={} estimate={} -> {}", name, c.text.chars().count(), c.tokens, path.display());
    }
}
