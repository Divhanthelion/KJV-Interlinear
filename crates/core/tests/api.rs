//! The views the app draws, checked against the real bundled data.

use std::path::Path;
use std::sync::OnceLock;

use kjv_core::api::{self, ChapterOptions, Scope};
use kjv_core::bundle::DataBundle;

fn data() -> &'static DataBundle {
    static DATA: OnceLock<DataBundle> = OnceLock::new();
    DATA.get_or_init(|| {
        DataBundle::from_sources(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")))
            .expect("bundle builds")
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
