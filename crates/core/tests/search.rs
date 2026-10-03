//! Search across the library, checked against the real data: the word index never
//! hides a match, the app's KJV reads as its reader does, and notes are found.

use std::path::Path;
use std::sync::OnceLock;
use std::time::Instant;

use kjv_core::api::{self, Scope};
use kjv_core::bundle::DataBundle;
use kjv_core::search::{self, Kind, SearchArgs, SourceResults};
use kjv_core::text::fold_for_search;
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
        let lib = Library::open(bytes).unwrap();
        // Everything stays folded, so checking every book for every query is quick
        lib.set_search_cache_limit(1 << 30);
        lib
    })
}

fn find(source: &str, query: &str) -> SourceResults {
    find_in(source, query, Scope::All, None)
}

/// A translation, or with "comm:" before its id a commentary
fn find_in(source: &str, query: &str, scope: Scope, book: Option<&str>) -> SourceResults {
    let (kind, id) = match source.strip_prefix("comm:") {
        Some(id) => (Kind::Commentary, id),
        None => (Kind::Bible, source),
    };
    let args = SearchArgs { query: query.into(), kind, source: id.into(), scope, book: book.map(String::from), limit: search::LIMIT };
    search::search(data(), lib(), &args).unwrap()
}

/// Every source: (id, its book codes, whether it's a commentary)
fn sources() -> Vec<(String, Vec<String>, bool)> {
    sources_of(lib())
}

fn sources_of(lib: &Library) -> Vec<(String, Vec<String>, bool)> {
    let mut out: Vec<(String, Vec<String>, bool)> = lib.bibles().iter().map(|b| (b.id.clone(), b.books.iter().map(|e| e.code.clone()).collect(), false)).collect();
    out.extend(lib.commentaries().iter().map(|c| (c.id.clone(), c.books.clone(), true)));
    out
}

/// Matches found by reading every book, without the index.
fn by_reading_everything(source: &str, codes: &[String], commentary: bool, query: &str) -> usize {
    let folded = fold_for_search(query.trim());
    codes
        .iter()
        .map(|code| {
            if commentary {
                lib().commentary_corpus(source, code).unwrap().find(&folded).len()
            } else if source == "kjv" && data().bible.books.iter().any(|b| Some(b.name.as_str()) == kjv_library::books::by_code(code).map(|k| k.name)) {
                // The app's KJV: its own text
                let name = kjv_library::books::by_code(code).unwrap().name;
                let book = data().bible.books.iter().find(|b| b.name == name).unwrap();
                let docs: Vec<&str> =
                    book.chapters.iter().flat_map(|ch| ch.superscription.iter().chain(ch.verses.iter())).map(|v| v.text.as_str()).collect();
                kjv_library::search::Corpus::new(docs).find(&folded).len()
            } else {
                lib().bible_corpus(source, code).unwrap().find(&folded).len()
            }
        })
        .sum()
}

#[test]
fn the_index_never_hides_a_match() {
    let queries = [
        "melchizedek",
        "propitiation",
        "lov",
        "God so loved",
        "lord's",
        "Cæsar",
        "caesar",
        "Jesus wept",
        "—",
        "3:16",
        "λόγος",
        "selah",
        "alleluia",
        "ye are the light",
        "o lord",
        "q",
        "it is finished",
        "LORD",
        "Bethlehem Ephratah",
        "\u{2019}s",
    ];
    let all = sources();
    let started = Instant::now();
    let mut checked = 0;
    for q in queries {
        for (id, codes, commentary) in &all {
            let found = find(&format!("{}{id}", if *commentary { "comm:" } else { "" }), q);
            let read = by_reading_everything(id, codes, *commentary, q);
            assert_eq!(found.total, read, "{q:?} in {id}: the search found {}, reading every book {}", found.total, read);
            checked += 1;
        }
    }
    println!("{} searches checked against reading everything in {:?}", checked, started.elapsed());
}

