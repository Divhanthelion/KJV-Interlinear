//! The views the app draws, checked against the real bundled data.

use std::path::Path;
use std::sync::OnceLock;

use kjv_core::api::{self, ChapterOptions, Scope};
use kjv_core::bundle::DataBundle;

fn data() -> &'static DataBundle {
    static DATA: OnceLock<DataBundle> = OnceLock::new();
    DATA.get_or_init(|| {
        DataBundle::from_sources(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))).expect("bundle builds")
    })
}

fn full() -> ChapterOptions {
    ChapterOptions { red_letter: true, query: None, original: true }
}

#[test]
fn books_have_display_names() {
    let books = api::books(data());
    assert_eq!(books.len(), 66);
    let sam = books.iter().find(|b| b.name == "First Samuel").unwrap();
    assert_eq!((sam.display.as_str(), sam.abbr.as_str(), sam.chapters), ("1 Samuel", "1 Sam", 31));
    assert_eq!(books[39].testament, "new");
    assert_eq!(books.iter().map(|b| b.chapters).sum::<u32>(), 1189);
}

#[test]
fn chapter_navigation_crosses_books() {
    let gen1 = api::chapter(data(), "Genesis", 1, &full()).unwrap();
    assert!(gen1.prev.is_none());
    assert_eq!(gen1.next.as_ref().map(|n| (n.book.as_str(), n.chapter)), Some(("Genesis", 2)));

    let mal4 = api::chapter(data(), "Malachi", 4, &full()).unwrap();
    assert_eq!(mal4.next.as_ref().map(|n| (n.book.as_str(), n.chapter)), Some(("Matthew", 1)));
    let mat1 = api::chapter(data(), "Matthew", 1, &full()).unwrap();
    assert_eq!(mat1.prev.as_ref().map(|n| (n.book.as_str(), n.chapter)), Some(("Malachi", 4)));

    let rev22 = api::chapter(data(), "Revelation", 22, &full()).unwrap();
    assert!(rev22.next.is_none());
    assert_eq!(rev22.verses.len(), 21);

    assert!(api::chapter(data(), "Genesis", 51, &full()).is_err());
    assert!(api::chapter(data(), "Hezekiah", 1, &full()).is_err());
}

#[test]
fn psalm_titles_and_headings() {
    let ps51 = api::chapter(data(), "Psalms", 51, &full()).unwrap();
    assert_eq!(ps51.heading, "Psalm 51");
    let title = ps51.title.as_ref().expect("Psalm 51 has a title");
    assert_eq!(title.number, 0);
    assert!(title.segments[0].text.starts_with("To the chief Musician"));
    assert_eq!(title.original.as_ref().unwrap().lang, "he");
    assert!(api::chapter(data(), "Psalms", 1, &full()).unwrap().title.is_none());
    assert_eq!(api::reference("Psalms", 23, 1), "Psalm 23:1");
    assert_eq!(api::reference("First John", 5, 7), "1 John 5:7");
}

#[test]
fn verses_carry_red_letter_and_original_words() {
    let mark12 = api::chapter(data(), "Mark", 12, &full()).unwrap();
    let v17 = &mark12.verses[16];
    let red: String = v17.segments.iter().filter(|s| s.red).map(|s| s.text.as_str()).collect();
    assert!(red.starts_with("Render to Cæsar"), "{}", red);
    assert!(v17.segments.iter().any(|s| !s.red && s.text.starts_with("And Jesus answering")));
    let greek = v17.original.as_ref().unwrap();
    assert_eq!(greek.lang, "grc");

    let gen1 = api::chapter(data(), "Genesis", 1, &full()).unwrap();
    let words = &gen1.verses[0].original.as_ref().unwrap().words;
    assert_eq!(words.len(), 7);
    assert_eq!(words[2].strongs.as_deref(), Some("H430"));
    assert_eq!(words[2].key.as_deref(), Some("H0430"));
    assert_eq!(words[3].gloss, "(obj.)");

    // Red letter off and originals off
    let plain = api::chapter(data(), "Mark", 12, &ChapterOptions::default()).unwrap();
    assert!(plain.verses.iter().all(|v| v.original.is_none() && v.segments.iter().all(|s| !s.red)));
}

#[test]
fn search_marks_hits_and_folds_typography() {
    let r = api::search(data(), "Caesar's", Scope::All, None, 1000);
    assert_eq!(r.total, 8);
    let first = &r.hits[0];
    assert_eq!(first.reference, "Matthew 22:21");
    assert!(first.segments.iter().any(|s| s.hit && s.text == "Cæsar’s"));

    let limited = api::search(data(), "the", Scope::All, None, 50);
    assert_eq!(limited.hits.len(), 50);
    assert!(limited.total > 20_000);

    let in_book = api::search(data(), "wept", Scope::Book, Some("John"), 1000);
    assert!(in_book.hits.iter().all(|h| h.book == "John"));
    assert!(in_book.hits.iter().any(|h| h.reference == "John 11:35"));
    let nt = api::search(data(), "LORD", Scope::New, None, 5);
    assert!(nt.hits.iter().all(|h| !["Genesis", "Psalms"].contains(&h.book.as_str())));

    // Psalm titles are searchable
    let titles = api::search(data(), "chief Musician", Scope::All, None, 1000);
    assert!(titles.hits.iter().any(|h| h.reference == "Psalm 51 (title)"));

    assert_eq!(api::search(data(), "   ", Scope::All, None, 10).total, 0);
}

