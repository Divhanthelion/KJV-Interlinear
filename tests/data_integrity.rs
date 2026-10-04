//! Checks the bundled KJV, STEP Bible, and red-letter data exactly as the app loads it.

use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;

use kjv_interlinear::models::{Bible, ExtendedBible, Testament, VerseRef};
use kjv_interlinear::original_languages::load_extended_bible;
use kjv_interlinear::red_letter::{RedLetterIndex, RedLetterSpec, red_letter_segments};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn bible() -> &'static Bible {
    static BIBLE: OnceLock<Bible> = OnceLock::new();
    BIBLE.get_or_init(|| {
        Bible::from_directories(&root().join("old_testament"), &root().join("new_testament"))
            .expect("KJV text loads")
    })
}

fn extended() -> &'static ExtendedBible {
    static EXT: OnceLock<ExtendedBible> = OnceLock::new();
    EXT.get_or_init(|| load_extended_bible(&root().join("data")).expect("STEP data loads"))
}

/// Every verse, plus Psalm titles as verse 0.
fn kjv_refs() -> Vec<VerseRef> {
    let mut refs = Vec::new();
    for book in &bible().books {
        for chapter in &book.chapters {
            if chapter.superscription.is_some() {
                refs.push(VerseRef::new(&book.name, chapter.number, 0));
            }
            for verse in &chapter.verses {
                refs.push(VerseRef::new(
                    &book.name,
                    chapter.number,
                    verse.verse_number,
                ));
            }
        }
    }
    refs
}

fn verse_text(book: &str, chapter: u32, verse: u32) -> &'static str {
    &bible()
        .get_verse(book, chapter, verse)
        .unwrap_or_else(|| panic!("{} {}:{} exists", book, chapter, verse))
        .text
}

fn original_words(book: &str, chapter: u32, verse: u32) -> Vec<&'static str> {
    extended()
        .get_interlinear(book, chapter, verse)
        .map(|iv| {
            iv.original_words
                .iter()
                .map(|w| w.original_text.as_str())
                .collect()
        })
        .unwrap_or_default()
}