#[test]
fn random_pieces_of_every_source_are_found_where_they_are() {
    // A small deterministic generator: pieces of real text, with their punctuation,
    // across word boundaries, from every source
    let mut seed: u64 = 0x2545F4914F6CDD1D;
    let mut next = |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n.max(1) as u64) as usize
    };
    let mut checked = 0;
    for (id, codes, commentary) in sources() {
        for _ in 0..12 {
            let code = &codes[next(codes.len())];
            let texts: Vec<String> = if commentary {
                lib().commentary_book(&id, code).unwrap().iter().map(|n| kjv_library::notes::text(&n.body)).collect()
            } else {
                lib().verses(&id, code).unwrap().iter().map(|v| v.text.clone()).collect()
            };
            let text = &texts[next(texts.len())];
            let chars: Vec<char> = text.chars().collect();
            if chars.len() < 4 {
                continue;
            }
            let len = 3 + next(18.min(chars.len() - 3));
            let start = next(chars.len() - len + 1);
            let piece: String = chars[start..start + len].iter().collect();
            if piece.trim().is_empty() || piece.contains('\n') {
                continue;
            }
            let found = find(&format!("{}{id}", if commentary { "comm:" } else { "" }), &piece);
            let read = by_reading_everything(&id, &codes, commentary, &piece);
            assert!(read > 0, "{piece:?} is in {id} {code}");
            assert_eq!(found.total, read, "{piece:?} in {id}: the search found {}, reading every book {}", found.total, read);
            checked += 1;
        }
    }
    assert!(checked > 500, "{checked}");
}

#[test]
fn the_kjv_reads_as_its_reader_does() {
    let r = find("kjv", "Jesus wept");
    assert_eq!(r.total, 1);
    let hit = &r.hits[0];
    assert_eq!((hit.book.as_str(), hit.chapter, hit.verse, hit.reference.as_str()), ("John", 11, 35, "John 11:35"));
    let marked: Vec<&str> = hit.segments.iter().filter(|s| s.hit).map(|s| s.text.as_str()).collect();
    assert_eq!(marked, ["Jesus wept"]);
    // The same verses as the KJV search the app has always had, and the Apocrypha too
    for q in ["love", "caesar's", "selah", "LORD of hosts"] {
        let old = api::search(data(), q, Scope::New, None, 0).total;
        assert_eq!(find_in("kjv", q, Scope::New, None).total, old, "{q} in the New Testament");
        let old_all = api::search(data(), q, Scope::All, None, 0).total;
        assert!(find("kjv", q).total >= old_all, "{q}: the Apocrypha adds to the 66 books");
    }
    assert!(find_in("kjv", "Tobias", Scope::Old, None).total > 0, "Tobit is searched");
    let title = find_in("kjv", "To the chief Musician, A Psalm of David", Scope::Book, Some("Psalms"));
    assert!(title.hits.iter().any(|h| h.reference == "Psalm 51 (title)" && h.verse == 0), "Psalm titles");
    // Scope: one book
    let john = find_in("kjv", "love", Scope::Book, Some("John"));
    assert!(john.hits.iter().all(|h| h.book == "John") && john.total > 0);
}

#[test]
fn other_translations_and_their_numbering() {
    // The Douay-Rheims' Psalm 22 is the KJV's 23
    let r = find("dra", "The Lord ruleth me");
    assert_eq!(r.total, 1);
    assert_eq!((r.hits[0].book.as_str(), r.hits[0].chapter, r.hits[0].verse, r.hits[0].reference.as_str()), ("Psalms", 22, 1, "Psalm 22:1"));
    // A translation without the New Testament has nothing there to read
    let jps = find_in("jps", "Jesus", Scope::New, None);
    assert_eq!((jps.total, jps.read), (0, 0));
    // A rare word reads only the books it's in
    let web = find("web", "Melchizedek");
    assert!(web.total > 0);
    assert!(web.read <= 4 && web.skipped > 60, "read {} skipped {}", web.read, web.skipped);
}

