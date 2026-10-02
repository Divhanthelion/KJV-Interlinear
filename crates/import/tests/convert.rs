//! Turning a module's entries into notes (`kjv_import::commentaries::convert_extraction`),
//! and writing a book's file, on small synthetic modules.

use kjv_import::commentaries::{Note, Options2, book_file, convert_extraction};
use kjv_import::markup::Dialect;
use kjv_sword::{Encoding, Entry, Extraction, Orphan, Repeat, Testament};

fn entry(book: &'static str, chapter: u32, verse: u32, to: (u32, u32), text: &str) -> Entry {
    Entry { book, chapter, verse, to_chapter: to.0, to_verse: to.1, text: text.into() }
}

fn single(book: &'static str, chapter: u32, verse: u32, text: &str) -> Entry {
    entry(book, chapter, verse, (chapter, verse), text)
}

fn extraction(entries: Vec<Entry>, repeats: Vec<Repeat>, orphans: Vec<Orphan>) -> Extraction {
    Extraction { entries, headings: vec![], repeats, orphans, reports: vec![], encoding: Encoding::Utf8, encoding_detected: false }
}

fn orphan(after: Option<usize>, before: Option<usize>, block: u32, offset: usize, text: &str) -> Orphan {
    Orphan { testament: Testament::Old, block, offset, len: text.len(), text: text.into(), after, before }
}

fn opts() -> Options2 {
    Options2 { dialect: Dialect::Osis, jud_is_judges: false, trim_cross_chapter: false, relative_refs: false }
}

fn p(text: &str) -> String {
    format!(r#"<div type="x-p">{text}</div>"#)
}

#[test]
fn each_entry_becomes_a_note_with_its_range() {
    let ex = extraction(
        vec![
            single("GEN", 0, 0, &p("Book intro")),
            single("GEN", 1, 0, &p("Chapter intro")),
            entry("GEN", 1, 1, (1, 3), &p("Verses one to three")),
            single("GEN", 1, 4, &p("Four")),
            single("EXO", 2, 5, &p("Exodus")),
        ],
        vec![],
        vec![],
    );
    let (notes, report) = convert_extraction("t", &ex, &opts()).unwrap();
    let got: Vec<_> = notes.iter().map(|n| (n.book, n.from, n.to, n.body.as_str())).collect();
    assert_eq!(
        got,
        vec![
            ("GEN", (0, 0), (0, 0), "<p>Book intro</p>"),
            ("GEN", (1, 0), (1, 0), "<p>Chapter intro</p>"),
            ("GEN", (1, 1), (1, 3), "<p>Verses one to three</p>"),
            ("GEN", (1, 4), (1, 4), "<p>Four</p>"),
            ("EXO", (2, 5), (2, 5), "<p>Exodus</p>"),
        ]
    );
    assert_eq!((report.notes, report.entries, report.empty.len()), (5, 5, 0));
    assert_eq!(report.per_book["GEN"].0, 4);
    assert_eq!(report.per_book["EXO"].0, 1);
    // characters: Book intro (9) + Chapter intro (12) + ... without spaces
    let expected: usize = ["Bookintro", "Chapterintro", "Versesonetothree", "Four", "Exodus"].iter().map(|s| s.chars().count()).sum();
    assert_eq!(report.chars as usize, expected);
    assert_eq!(report.repeats_dropped, 0);
    assert_eq!(report.orphans_merged, 0);
}

#[test]
fn an_entry_with_only_structure_makes_no_note() {
    let ex = extraction(
        vec![
            single("GEN", 1, 0, r#"<chapter n="1" osisID="Gen.1" sID="Gen.1"/> "#),
            single("GEN", 1, 1, &p("Text")),
        ],
        vec![],
        vec![],
    );
    let (notes, report) = convert_extraction("t", &ex, &opts()).unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].from, (1, 1));
    assert_eq!(report.empty, vec![("GEN", (1, 0), (1, 0))]);
    assert_eq!(report.entries, 2);
}