#[test]
fn strongs_search_and_lexicon() {
    let r = api::strongs_search(data(), "h430", 100);
    assert_eq!(r.strongs.as_deref(), Some("H430"));
    assert_eq!(r.hits[0].reference, "Genesis 1:1");
    assert_eq!(r.hits.len(), 100);
    assert!(r.total > 2000);
    assert_eq!(r.lexicon.as_ref().map(|l| l.gloss.as_str()), Some("God"));

    let greek = api::lexicon(data(), "G3056").unwrap();
    assert_eq!((greek.strongs.as_str(), greek.lang), ("G3056", "grc"));
    assert_eq!(greek.gloss, "word");
    assert!(greek.definition.starts_with(&greek.word), "definition opens with the headword");

    let none = api::strongs_search(data(), "hello", 10);
    assert!(none.key.is_none() && none.hits.is_empty());
    assert!(api::lexicon(data(), "H999999").is_none());
}

/// The word's sense code picks the lexicon sense STEP tagged it with.
#[test]
fn words_open_the_lexicon_at_their_own_sense() {
    // Hebrews 4:8 "For if Jesus had given them rest": the Greek is Joshua's name
    let heb4 = api::chapter(data(), "Hebrews", 4, &full()).unwrap();
    let words = &heb4.verses[7].original.as_ref().unwrap().words;
    let joshua = words.iter().find(|w| w.key.as_deref() == Some("G2424")).expect("Ἰησοῦς in Hebrews 4:8");
    assert_eq!((joshua.strongs.as_deref(), joshua.dkey.as_deref()), (Some("G2424"), Some("G2424I")));
    let lex = api::lexicon(data(), joshua.dkey.as_deref().unwrap()).unwrap();
    assert!(lex.gloss.contains("Joshua") && !lex.gloss.contains("Jesus"), "{}", lex.gloss);
    assert_eq!((lex.strongs.as_str(), lex.key.as_str()), ("G2424", "G2424"));
    // The plain number is still Jesus, and the concordance is the same for both
    assert_eq!(api::lexicon(data(), "G2424").unwrap().gloss, "Jesus");
    let by_sense = api::strongs_search(data(), "G2424I", 10);
    assert_eq!(by_sense.lexicon.as_ref().map(|l| l.gloss.as_str()), Some("Joshua"));
    assert_eq!(
        (by_sense.key.as_deref(), by_sense.total),
        (Some("G2424"), api::strongs_search(data(), "G2424", 10).total)
    );

    // Genesis 1:1 "beginning": H7225 with its sense, and without one
    let gen1 = api::chapter(data(), "Genesis", 1, &full()).unwrap();
    let first = &gen1.verses[0].original.as_ref().unwrap().words[0];
    assert_eq!((first.key.as_deref(), first.dkey.as_deref()), (Some("H7225"), Some("H7225G")));
    assert!(api::lexicon(data(), "H7225G").unwrap().gloss.contains("beginning"));
    assert!(api::lexicon(data(), "H7225").is_some());
    // A word without a sense letter looks itself up
    let mat1 = api::chapter(data(), "Matthew", 1, &full()).unwrap();
    let book = &mat1.verses[0].original.as_ref().unwrap().words[0];
    assert_eq!((book.key.as_deref(), book.dkey.as_deref()), (Some("G0976"), Some("G0976")));
    // A sense the lexicon lacks falls back to the number
    assert_eq!(api::lexicon(data(), "G2424Z").unwrap().gloss, "Jesus");
}

#[test]
fn aramaic_verses_are_tagged_arc() {
    let lang = |book: &str, chapter: u32, verse: u32| {
        let view = api::chapter(data(), book, chapter, &full()).unwrap();
        view.verses[verse as usize - 1].original.as_ref().unwrap().lang
    };
    assert_eq!(lang("Daniel", 2, 4), "arc");
    assert_eq!(lang("Daniel", 2, 5), "arc");
    assert_eq!(lang("Jeremiah", 10, 11), "arc");
    assert_eq!(lang("Jeremiah", 10, 10), "he");
    assert_eq!(lang("Genesis", 1, 1), "he");
}

#[test]
fn search_ignores_extra_spaces_in_the_query() {
    let spaced = api::search(data(), "God  so", Scope::All, None, 1000);
    assert_eq!(spaced.total, api::search(data(), "God so", Scope::All, None, 1000).total);
    let john = spaced.hits.iter().find(|h| h.reference == "John 3:16").expect("John 3:16 found");
    assert!(john.segments.iter().any(|s| s.hit && s.text == "God so"));

    let options = ChapterOptions { red_letter: false, query: Some(" God \t so ".into()), original: false };
    let john3 = api::chapter(data(), "John", 3, &options).unwrap();
    assert!(john3.verses[15].segments.iter().any(|s| s.hit && s.text == "God so"));
}

#[test]
fn copy_errors_name_the_verse() {
    let err = kjv_core::dispatch::dispatch(
        data(),
        "copy_text",
        serde_json::json!({"book": "John", "chapter": 3, "verse": 99}),
    )
    .unwrap_err();
    assert_eq!(err, "no text for John 3:99");
    let err = kjv_core::dispatch::dispatch(data(), "copy_text", serde_json::json!({"book": "John", "chapter": 30}))
        .unwrap_err();
    assert_eq!(err, "no text for John 30");
}

#[test]
fn clipboard_text() {
    assert_eq!(
        api::copy_verse(data(), "John", 3, 16).unwrap(),
        "John 3:16 KJV\nFor God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life."
    );
    let ps117 = api::copy_chapter(data(), "Psalms", 117).unwrap();
    assert!(ps117.starts_with("Psalm 117 KJV\n1 O praise the LORD"));
    let ps3 = api::copy_chapter(data(), "Psalms", 3).unwrap();
    assert!(ps3.starts_with("Psalm 3 KJV\nA Psalm of David, when he fled from Absalom his son.\n1 LORD"));
}