#[test]
fn notes_are_found_with_the_words_around_the_match() {
    let r = find("comm:mhc", "Melchizedek");
    assert!(r.total > 10, "{}", r.total);
    assert_eq!(r.kind, Kind::Commentary);
    let first = &r.hits[0];
    assert_eq!((first.book.as_str(), first.reference.starts_with("Genesis 14")), ("Genesis", true), "{}", first.reference);
    assert!(first.segments.iter().any(|s| s.hit && s.text.eq_ignore_ascii_case("melchizedek")));
    let text: String = first.segments.iter().map(|s| s.text.as_str()).collect();
    assert!(text.chars().count() < 420, "a snippet, not the note: {}", text.len());
    // Introductions are placed at chapter or verse 0
    let intro = find_in("comm:tyndale", "Paul wrote this letter", Scope::Book, Some("Romans"));
    assert!(intro.hits.iter().any(|h| h.reference == "Romans (introduction)" && h.chapter == 0), "{:?}", intro.hits.iter().map(|h| &h.reference).collect::<Vec<_>>());
    // Results stop at the limit; the total doesn't
    let mut args = SearchArgs { query: "the".into(), kind: Kind::Commentary, source: "gill".into(), scope: Scope::All, book: None, limit: 20 };
    let many = search::search(data(), lib(), &args).unwrap();
    assert_eq!(many.hits.len(), 20);
    assert!(many.total > 20_000, "{}", many.total);
    args.query = "   ".into();
    assert_eq!(search::search(data(), lib(), &args).unwrap().total, 0);
    args.source = "haydock".into();
    assert!(search::search(data(), lib(), &args).is_err());
}

#[test]
fn the_translation_being_read_is_searched_quickly() {
    // Once a translation has been searched (its books folded), searching it again
    // takes a few milliseconds here; the budget leaves room for slower machines
    for source in ["kjv", "web", "dra"] {
        find(source, "love");
        let started = Instant::now();
        let r = find(source, "love");
        let took = started.elapsed();
        assert!(r.total > 400, "{source}: {}", r.total);
        assert!(took.as_millis() < 100, "{source}: searching again took {took:?}");
    }
}

/// Times for the record: `cargo test --release -p kjv-core --test search -- --ignored --nocapture timings`
#[test]
#[ignore]
fn timings() {
    let fresh = || {
        let (bytes, _) = build::archive(root(), &|b| zstd::encode_all(b, 19).unwrap()).unwrap();
        Library::open(bytes).unwrap()
    };
    let lib = fresh();
    data();
    let time_limit = |lib: &Library, source: &str, q: &str, limit: usize| {
        let (kind, source) = match source.strip_prefix("comm:") {
            Some(id) => (Kind::Commentary, id),
            None => (Kind::Bible, source),
        };
        let args = SearchArgs { query: q.into(), kind, source: source.into(), scope: Scope::All, book: None, limit };
        let t = Instant::now();
        let r = search::search(data(), lib, &args).unwrap();
        (t.elapsed(), r.total, r.read)
    };
    let time = |lib: &Library, source: &str, q: &str| time_limit(lib, source, q, search::LIMIT);
    for (source, q) in [("kjv", "love"), ("kjv", "love"), ("web", "love"), ("web", "love"), ("web", "melchizedek"), ("comm:mhc", "melchizedek"), ("comm:mhc", "love"), ("comm:mhc", "love")] {
        let (t, total, read) = time(&lib, source, q);
        println!("{source:10} {q:14} {total:6} in {read:3} books: {t:?}");
    }
    // Everything, one source after another, cold and then warm (each query on a fresh
    // library for cold)
    for q in ["melchizedek", "love"] {
        let lib = fresh();
        for round in ["cold", "warm"] {
            let started = Instant::now();
            let mut total = 0;
            for (id, _, commentary) in sources_of(&lib) {
                // As the app asks when searching several sources: the first 20 of each
                total += time_limit(&lib, &format!("{}{id}", if commentary { "comm:" } else { "" }), q, 20).1;
            }
            println!("everything {round:4} {q:12} {total:7}: {:?}", started.elapsed());
        }
    }
}