#[test]
fn repeats_are_dropped_when_identical_to_their_original() {
    let ex = extraction(
        vec![
            single("GEN", 50, 26, &p("Last note of Genesis")),
            single("EXO", 1, 0, &p("Last note of Genesis")),
            single("EXO", 1, 1, &p("Real")),
        ],
        vec![Repeat { entry: 1, first: 0 }],
        vec![],
    );
    let (notes, report) = convert_extraction("t", &ex, &opts()).unwrap();
    assert_eq!(notes.iter().map(|n| (n.book, n.from)).collect::<Vec<_>>(), vec![("GEN", (50, 26)), ("EXO", (1, 1))]);
    assert_eq!(report.repeats_dropped, 1);
    assert_eq!(report.entries, 2);
}

#[test]
fn a_repeat_that_differs_from_its_original_is_an_error() {
    let ex = extraction(
        vec![single("GEN", 50, 26, &p("One")), single("EXO", 1, 0, &p("Two"))],
        vec![Repeat { entry: 1, first: 0 }],
        vec![],
    );
    let e = convert_extraction("t", &ex, &opts()).unwrap_err();
    assert!(e.contains("differs from its original"), "{e}");
}

#[test]
fn a_repeat_listed_twice_or_of_a_repeat_is_an_error() {
    let entries = || vec![single("GEN", 1, 1, &p("A")), single("GEN", 1, 2, &p("A")), single("GEN", 1, 3, &p("A"))];
    let twice = extraction(entries(), vec![Repeat { entry: 1, first: 0 }, Repeat { entry: 1, first: 0 }], vec![]);
    assert!(convert_extraction("t", &twice, &opts()).unwrap_err().contains("listed as a repeat twice"));
    let chain = extraction(entries(), vec![Repeat { entry: 1, first: 0 }, Repeat { entry: 2, first: 1 }], vec![]);
    assert!(convert_extraction("t", &chain, &opts()).unwrap_err().contains("itself a repeat"));
}

#[test]
fn an_orphan_is_appended_to_the_entry_it_follows_and_every_byte_lands_once() {
    let ex = extraction(
        vec![single("GEN", 1, 1, &p("First")), single("GEN", 1, 3, &p("Third")), single("GEN", 1, 5, &p("Fifth"))],
        vec![],
        vec![
            orphan(Some(0), Some(1), 0, 100, &p("Second (orphan)")),
            orphan(Some(0), Some(1), 0, 200, &p("Second again (orphan)")),
            orphan(Some(2), None, 1, 0, &p("After the last")),
        ],
    );
    let (notes, report) = convert_extraction("t", &ex, &opts()).unwrap();
    let bodies: Vec<&str> = notes.iter().map(|n| n.body.as_str()).collect();
    assert_eq!(
        bodies,
        vec![
            "<p>First</p><p>Second (orphan)</p><p>Second again (orphan)</p>",
            "<p>Third</p>",
            "<p>Fifth</p><p>After the last</p>",
        ]
    );
    // the host's range is the host's
    assert_eq!(notes[0].to, (1, 1));
    assert_eq!(report.orphans_merged, 3);
    let orphan_bytes: usize = ex.orphans.iter().map(|o| o.text.len()).sum();
    assert_eq!(report.orphan_bytes, orphan_bytes);
    let entry_bytes: usize = ex.entries.iter().map(|e| e.text.len()).sum();
    assert_eq!(report.source_bytes, entry_bytes + orphan_bytes);
}

#[test]
fn orphans_are_joined_in_the_modules_own_order_whatever_order_they_are_listed() {
    let ex = extraction(
        vec![single("GEN", 1, 1, &p("A")), single("GEN", 1, 9, &p("Z"))],
        vec![],
        vec![orphan(Some(0), Some(1), 0, 300, &p("c")), orphan(Some(0), Some(1), 0, 100, &p("a")), orphan(Some(0), Some(1), 0, 200, &p("b"))],
    );
    let (notes, _) = convert_extraction("t", &ex, &opts()).unwrap();
    assert_eq!(notes[0].body, "<p>A</p><p>a</p><p>b</p><p>c</p>");
}