/// English glosses of a verse's original-language words (plain text, easy to compare).
fn glosses(book: &str, chapter: u32, verse: u32) -> Vec<&'static str> {
    extended()
        .get_interlinear(book, chapter, verse)
        .map(|iv| {
            iv.original_words
                .iter()
                .map(|w| w.english_gloss.as_str())
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------- KJV text

#[test]
fn kjv_has_canonical_books_chapters_and_verses() {
    let b = bible();
    assert_eq!(b.books.len(), 66);
    assert_eq!(b.books[0].name, "Genesis");
    assert_eq!(b.books[65].name, "Revelation");

    let count = |t: Testament| -> usize {
        b.books
            .iter()
            .filter(|bk| bk.testament == t)
            .flat_map(|bk| &bk.chapters)
            .map(|c| c.verses.len())
            .sum()
    };
    assert_eq!(count(Testament::Old), 23_145);
    assert_eq!(count(Testament::New), 7_957);
    assert_eq!(
        b.books.iter().map(|bk| bk.chapters.len()).sum::<usize>(),
        1_189
    );

    for book in &b.books {
        for (i, chapter) in book.chapters.iter().enumerate() {
            assert_eq!(
                chapter.number as usize,
                i + 1,
                "{} chapter order",
                book.name
            );
            assert!(
                !chapter.verses.is_empty(),
                "{} {} is empty",
                book.name,
                chapter.number
            );
            for (j, verse) in chapter.verses.iter().enumerate() {
                let at = format!("{} {}:{}", book.name, chapter.number, verse.verse_number);
                assert_eq!(verse.verse_number as usize, j + 1, "{} numbering", at);
                assert!(!verse.text.contains("  "), "double space in {}", at);
                assert!(
                    !verse.text.contains(['[', ']', '¶', '{', '}', '<', '>']),
                    "markup in {}",
                    at
                );
            }
        }
    }
}

#[test]
fn psalm_titles_are_superscriptions() {
    let titled = bible()
        .books
        .iter()
        .flat_map(|b| &b.chapters)
        .filter(|c| c.superscription.is_some())
        .count();
    assert_eq!(titled, 116);
    let psalms = bible().books.iter().find(|b| b.name == "Psalms").unwrap();
    assert_eq!(
        psalms
            .chapters
            .iter()
            .filter(|c| c.superscription.is_some())
            .count(),
        116
    );

    let ps51 = &psalms.chapters[50];
    assert!(
        ps51.superscription
            .as_ref()
            .unwrap()
            .text
            .starts_with("To the chief Musician, A Psalm of David, when Nathan")
    );
    assert!(ps51.verses[0].text.starts_with("Have mercy upon me, O God"));
    assert!(psalms.chapters[0].superscription.is_none());
}

/// Verses where the old Project Gutenberg text differed from the 1769 KJV.
#[test]
fn known_text_corrections() {
    assert!(verse_text("Genesis", 6, 5).starts_with("And GOD saw"));
    assert!(verse_text("Psalms", 3, 1).starts_with("LORD, how are they increased"));
    assert!(verse_text("Psalms", 38, 1).starts_with("O LORD, rebuke me not"));
    assert!(verse_text("Judges", 16, 28).contains("O Lord GOD, remember me"));
    assert!(verse_text("John", 20, 28).contains("My Lord and my God"));
    assert!(verse_text("Jonah", 1, 15).starts_with("So they took up Jonah"));
    assert!(verse_text("Nehemiah", 9, 28).contains("in the hand of their enemies"));
    assert!(verse_text("First Samuel", 15, 33).starts_with("And Samuel said, As thy sword"));
    assert!(verse_text("Hosea", 14, 1).starts_with("O Israel"));
    assert!(verse_text("Mark", 12, 17).contains("Render to Cæsar"));
}

/// Every Hebrew YHWH should appear as LORD / GOD (or Jehovah / JAH) in the KJV,
/// apart from the few places the KJV itself renders it otherwise.
#[test]
fn yhwh_is_rendered_lord_or_god() {
    let allowed: HashSet<(&str, u32, u32)> = [
        ("Second Samuel", 7, 11),    // "that he will make thee an house"
        ("First Chronicles", 15, 2), // "to minister unto him"
        ("Psalms", 68, 26),          // "even the Lord"
        ("Isaiah", 10, 16),          // "the Lord, the Lord of hosts"
        ("Daniel", 9, 8),            // "O Lord, to us belongeth"
    ]
    .into_iter()
    .collect();

    let mut unexpected = Vec::new();
    for book in bible()
        .books
        .iter()
        .filter(|b| b.testament == Testament::Old)
    {
        for chapter in &book.chapters {
            for verse in chapter.superscription.iter().chain(&chapter.verses) {
                let Some(iv) =
                    extended().get_interlinear(&book.name, chapter.number, verse.verse_number)
                else {
                    continue;
                };
                let yhwh = iv
                    .original_words
                    .iter()
                    .filter(|w| matches!(w.strongs_number.as_deref(), Some("H3068" | "H3069")))
                    .count();
                let rendered = verse
                    .text
                    .split(|c: char| !c.is_alphabetic())
                    .filter(|w| matches!(*w, "LORD" | "GOD" | "JEHOVAH" | "JAH"))
                    .count()
                    + verse.text.matches("Jehovah").count();
                if yhwh > rendered
                    && !allowed.contains(&(book.name.as_str(), chapter.number, verse.verse_number))
                {
                    unexpected.push(format!(
                        "{} {}:{} {}",
                        book.name, chapter.number, verse.verse_number, verse.text
                    ));
                }
            }
        }
    }
    assert!(
        unexpected.is_empty(),
        "YHWH not rendered LORD/GOD:\n{}",
        unexpected.join("\n")
    );
}

// ---------------------------------------------------------------- original languages

#[test]
fn every_verse_has_original_language_words() {
    let ext = extended();
    let missing: Vec<String> = kjv_refs()
        .iter()
        .filter(|r| {
            ext.get_interlinear(&r.book, r.chapter, r.verse)
                .is_none_or(|iv| iv.original_words.is_empty())
        })
        .map(|r| format!("{} {}:{}", r.book, r.chapter, r.verse))
        .collect();
    assert!(
        missing.is_empty(),
        "{} verses lack Hebrew/Greek: {:?}",
        missing.len(),
        &missing[..missing.len().min(20)]
    );
}

#[test]
fn no_original_language_verse_is_orphaned() {
    let refs = kjv_refs();
    let kjv: HashSet<&VerseRef> = refs.iter().collect();
    let ext = extended();
    let orphans: Vec<&VerseRef> = ext
        .interlinear_ot
        .keys()
        .chain(ext.interlinear_nt.keys())
        .filter(|r| !kjv.contains(r))
        .collect();
    assert!(orphans.is_empty(), "no KJV verse for {:?}", orphans);
}

#[test]
fn words_are_complete_and_linked_to_the_lexicon() {
    let ext = extended();
    for iv in ext
        .interlinear_ot
        .values()
        .chain(ext.interlinear_nt.values())
    {
        for (i, w) in iv.original_words.iter().enumerate() {
            let at = format!(
                "{} {}:{} word {}",
                iv.book,
                iv.chapter,
                iv.verse_number,
                i + 1
            );
            assert_eq!(w.position as usize, i + 1, "{} position", at);
            assert!(!w.original_text.trim().is_empty(), "{} has no text", at);
            assert!(
                !w.original_text.contains(['/', '\\', '¶', '[', ']']),
                "{} markup: {}",
                at,
                w.original_text
            );
            assert!(
                !w.transliteration.contains(['/', '\\']),
                "{} transliteration: {}",
                at,
                w.transliteration
            );
            if let Some(s) = &w.strongs_number {
                assert!(
                    ext.get_lexicon_entry(s).is_some(),
                    "{} {} not in lexicon",
                    at,
                    s
                );
            }
        }
    }
}

#[test]
fn hebrew_follows_the_text_the_kjv_translated() {
    let gen1 = extended().get_interlinear("Genesis", 1, 1).unwrap();
    assert_eq!(gen1.original_words.len(), 7);
    assert_eq!(gen1.original_words[0].transliteration, "bereShit");
    assert_eq!(
        gen1.original_words[0].strongs_number.as_deref(),
        Some("H7225")
    );

    // Psalm titles are verse 0; Hebrew 3:2 is English 3:1
    assert_eq!(glosses("Psalms", 3, 0).first(), Some(&"a psalm"));
    assert_eq!(glosses("Psalms", 3, 1).first(), Some(&"O Yahweh"));
    // English Malachi 4 is Hebrew 3:19-24
    assert!(!original_words("Malachi", 4, 6).is_empty());
    // LXX-reconstructed "thirty" (type X) is not in the Hebrew the KJV followed
    assert!(
        !glosses("First Samuel", 13, 1)
            .iter()
            .any(|g| g.contains("thirty"))
    );
}

#[test]
fn greek_follows_the_textus_receptus() {
    // TR-only verses
    for (b, c, v) in [
        ("Matthew", 17, 21),
        ("Matthew", 18, 11),
        ("Matthew", 23, 14),
        ("Mark", 7, 16),
        ("Mark", 9, 44),
        ("Mark", 9, 46),
        ("Mark", 11, 26),
        ("Mark", 15, 28),
        ("Luke", 17, 36),
        ("Luke", 23, 17),
        ("John", 5, 4),
        ("Acts", 8, 37),
        ("Acts", 15, 34),
        ("Acts", 24, 7),
        ("Acts", 28, 29),
        ("Romans", 16, 24),
    ] {
        assert!(
            original_words(b, c, v).len() >= 5,
            "{} {}:{} missing TR text",
            b,
            c,
            v
        );
    }
    assert!(glosses("First John", 5, 7).contains(&"Father"));
    assert!(glosses("Matthew", 6, 13).contains(&"glory"));

    // NA-only words are left out ("in Jordan", not "in the river Jordan")
    assert!(!glosses("Matthew", 3, 6).contains(&"River"));
    // TR readings replace NA ones: "cast into hell", "Amon"
    assert_eq!(glosses("Matthew", 5, 30).last(), Some(&"may be cast"));
    assert!(glosses("Matthew", 1, 10).contains(&"Amon"));
    assert!(!glosses("Matthew", 1, 10).contains(&"Amos"));
    // Fused TR words are not doubled: "διαπαντός", not "διὰ διαπαντός"
    let acts = glosses("Acts", 10, 2);
    assert_eq!(acts.last(), Some(&"always"));
    assert!(!acts.contains(&"through"));
    // KJV versification: NRSV Rev 12:18 is KJV Rev 13:1 (and the "[13.1]" note is dropped)
    assert_eq!(&glosses("Revelation", 13, 1)[..2], &["And", "I stood"]);
    // NA's [[ ]] around Mark 16:9-20 is stripped
    assert_eq!(glosses("Mark", 16, 9).first(), Some(&"Having risen"));
}

#[test]
fn strongs_search_accepts_unpadded_numbers() {
    let ext = extended();
    let refs = ext
        .strongs_index
        .get_occurrences("H430")
        .expect("H430 found");
    assert_eq!(refs[0], VerseRef::new("Genesis", 1, 1));
    let unique: HashSet<&VerseRef> = refs.iter().collect();
    assert_eq!(unique.len(), refs.len(), "each verse listed once");
    assert_eq!(ext.strongs_count("g2316"), ext.strongs_count("G2316"));
    assert!(ext.get_lexicon_entry("H430").is_some());
}

// ---------------------------------------------------------------- red letter

#[test]
fn red_letter_quotes_all_match_their_verses() {
    let rl = RedLetterIndex::load(&root().join("data/red_letter_verses.json"))
        .expect("red-letter data loads");
    assert!(rl.len() > 2000);

    let mut problems = Vec::new();
    for (book, chapter, verse) in rl.keys() {
        let Some(v) = bible().get_verse(&book, chapter, verse) else {
            problems.push(format!("{} {}:{} is not a verse", book, chapter, verse));
            continue;
        };
        let spec = rl.get(&book, chapter, verse).unwrap();
        if let RedLetterSpec::Quote(quote) = spec {
            let red: usize = red_letter_segments(&v.text, spec)
                .iter()
                .filter(|(_, is_red)| *is_red)
                .map(|(s, _)| s.len())
                .sum();
            // Allow for folded apostrophes/ligatures and trimmed punctuation
            if red + 3 < quote.trim().len() {
                problems.push(format!(
                    "{} {}:{} only {} of {:?} is red",
                    book, chapter, verse, red, quote
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
