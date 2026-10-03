//! Translations side by side, checked against the real data and library.

use std::path::Path;
use std::sync::OnceLock;

use kjv_core::bundle::DataBundle;
use kjv_core::parallel::{self, ParallelArgs, ParallelChapter};
use kjv_library::view::Part;
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

fn side_by_side(columns: &[&str], book: &str, chapter: u32) -> ParallelChapter {
    let args = ParallelArgs { columns: columns.iter().map(|c| c.to_string()).collect(), book: book.into(), chapter };
    parallel::chapter(data(), lib(), &args).unwrap()
}

/// A cell as "label text | label text", "↑" when given above, "" when empty
fn cell(p: &ParallelChapter, row: usize, column: usize) -> String {
    let c = &p.rows[row].cells[column];
    if c.above {
        return "↑".into();
    }
    c.verses
        .iter()
        .map(|v| {
            // Poetry lines run on with a space
            let text: String = v
                .parts
                .iter()
                .filter_map(|x| match x {
                    Part::Text { text, .. } => Some(text.as_str()),
                    Part::Break { .. } => Some(" "),
                    _ => None,
                })
                .collect();
            format!("{} {}", v.label, text.split_whitespace().collect::<Vec<_>>().join(" "))
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

#[test]
fn the_kjv_leads_and_others_follow_its_verses() {
    let p = side_by_side(&["kjv", "web", "dra"], "Psalms", 23);
    assert_eq!(p.heading, "Psalm 23");
    assert_eq!(p.columns.iter().map(|c| c.abbr.as_str()).collect::<Vec<_>>(), ["KJV", "WEB", "DRA"]);
    // The title, then six verses
    assert_eq!(p.rows.iter().map(|r| r.number.as_str()).collect::<Vec<_>>(), ["0", "1", "2", "3", "4", "5", "6"]);
    assert_eq!(cell(&p, 1, 0), "1 The LORD is my shepherd; I shall not want.");
    assert_eq!(cell(&p, 1, 1), "1 The LORD is my shepherd; I shall lack nothing.");
    // The Douay-Rheims' Psalm 22:1 holds the KJV's title and first verse: given once
    assert!(cell(&p, 0, 2).starts_with("22:1 A psalm for David. The Lord ruleth me"), "{}", cell(&p, 0, 2));
    assert_eq!(cell(&p, 1, 2), "↑");
    assert!(cell(&p, 2, 2).starts_with("22:2 He hath set me in a place of pasture"), "{}", cell(&p, 2, 2));
    assert_eq!((p.prev.as_ref().unwrap().chapter, p.next.as_ref().unwrap().chapter), (22, 24));
}

#[test]
fn another_translation_leads_in_its_own_numbering() {
    // The Douay-Rheims' Psalm 22 beside the KJV's 23
    let p = side_by_side(&["dra", "kjv"], "Psalms", 22);
    assert_eq!(p.heading, "Psalm 22");
    assert_eq!(cell(&p, 0, 1), "23 (title) A Psalm of David. | 23:1 The LORD is my shepherd; I shall not want.");
    // The WEB prints Romans 16:25-27 as 14:24-26
    let p = side_by_side(&["web", "kjv"], "Romans", 14);
    let last: Vec<String> = (p.rows.len() - 3..p.rows.len()).map(|r| cell(&p, r, 1)).collect();
    assert!(last[0].starts_with("16:25 Now to him that is of power to stablish you"), "{:?}", last);
    assert!(last[2].starts_with("16:27 To God only wise"), "{:?}", last);
    // Words of Jesus keep their style in every column
    let p = side_by_side(&["kjv", "web"], "John", 11);
    let red = |c: usize| p.rows[42].cells[c].verses[0].parts.iter().any(|x| matches!(x, Part::Text { styles, .. } if styles.iter().any(|s| s == "wj")));
    assert!(red(0) && red(1), "John 11:43, Lazarus, come forth");
}

#[test]
fn a_translation_without_the_book_is_empty_and_the_original_is_named() {
    let p = side_by_side(&["kjv", "jps", "original"], "John", 1);
    assert!(p.rows.iter().all(|r| r.cells[1].verses.is_empty() && !r.cells[1].above));
    assert_eq!(p.columns[2].abbr, "Greek");
    let greek = p.rows[0].cells[2].verses[0].original.as_ref().unwrap();
    assert_eq!(greek.lang, "grc");
    assert!(greek.words.iter().any(|w| w.strongs.as_deref() == Some("G3056")), "λόγος in John 1:1");
    // Daniel 2 turns from Hebrew to Aramaic at verse 4
    let dan = side_by_side(&["kjv", "original"], "Daniel", 2);
    assert_eq!(dan.columns[1].abbr, "Hebrew & Aramaic");
    // The original language follows the KJV's verses for any leading translation
    let dra = side_by_side(&["dra", "original"], "Psalms", 22);
    assert_eq!(dra.rows[0].cells[1].verses.len(), 2, "the KJV's title and verse 1");
    assert_eq!(dra.columns[1].abbr, "Hebrew");
}

#[test]
fn aramaic_is_named_where_the_old_testament_is_written_in_it() {
    use kjv_core::models::OriginalLanguage::{Aramaic, Hebrew};
    let lang = |book: &str, c: u32, v: u32| data().extended.get_interlinear(book, c, v).map(|iv| iv.language.clone());
    for (book, c, v, expected) in [
        ("Daniel", 2, 3, Hebrew),
        ("Daniel", 2, 5, Aramaic),
        ("Daniel", 7, 28, Aramaic),
        ("Daniel", 8, 1, Hebrew),
        ("Ezra", 4, 7, Hebrew),
        ("Ezra", 4, 8, Aramaic),
        ("Ezra", 6, 18, Aramaic),
        ("Ezra", 6, 19, Hebrew),
        ("Ezra", 7, 11, Hebrew),
        ("Ezra", 7, 12, Aramaic),
        ("Ezra", 7, 26, Aramaic),
        ("Ezra", 7, 27, Hebrew),
        ("Jeremiah", 10, 10, Hebrew),
        ("Jeremiah", 10, 11, Aramaic),
        ("Genesis", 31, 47, Hebrew),
    ] {
        assert_eq!(lang(book, c, v), Some(expected.clone()), "{book} {c}:{v}");
    }
    let count = data().extended.interlinear_ot.values().filter(|iv| iv.language == Aramaic).count();
    assert!((260..=275).contains(&count), "{count} Aramaic verses");
}

/// Every chapter of every translation leading, with the KJV, another translation, and
/// the original beside it: never an error, never a verse without words, and the
/// leading column always its own verse.
/// `cargo test --release -p kjv-core --test parallel -- --ignored --nocapture sweep`
#[test]
#[ignore]
fn sweep() {
    let started = std::time::Instant::now();
    let mut chapters = 0;
    let mut problems = Vec::new();
    for b in lib().bibles() {
        let other = if b.id == "web" { "dra" } else { "web" };
        for e in &b.books {
            let Some(k) = kjv_library::books::by_code(&e.code) else { continue };
            for &c in &e.numbers {
                let args = ParallelArgs { columns: vec![b.id.clone(), "kjv".into(), other.into(), "original".into()], book: k.name.into(), chapter: c };
                chapters += 1;
                match parallel::chapter(data(), lib(), &args) {
                    Err(err) => problems.push(format!("{} {} {}: {}", b.id, e.code, c, err)),
                    Ok(p) => {
                        for row in &p.rows {
                            for (i, cell) in row.cells.iter().enumerate() {
                                for v in &cell.verses {
                                    if v.parts.is_empty() && v.original.is_none() || v.label.is_empty() {
                                        problems.push(format!("{} {} {}:{} column {}: an empty verse {:?}", b.id, e.code, c, row.number, i, v.label));
                                    }
                                }
                            }
                            // The leading cell is its own verse, or empty where the translation
                            // numbers a verse it leaves out (Noyes' Matthew 17:21)
                            let lead = &row.cells[0];
                            let words = lib().verses(&b.id, &e.code).unwrap().iter().find(|v| v.chapter == c && v.number == row.number).map(|v| v.text.clone());
                            let empty = words.as_deref().is_some_and(|w| w.trim().is_empty());
                            // (A verse left out but given a footnote saying so is shown, as the
                            // reader shows it)
                            if row.number != "0" && (lead.above || lead.verses.len() > 1 || (lead.verses.is_empty() && !empty)) {
                                problems.push(format!("{} {} {}:{}: the leading cell isn't its verse ({:?})", b.id, e.code, c, row.number, words));
                            }
                        }
                    }
                }
            }
        }
    }
    println!("{} chapters in {:?}", chapters, started.elapsed());
    for p in problems.iter().take(40) {
        println!("{}", p);
    }
    assert!(problems.is_empty(), "{} problems", problems.len());
}