#[test]
fn an_orphan_that_follows_a_structure_only_entry_gives_it_a_note() {
    let ex = extraction(
        vec![single("GEN", 1, 0, r#"<chapter n="1" osisID="Gen.1" sID="Gen.1"/>"#), single("GEN", 1, 1, &p("One"))],
        vec![],
        vec![orphan(Some(0), Some(1), 0, 5, &p("Held by no verse"))],
    );
    let (notes, report) = convert_extraction("t", &ex, &opts()).unwrap();
    assert_eq!(notes.len(), 2);
    assert_eq!((notes[0].from, notes[0].body.as_str()), ((1, 0), "<p>Held by no verse</p>"));
    assert!(report.empty.is_empty());
}

#[test]
fn an_orphan_with_nowhere_to_go_is_an_error() {
    // follows nothing
    let ex = extraction(vec![single("GEN", 1, 1, &p("One"))], vec![], vec![orphan(None, Some(0), 0, 0, &p("Lost"))]);
    let e = convert_extraction("t", &ex, &opts()).unwrap_err();
    assert!(e.contains("follows no entry") && e.contains("Lost"), "{e}");
    // follows an entry of another book than the one after it
    let ex = extraction(
        vec![single("GEN", 50, 26, &p("End")), single("EXO", 1, 1, &p("Start"))],
        vec![],
        vec![orphan(Some(0), Some(1), 0, 0, &p("Between books"))],
    );
    let e = convert_extraction("t", &ex, &opts()).unwrap_err();
    assert!(e.contains("is followed by an entry of EXO"), "{e}");
    // follows a repeat
    let ex = extraction(
        vec![single("GEN", 1, 1, &p("A")), single("EXO", 1, 0, &p("A"))],
        vec![Repeat { entry: 1, first: 0 }],
        vec![orphan(Some(1), None, 0, 0, &p("x"))],
    );
    let e = convert_extraction("t", &ex, &opts()).unwrap_err();
    assert!(e.contains("follows a repeated entry"), "{e}");
}

#[test]
fn a_cross_chapter_range_is_trimmed_to_its_first_chapter_when_asked() {
    // Genesis 12 has 20 verses
    let entries = || vec![entry("GEN", 12, 20, (13, 2), &p("Pharaoh commanded")), single("GEN", 13, 3, &p("Abram was rich"))];
    let (notes, report) = convert_extraction("t", &extraction(entries(), vec![], vec![]), &opts()).unwrap();
    assert_eq!(notes[0].to, (13, 2));
    assert!(report.trimmed.is_empty());
    let trim = Options2 { trim_cross_chapter: true, ..opts() };
    let (notes, report) = convert_extraction("t", &extraction(entries(), vec![], vec![]), &trim).unwrap();
    assert_eq!((notes[0].from, notes[0].to), ((12, 20), (12, 20)));
    assert_eq!((notes[1].from, notes[1].to), ((13, 3), (13, 3)));
    assert_eq!(report.trimmed.len(), 1);
    let t = &report.trimmed[0];
    assert_eq!((t.book, t.from, t.was_to, t.to), ("GEN", (12, 20), (13, 2), (12, 20)));
    // from a chapter's start to the end of that chapter
    let ex = extraction(vec![entry("PSA", 22, 28, (24, 0), &p("Psalm"))], vec![], vec![]);
    let (notes, report) = convert_extraction("t", &ex, &trim).unwrap();
    assert_eq!(notes[0].to, (22, 31));
    assert_eq!(report.trimmed[0].was_to, (24, 0));
    // a range inside one chapter is left alone
    let ex = extraction(vec![entry("GEN", 1, 3, (1, 5), &p("Three to five"))], vec![], vec![]);
    let (notes, report) = convert_extraction("t", &ex, &trim).unwrap();
    assert_eq!(notes[0].to, (1, 5));
    assert!(report.trimmed.is_empty());
}

#[test]
fn notes_must_come_in_order_and_books_together() {
    let ex = extraction(vec![single("GEN", 1, 2, &p("b")), single("GEN", 1, 1, &p("a"))], vec![], vec![]);
    assert!(convert_extraction("t", &ex, &opts()).unwrap_err().contains("out of order"));
    let ex = extraction(vec![single("GEN", 1, 1, &p("a")), single("EXO", 1, 1, &p("b")), single("GEN", 1, 2, &p("c"))], vec![], vec![]);
    assert!(convert_extraction("t", &ex, &opts()).unwrap_err().contains("not contiguous"));
    // the same verse twice
    let ex = extraction(vec![single("GEN", 1, 1, &p("a")), single("GEN", 1, 1, &p("b"))], vec![], vec![]);
    assert!(convert_extraction("t", &ex, &opts()).unwrap_err().contains("out of order"));
}

#[test]
fn unknown_markup_names_its_note() {
    let ex = extraction(vec![single("GEN", 1, 1, &p("ok")), single("GEN", 1, 2, "<blink>x</blink>")], vec![], vec![]);
    let e = convert_extraction("mhc", &ex, &opts()).unwrap_err();
    assert!(e.starts_with("mhc GEN 1:2: "), "{e}");
    assert!(e.contains("unknown element <blink>"), "{e}");
}

#[test]
fn relative_references_use_each_notes_own_verse() {
    let thml = Options2 { dialect: Dialect::Thml, relative_refs: true, ..opts() };
    let ex = extraction(
        vec![single("GEN", 1, 1, "<scripRef>5; 8:2</scripRef>"), single("EXO", 20, 4, "<scripRef>5; 8:2</scripRef>")],
        vec![],
        vec![],
    );
    let (notes, report) = convert_extraction("tsk", &ex, &thml).unwrap();
    assert_eq!(notes[0].body, r#"<p><ref to="GEN.1.5 GEN.8.2">5; 8:2</ref></p>"#);
    assert_eq!(notes[1].body, r#"<p><ref to="EXO.20.5 EXO.8.2">5; 8:2</ref></p>"#);
    assert_eq!(report.stats.refs_resolved, 2);
    // a book introduction has no chapter: a bare number cannot be placed, a chapter and verse can
    let ex = extraction(vec![single("GEN", 0, 0, "<scripRef>5</scripRef> <scripRef>8:2</scripRef>")], vec![], vec![]);
    let (notes, report) = convert_extraction("tsk", &ex, &thml).unwrap();
    assert_eq!(notes[0].body, r#"<p><ref>5</ref> <ref to="GEN.8.2">8:2</ref></p>"#);
    assert_eq!(report.stats.unparsed, ["5"]);
}

// ---------------------------------------------------------------------------------------
// The book file
// ---------------------------------------------------------------------------------------

fn note(from: (u32, u32), to: (u32, u32), body: &str) -> Note {
    Note { book: "GEN", from, to, body: body.into() }
}

#[test]
fn a_book_file_has_one_json_object_per_line_with_a_fixed_key_order() {
    let notes = [note((0, 0), (0, 0), "<p>Book</p>"), note((1, 0), (1, 0), "<p>Chapter</p>"), note((3, 16), (3, 18), "<p>Verses</p>")];
    let refs: Vec<&Note> = notes.iter().collect();
    let text = book_file(&refs);
    assert_eq!(
        text,
        "{\"from\":\"0:0\",\"body\":\"<p>Book</p>\"}\n{\"from\":\"1:0\",\"body\":\"<p>Chapter</p>\"}\n{\"from\":\"3:16\",\"to\":\"3:18\",\"body\":\"<p>Verses</p>\"}\n"
    );
    assert!(!text.contains('\r'));
}

#[test]
fn a_book_file_escapes_exactly_what_json_needs_and_every_control_character() {
    let body = "<p>quote \" backslash \\ newline \n tab \t cr \r nul \u{0} bell \u{7} del \u{7f} c1 \u{89}\u{9f} ls \u{2028}\u{2029} replacement \u{fffd} é דבר 𐤀 &amp; &lt;</p>";
    let text = book_file(&[&note((1, 1), (1, 1), body)]);
    // one line only
    assert_eq!(text.matches('\n').count(), 1);
    assert!(text.ends_with("}\n"));
    for c in ['\r', '\t', '\u{0}', '\u{7}', '\u{7f}', '\u{89}', '\u{9f}', '\u{2028}', '\u{2029}'] {
        assert!(!text.contains(c), "{:?} must be escaped", c);
    }
    // the odd characters show as escapes; ordinary and non-ASCII text is as it is
    for want in ["\\\"", "\\\\", "\\n", "\\t", "\\r", "\\u0000", "\\u0007", "\\u007f", "\\u0089", "\\u009f", "\\u2028", "\\u2029"] {
        assert!(text.contains(want), "missing {want}");
    }
    assert!(text.contains('\u{fffd}') && text.contains('é') && text.contains("דבר") && text.contains('𐤀'));
    // and it reads back to the same body
    let v: serde_json::Value = serde_json::from_str(text.trim_end_matches('\n')).unwrap();
    assert_eq!(v["body"].as_str().unwrap(), body);
    assert_eq!(v["from"], "1:1");
    assert!(v.get("to").is_none());
}
