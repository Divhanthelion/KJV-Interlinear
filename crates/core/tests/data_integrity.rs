//! Checks the bundled KJV, STEP Bible, and red-letter data exactly as the app loads it.

use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;

use kjv_core::bundle::DataBundle;
use kjv_core::models::{Bible, ExtendedBible, OriginalLanguage, Testament, Verse, VerseRef};
use kjv_core::original_languages::load_extended_bible;
use kjv_core::red_letter::RedLetterIndex;

/// Repository root, where the source data lives (this crate is at crates/core).
fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn bible() -> &'static Bible {
    static BIBLE: OnceLock<Bible> = OnceLock::new();
    BIBLE.get_or_init(|| {
        Bible::from_directories(&root().join("old_testament"), &root().join("new_testament")).expect("KJV text loads")
    })
}

fn extended() -> &'static ExtendedBible {
    static EXT: OnceLock<ExtendedBible> = OnceLock::new();
    EXT.get_or_init(|| load_extended_bible(&root().join("data")).expect("STEP data loads"))
}

fn bundle() -> &'static DataBundle {
    static BUNDLE: OnceLock<DataBundle> = OnceLock::new();
    BUNDLE.get_or_init(|| DataBundle::from_sources(root()).expect("bundle builds"))
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
                refs.push(VerseRef::new(&book.name, chapter.number, verse.verse_number));
            }
        }
    }
    refs
}

fn verse_text(book: &str, chapter: u32, verse: u32) -> &'static str {
    &bible().get_verse(book, chapter, verse).unwrap_or_else(|| panic!("{} {}:{} exists", book, chapter, verse)).text
}

fn original_words(book: &str, chapter: u32, verse: u32) -> Vec<&'static str> {
    extended()
        .get_interlinear(book, chapter, verse)
        .map(|iv| iv.original_words.iter().map(|w| w.original_text.as_str()).collect())
        .unwrap_or_default()
}

/// English glosses of a verse's original-language words (plain text, easy to compare).
fn glosses(book: &str, chapter: u32, verse: u32) -> Vec<&'static str> {
    extended()
        .get_interlinear(book, chapter, verse)
        .map(|iv| iv.original_words.iter().map(|w| w.english_gloss.as_str()).collect())
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
        b.books.iter().filter(|bk| bk.testament == t).flat_map(|bk| &bk.chapters).map(|c| c.verses.len()).sum()
    };
    assert_eq!(count(Testament::Old), 23_145);
    assert_eq!(count(Testament::New), 7_957);
    assert_eq!(b.books.iter().map(|bk| bk.chapters.len()).sum::<usize>(), 1_189);

    for book in &b.books {
        for (i, chapter) in book.chapters.iter().enumerate() {
            assert_eq!(chapter.number as usize, i + 1, "{} chapter order", book.name);
            assert!(!chapter.verses.is_empty(), "{} {} is empty", book.name, chapter.number);
            for (j, verse) in chapter.verses.iter().enumerate() {
                let at = format!("{} {}:{}", book.name, chapter.number, verse.verse_number);
                assert_eq!(verse.verse_number as usize, j + 1, "{} numbering", at);
                assert!(!verse.text.contains("  "), "double space in {}", at);
                assert!(!verse.text.contains(['[', ']', '¶', '{', '}', '<', '>']), "markup in {}", at);
            }
        }
    }
}

#[test]
fn psalm_titles_are_superscriptions() {
    let titled = bible().books.iter().flat_map(|b| &b.chapters).filter(|c| c.superscription.is_some()).count();
    assert_eq!(titled, 116);
    let psalms = bible().books.iter().find(|b| b.name == "Psalms").unwrap();
    assert_eq!(psalms.chapters.iter().filter(|c| c.superscription.is_some()).count(), 116);

    let ps51 = &psalms.chapters[50];
    assert!(
        ps51.superscription.as_ref().unwrap().text.starts_with("To the chief Musician, A Psalm of David, when Nathan")
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
    for book in bible().books.iter().filter(|b| b.testament == Testament::Old) {
        for chapter in &book.chapters {
            for verse in chapter.superscription.iter().chain(&chapter.verses) {
                let Some(iv) = extended().get_interlinear(&book.name, chapter.number, verse.verse_number) else {
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
                if yhwh > rendered && !allowed.contains(&(book.name.as_str(), chapter.number, verse.verse_number)) {
                    unexpected.push(format!("{} {}:{} {}", book.name, chapter.number, verse.verse_number, verse.text));
                }
            }
        }
    }
    assert!(unexpected.is_empty(), "YHWH not rendered LORD/GOD:\n{}", unexpected.join("\n"));
}

// ---------------------------------------------------------------- original languages

#[test]
fn every_verse_has_original_language_words() {
    let ext = extended();
    let missing: Vec<String> = kjv_refs()
        .iter()
        .filter(|r| ext.get_interlinear(&r.book, r.chapter, r.verse).is_none_or(|iv| iv.original_words.is_empty()))
        .map(|r| format!("{} {}:{}", r.book, r.chapter, r.verse))
        .collect();
    assert!(missing.is_empty(), "{} verses lack Hebrew/Greek: {:?}", missing.len(), &missing[..missing.len().min(20)]);
}

#[test]
fn no_original_language_verse_is_orphaned() {
    let refs = kjv_refs();
    let kjv: HashSet<&VerseRef> = refs.iter().collect();
    let ext = extended();
    let orphans: Vec<&VerseRef> =
        ext.interlinear_ot.keys().chain(ext.interlinear_nt.keys()).filter(|r| !kjv.contains(r)).collect();
    assert!(orphans.is_empty(), "no KJV verse for {:?}", orphans);
}

/// Every word of every file is loaded: these totals change only with the data
/// files themselves (or with a deliberate change to what the loader keeps).
#[test]
fn every_word_row_is_loaded() {
    let words = |verses: &std::collections::HashMap<VerseRef, kjv_core::models::InterlinearVerse>| -> usize {
        verses.values().map(|v| v.original_words.len()).sum()
    };
    // TAHOT Gen-Deu 79,981 + Jos-Est 107,120 + Job-Sng 39,080 + Isa-Mal 79,305
    assert_eq!(words(&extended().interlinear_ot), 305_486);
    // TAGNT Mat-Jhn 66,355 + Act-Rev 74,542
    assert_eq!(words(&extended().interlinear_nt), 140_897);
    // Lexicon entries by plain number; sense codes ("G2424I") are keyed besides these
    let plain = |lexicon: &std::collections::HashMap<String, kjv_core::models::LexiconEntry>| {
        lexicon.keys().filter(|k| k[1..].chars().all(|c| c.is_ascii_digit())).count()
    };
    assert_eq!(plain(&extended().hebrew_lexicon), 8_723);
    assert_eq!(plain(&extended().greek_lexicon), 10_847);
}

#[test]
fn words_are_complete_and_linked_to_the_lexicon() {
    let ext = extended();
    for iv in ext.interlinear_ot.values().chain(ext.interlinear_nt.values()) {
        for (i, w) in iv.original_words.iter().enumerate() {
            let at = format!("{} {}:{} word {}", iv.book, iv.chapter, iv.verse_number, i + 1);
            assert_eq!(w.position as usize, i + 1, "{} position", at);
            assert!(!w.original_text.trim().is_empty(), "{} has no text", at);
            assert!(!w.original_text.contains(['/', '\\', '¶', '[', ']']), "{} markup: {}", at, w.original_text);
            assert!(!w.transliteration.contains(['/', '\\']), "{} transliteration: {}", at, w.transliteration);
            if let Some(s) = &w.strongs_number {
                assert!(ext.get_lexicon_entry(s).is_some(), "{} {} not in lexicon", at, s);
            }
            if let Some(d) = &w.dstrong {
                assert!(
                    w.strongs_number.as_ref().is_some_and(|s| d.starts_with(s.as_str()) && d.len() > s.len()),
                    "{} sense {}",
                    at,
                    d
                );
            }
        }
    }
}

#[test]
fn aramaic_verses_are_marked_aramaic() {
    let language = |b: &str, c: u32, v: u32| extended().get_interlinear(b, c, v).map(|iv| iv.language.clone());
    assert_eq!(language("Genesis", 1, 1), Some(OriginalLanguage::Hebrew));
    assert_eq!(language("Psalms", 3, 0), Some(OriginalLanguage::Hebrew));
    assert_eq!(language("Jeremiah", 10, 11), Some(OriginalLanguage::Aramaic));
    // Daniel 2:4 turns to Aramaic after "to the king in Syriack"; 2:4b–7:28 is Aramaic
    assert_eq!(language("Daniel", 2, 3), Some(OriginalLanguage::Hebrew));
    assert_eq!(language("Daniel", 2, 4), Some(OriginalLanguage::Aramaic));
    assert_eq!(language("Daniel", 2, 5), Some(OriginalLanguage::Aramaic));
    assert_eq!(language("Daniel", 7, 28), Some(OriginalLanguage::Aramaic));
    assert_eq!(language("Daniel", 8, 1), Some(OriginalLanguage::Hebrew));
    assert_eq!(language("Ezra", 4, 7), Some(OriginalLanguage::Hebrew));
    assert_eq!(language("Ezra", 4, 8), Some(OriginalLanguage::Aramaic));
    assert_eq!(language("Matthew", 1, 1), Some(OriginalLanguage::Greek));
}

#[test]
fn hebrew_follows_the_text_the_kjv_translated() {
    let gen1 = extended().get_interlinear("Genesis", 1, 1).unwrap();
    assert_eq!(gen1.original_words.len(), 7);
    assert_eq!(gen1.original_words[0].transliteration, "bereShit");
    assert_eq!(gen1.original_words[0].strongs_number.as_deref(), Some("H7225"));

    // Psalm titles are verse 0; Hebrew 3:2 is English 3:1
    assert_eq!(glosses("Psalms", 3, 0).first(), Some(&"a psalm"));
    assert_eq!(glosses("Psalms", 3, 1).first(), Some(&"O Yahweh"));
    // English Malachi 4 is Hebrew 3:19-24
    assert!(!original_words("Malachi", 4, 6).is_empty());
    // LXX-reconstructed "thirty" (type X) is not in the Hebrew the KJV followed
    assert!(!glosses("First Samuel", 13, 1).iter().any(|g| g.contains("thirty")));
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
        assert!(original_words(b, c, v).len() >= 5, "{} {}:{} missing TR text", b, c, v);
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
    let refs = ext.strongs_index.get_occurrences("H430").expect("H430 found");
    assert_eq!(refs[0], VerseRef::new("Genesis", 1, 1));
    let unique: HashSet<&VerseRef> = refs.iter().collect();
    assert_eq!(unique.len(), refs.len(), "each verse listed once");
    assert_eq!(ext.strongs_count("g2316"), ext.strongs_count("G2316"));
    assert!(ext.get_lexicon_entry("H430").is_some());
}

// ---------------------------------------------------------------- red letter

#[test]
fn red_letter_spans_are_exact() {
    let rl = RedLetterIndex::load(&root().join("data/words_of_jesus.json")).expect("red-letter data loads");
    assert_eq!(rl.len(), 2028);
    let problems = rl.unresolved(bible());
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Red text for a verse, spans joined with " / ".
fn red_text(book: &str, chapter: u32, verse: u32) -> String {
    let rl = RedLetterIndex::load(&root().join("data/words_of_jesus.json")).unwrap();
    rl.get(book, chapter, verse).map(|s| s.join(" / ")).unwrap_or_default()
}

/// Places where the previous (kjvstudy.org) map was wrong.
#[test]
fn red_letter_marks_only_the_words_spoken() {
    // Narration is not red
    assert_eq!(red_text("John", 11, 35), "");
    assert_eq!(red_text("John", 7, 20), "");
    assert_eq!(red_text("John", 11, 43), "Lazarus, come forth.");
    assert_eq!(red_text("John", 11, 41), "Father, I thank thee that thou hast heard me.");
    assert_eq!(red_text("John", 12, 28), "Father, glorify thy name.");
    // Two quotations in one verse
    assert_eq!(red_text("Acts", 1, 4), "but wait for the promise of the Father, which, / ye have heard of me.");
    // Words of the risen Christ outside the Gospels
    assert!(red_text("Acts", 9, 11).starts_with("Arise, and go into the street which is called Straight"));
    assert_eq!(red_text("Acts", 20, 35), "It is more blessed to give than to receive.");
    // Unchanged classics
    assert!(red_text("John", 3, 16).starts_with("For God so loved the world"));
    assert!(red_text("Matthew", 5, 3).starts_with("Blessed are the poor in spirit"));
}

// ---------------------------------------------------------------- embedded bundle

#[test]
fn bundle_round_trips_exactly() {
    let bundle = bundle();
    let bytes = bundle.to_bytes().expect("bundle serializes");
    let back = DataBundle::from_bytes(&bytes).expect("bundle reads back");

    // The same length again means nothing was lost in the round trip (the bytes
    // themselves can differ: maps serialize in their in-memory order)
    assert_eq!(back.to_bytes().unwrap().len(), bytes.len());
    assert_eq!(back.bible.books.len(), 66);
    assert_eq!(back.extended.interlinear_ot.len(), bundle.extended.interlinear_ot.len());
    assert_eq!(back.extended.interlinear_nt.len(), bundle.extended.interlinear_nt.len());
    assert_eq!(back.red_letter.len(), bundle.red_letter.len());
    // The concordance rebuilt from the bundle matches the one built while loading
    for (map, original) in [
        (&back.extended.strongs_index.hebrew, &bundle.extended.strongs_index.hebrew),
        (&back.extended.strongs_index.greek, &bundle.extended.strongs_index.greek),
    ] {
        assert_eq!(map.len(), original.len());
        for (key, refs) in original {
            assert_eq!(map.get(key), Some(refs), "{} differs after rebuild", key);
        }
    }
    std::fs::write(concat!(env!("CARGO_TARGET_TMPDIR"), "/bundle.bin"), &bytes).unwrap();
    let v = back.bible.get_verse("John", 3, 16).unwrap();
    assert!(v.text.starts_with("For God so loved the world"));
    eprintln!("bundle size: {:.1} MB", bytes.len() as f64 / 1e6);
}

/// The problems `validate` reports after `change` breaks a copy of the real bundle.
fn broken(change: impl FnOnce(&mut DataBundle)) -> String {
    let mut b = bundle().clone();
    change(&mut b);
    b.validate().expect_err("validate catches the change")
}

/// `from_sources` validates, so these fail the app's build rather than ship.
#[test]
fn validation_rejects_an_incomplete_bundle() {
    assert_eq!(bundle().validate(), Ok(()));

    let err = broken(|b| b.bible.books.swap(0, 1));
    assert!(err.contains("not the 66 in canonical order"), "{}", err);

    let err = broken(|b| {
        b.bible.books[0].chapters[0].verses.remove(4);
    });
    assert!(err.contains("Genesis 1: verse 5 is numbered 6"), "{}", err);
    assert!(err.contains("31101 verses, 1189 chapters, and 116 Psalm titles"), "{}", err);

    let err = broken(|b| {
        b.bible.books[42].chapters.remove(2);
    });
    assert!(err.contains("John: chapter 3 is numbered 4"), "{}", err);

    let err = broken(|b| {
        let first = b.bible.books[1].chapters[0].verses[0].clone();
        b.bible.books[1].chapters[0].superscription = Some(Verse { verse_number: 0, ..first });
    });
    assert!(err.contains("Exodus 1 has a title; only Psalms do"), "{}", err);
    assert!(err.contains("Exodus 1:0 has no Hebrew or Greek words"), "{}", err);

    let err = broken(|b| {
        b.extended.interlinear_nt.remove(&VerseRef::new("John", 3, 16));
    });
    assert!(err.contains("John 3:16 has no Hebrew or Greek words"), "{}", err);

    let err = broken(|b| {
        let ch = &mut b.bible.books[42].chapters[10];
        ch.verses[42].text = ch.verses[42].text.replace("Lazarus", "Lazarvs");
    });
    assert!(err.contains("John 11:43: 0 of 1 red-letter spans found"), "{}", err);
}
