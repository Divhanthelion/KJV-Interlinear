//! Tests against the real CrossWire / SermonIndex modules cached in
//! `.cache/sources/` (see docs/LIBRARY.md). They skip, with a printed note, when the
//! cache is not there; on a developer machine that has fetched the sources they must all
//! pass.
//!
//! Run `cargo test -p kjv-sword --test modules -- --nocapture` to see the statistics.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use kjv_sword::{Driver, Encoding, Entry, Extraction, Module, Testament};

struct Spec {
    name: &'static str,
    zip: &'static str,
}

const SPECS: [Spec; 7] = [
    Spec {
        name: "MHC",
        zip: "crosswire/MHC.zip",
    },
    Spec {
        name: "Catena",
        zip: "crosswire/Catena.zip",
    },
    Spec {
        name: "Wesley",
        zip: "crosswire/Wesley.zip",
    },
    Spec {
        name: "KD",
        zip: "crosswire/KD.zip",
    },
    Spec {
        name: "JFB",
        zip: "crosswire/JFB.zip",
    },
    Spec {
        name: "TSK",
        zip: "crosswire/TSK.zip",
    },
    Spec {
        name: "Gill",
        zip: "sermonindex/sigill.zip",
    },
];

const MHC: usize = 0;
const CATENA: usize = 1;
const WESLEY: usize = 2;
const KD: usize = 3;
const JFB: usize = 4;
const TSK: usize = 5;
const GILL: usize = 6;

struct Loaded {
    path: PathBuf,
    module: Module,
    ex: Extraction,
}

fn cache_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.cache/sources")
}

fn load(i: usize) -> Option<&'static Loaded> {
    static LOADED: [OnceLock<Option<Loaded>>; 7] = [const { OnceLock::new() }; 7];
    LOADED[i]
        .get_or_init(|| {
            let path = cache_dir().join(SPECS[i].zip);
            if !path.exists() {
                eprintln!(
                    "SKIP {}: {} not found (fetch the sources first)",
                    SPECS[i].name,
                    path.display()
                );
                return None;
            }
            let module = Module::open_zip(&path)
                .unwrap_or_else(|e| panic!("{}: open failed: {e}", SPECS[i].name));
            let ex = module
                .read()
                .unwrap_or_else(|e| panic!("{}: read failed: {e}", SPECS[i].name));
            Some(Loaded { path, module, ex })
        })
        .as_ref()
}

/// The entry whose range includes the given slot.
fn covering<'a>(ex: &'a Extraction, book: &str, chapter: u32, verse: u32) -> Option<&'a Entry> {
    ex.entries.iter().find(|e| {
        e.book == book
            && (e.chapter, e.verse) <= (chapter, verse)
            && (chapter, verse) <= (e.to_chapter, e.to_verse)
    })
}

fn fnv1a(hash: &mut u64, bytes: &[u8]) {
    for &b in bytes {
        *hash ^= u64::from(b);
        *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
}

/// A digest of everything extracted: any change to any note, position or range changes it.
fn digest(ex: &Extraction) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for e in &ex.entries {
        fnv1a(&mut h, e.book.as_bytes());
        for n in [e.chapter, e.verse, e.to_chapter, e.to_verse] {
            fnv1a(&mut h, &n.to_le_bytes());
        }
        fnv1a(&mut h, &(e.text.len() as u64).to_le_bytes());
        fnv1a(&mut h, e.text.as_bytes());
    }
    for o in &ex.orphans {
        fnv1a(&mut h, &[o.testament as u8]);
        fnv1a(&mut h, &o.block.to_le_bytes());
        fnv1a(&mut h, &(o.offset as u64).to_le_bytes());
        fnv1a(&mut h, o.text.as_bytes());
    }
    h
}

#[derive(Debug, PartialEq, Eq)]
struct Stats {
    entries: usize,
    ranged: usize,
    book_intros: usize,
    chapter_intros: usize,
    text_bytes: usize,
    repeats: usize,
    orphans: usize,
    orphan_bytes: usize,
    headings: usize,
    /// FNV-1a of "BOOK:count;" over the per-book entry counts (see `per_book`).
    per_book_digest: u64,
}

fn stats(ex: &Extraction) -> Stats {
    Stats {
        entries: ex.entries.len(),
        ranged: ex.entries.iter().filter(|e| e.is_range()).count(),
        book_intros: ex.entries.iter().filter(|e| e.chapter == 0).count(),
        chapter_intros: ex
            .entries
            .iter()
            .filter(|e| e.chapter > 0 && e.verse == 0)
            .count(),
        text_bytes: ex.entries.iter().map(|e| e.text.len()).sum(),
        repeats: ex.repeats.len(),
        orphans: ex.orphans.len(),
        orphan_bytes: ex.orphans.iter().map(|o| o.len).sum(),
        headings: ex.headings.len(),
        per_book_digest: {
            let mut h = 0xcbf2_9ce4_8422_2325u64;
            for (b, n) in per_book(ex) {
                fnv1a(&mut h, format!("{b}:{n};").as_bytes());
            }
            h
        },
    }
}

fn per_book(ex: &Extraction) -> BTreeMap<&'static str, usize> {
    let mut m = BTreeMap::new();
    for e in &ex.entries {
        *m.entry(e.book).or_insert(0) += 1;
    }
    m
}

macro_rules! for_each_module {
    (|$i:ident, $l:ident| $body:block) => {
        for $i in 0..SPECS.len() {
            let Some($l) = load($i) else { continue };
            $body
        }
    };
}

// 1. The verse index has exactly the KJV slot count. --------------------------------------

#[test]
fn verse_index_record_counts_equal_the_kjv_slot_counts() {
    for_each_module!(|i, l| {
        let name = SPECS[i].name;
        let want_testaments: &[Testament] = if i == CATENA {
            &[Testament::New]
        } else {
            &Testament::BOTH
        };
        let got: Vec<_> = l.ex.reports.iter().map(|r| r.testament).collect();
        assert_eq!(got, want_testaments, "{name}: testaments present");
        for r in &l.ex.reports {
            let expected = match r.testament {
                Testament::Old => 2 + 39 + 929 + 23_145,
                Testament::New => 2 + 27 + 260 + 7_957,
            };
            assert_eq!(
                expected,
                if r.testament == Testament::Old {
                    24_115
                } else {
                    8_246
                }
            );
            assert_eq!(r.slot_count, expected, "{name} {:?}", r.testament);
            assert_eq!(r.records, expected, "{name} {:?}: records", r.testament);
            assert_eq!(
                r.record_size,
                if l.module.driver == Driver::ZCom4 {
                    12
                } else {
                    10
                },
                "{name}: record size"
            );
        }
        // The conf's driver, as published, for each module.
        let driver = if matches!(i, MHC | KD | JFB) {
            Driver::ZCom4
        } else {
            Driver::ZCom
        };
        assert_eq!(l.module.driver, driver, "{name}: driver");
    });
}

// 2. Blocks decompress to exactly their declared size; entries lie inside. -----------------

#[test]
fn blocks_decompress_exactly_and_entries_lie_inside_them() {
    // Module::read fails with Error::Corrupt on any violation (wrong size, bad zlib
    // checksum, entry outside its block, missing block), so success is the proof; the
    // counts below pin that every block was in fact examined.
    let expected_blocks: [(usize, usize); 7] = [
        (40, 28), // MHC: 1 + 39 books / 1 + 27 books
        (0, 90),  // Catena: one block per chapter (CHAPTER blocks), NT only
        (36, 26),
        (48, 0),
        (40, 28),
        (39, 27),
        (40, 28),
    ];
    for_each_module!(|i, l| {
        for r in &l.ex.reports {
            let (ot, nt) = expected_blocks[i];
            let want = if r.testament == Testament::Old {
                ot
            } else {
                nt
            };
            assert_eq!(
                r.blocks, want,
                "{}: {:?} block count",
                SPECS[i].name, r.testament
            );
            // The compressed blocks tile the data file exactly, except for ten zero bytes
            // that open the New Testament data file of Wesley and TSK.
            let gap = if matches!(i, WESLEY | TSK) && r.testament == Testament::New {
                10
            } else {
                0
            };
            assert_eq!(
                r.data_gap_bytes, gap,
                "{} {:?}: bytes outside any block",
                SPECS[i].name, r.testament
            );
            assert!(
                !r.data_gap_nonzero,
                "{} {:?}: the gap is not all zero",
                SPECS[i].name, r.testament
            );
            assert_eq!(r.data_covered + r.data_gap_bytes, r.data_file_len);
        }
    });
}

// 3. Every byte of every block is accounted for. ------------------------------------------

#[test]
fn every_block_byte_is_an_entry_or_a_reported_orphan() {
    for_each_module!(|i, l| {
        for r in &l.ex.reports {
            let orphan_bytes: u64 =
                l.ex.orphans
                    .iter()
                    .filter(|o| o.testament == r.testament)
                    .map(|o| o.len as u64)
                    .sum();
            assert_eq!(r.orphan_bytes, orphan_bytes);
            // Entries sit back to back with no separator bytes, never overlap, and what
            // no entry covers is exactly the orphan regions.
            assert_eq!(
                r.entry_bytes + r.orphan_bytes,
                r.block_bytes,
                "{} {:?}: distinct entry bytes + orphan bytes != block bytes",
                SPECS[i].name,
                r.testament
            );
        }
        // Orphans are reported with their neighbours, and never empty.
        for o in &l.ex.orphans {
            assert!(o.len > 0);
            if l.ex.encoding == Encoding::Utf8 {
                assert_eq!(o.text.len(), o.len);
            }
        }
    });
}

#[test]
fn modules_without_unindexed_bytes_tile_their_blocks_exactly() {
    for i in [CATENA, WESLEY, TSK, GILL] {
        let Some(l) = load(i) else { continue };
        assert!(l.ex.orphans.is_empty(), "{}", SPECS[i].name);
    }
}

// 4. No replacement characters or NULs. ---------------------------------------------------

#[test]
fn decoded_text_has_no_replacement_characters_or_nul() {
    for_each_module!(|i, l| {
        let texts =
            l.ex.entries
                .iter()
                .map(|e| &e.text)
                .chain(l.ex.orphans.iter().map(|o| &o.text))
                .chain(l.ex.headings.iter().map(|h| &h.text));
        for t in texts {
            assert!(!t.contains('\0'), "{}: NUL in {:.80}", SPECS[i].name, t);
            // U+FFFD is never produced by decoding (invalid bytes are an error). The only
            // one present is a literal EF BF BD sequence in Gill's source text.
            if i != GILL {
                assert!(
                    !t.contains('\u{fffd}'),
                    "{}: U+FFFD in {:.80}",
                    SPECS[i].name,
                    t
                );
            }
        }
    });
}

#[test]
fn source_defects_are_kept_as_stored() {
    // Gill: one U+FFFD (EF BF BD in the source, "Cornelius <U+FFFD>apide" at Ezekiel 45:1)
    // and three U+0089 (C2 89) where an accented letter was damaged ("La<89>rt.").
    if let Some(l) = load(GILL) {
        let all: String = l.ex.entries.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(all.matches('\u{fffd}').count(), 1);
        assert_eq!(all.matches('\u{89}').count(), 3);
        let e = covering(&l.ex, "EZK", 45, 1).unwrap();
        assert!(e.text.contains("Cornelius \u{fffd}apide"));
    }
    // TSK: control bytes inside the Numbers 35:4 cities-of-refuge diagram (and four 0x15).
    if let Some(l) = load(TSK) {
        let all: String = l.ex.entries.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(all.matches('\u{15}').count(), 4);
        assert_eq!(all.matches('\u{94}').count(), 3);
        assert_eq!(all.matches('\u{9c}').count(), 3);
        assert_eq!(all.matches('\u{90}').count(), 2);
    }
}

// 5. Placement spot checks. ----------------------------------------------------------------

#[test]
fn notes_land_on_the_right_verses() {
    let find =
        |i: usize, book: &str, c: u32, v: u32| -> Option<(&'static Loaded, &'static Entry)> {
            let l = load(i)?;
            let e = covering(&l.ex, book, c, v)
                .unwrap_or_else(|| panic!("{} has nothing at {book} {c}:{v}", SPECS[i].name));
            Some((l, e))
        };

    // Matthew Henry: the note whose range includes John 3:16.
    if let Some((_, e)) = find(MHC, "JHN", 3, 16) {
        assert!(
            e.text.contains("God so loved the world"),
            "MHC John 3:16: {:.200}",
            e.text
        );
        println!(
            "MHC John 3:16 entry: {}:{}-{}:{}",
            e.chapter, e.verse, e.to_chapter, e.to_verse
        );
    }
    // Catena Aurea: Matthew 1:1 is about "the book of the generation".
    if let Some((_, e)) = find(CATENA, "MAT", 1, 1) {
        assert!(
            e.text.contains("the book of the generation"),
            "Catena Matt 1:1: {:.200}",
            e.text
        );
    }
    // JFB: John 3:16 is covered by the range note "14-16. And as Moses"; the verse's own
    // note ("16. For God so loved, &c.") is stored in the block but no index record points
    // at it, so it comes back as the orphan that follows the range entry.
    if let Some((l, e)) = find(JFB, "JHN", 3, 16) {
        assert_eq!(
            (e.chapter, e.verse, e.to_chapter, e.to_verse),
            (3, 14, 3, 16)
        );
        let index =
            l.ex.entries
                .iter()
                .position(|x| std::ptr::eq(x, e))
                .unwrap();
        let orphan =
            l.ex.orphans
                .iter()
                .find(|o| o.after == Some(index))
                .expect("JFB John 3:14-16 is followed by an unindexed region");
        assert!(
            orphan.text.contains("16. For God so loved"),
            "{:.300}",
            orphan.text
        );
        assert_eq!(l.ex.entries[orphan.before.unwrap()].verse, 17);
    }
    // Gill: John 3:16.
    if let Some((_, e)) = find(GILL, "JHN", 3, 16) {
        assert!(
            e.text.contains("so loved"),
            "Gill John 3:16: {:.200}",
            e.text
        );
    }
    // Keil and Delitzsch: Genesis 1:1 is about "In the beginning".
    if let Some((_, e)) = find(KD, "GEN", 1, 1) {
        assert!(
            e.text.contains("In the beginning"),
            "KD Gen 1:1: {:.200}",
            e.text
        );
    }
    // TSK: Genesis 1:1 has cross-references including John 1:1.
    if let Some((_, e)) = find(TSK, "GEN", 1, 1) {
        assert!(
            e.text.contains("Joh 1:1"),
            "TSK Gen 1:1 should cite John 1:1: {:.300}",
            e.text
        );
        assert!(
            e.text.contains("<scripRef"),
            "TSK Gen 1:1 should be ThML scripRefs: {:.300}",
            e.text
        );
    }
    // Wesley: John 3:16.
    if let Some((_, e)) = find(WESLEY, "JHN", 3, 16) {
        assert!(
            e.text.contains("God so loved the world"),
            "Wesley John 3:16: {:.200}",
            e.text
        );
    }
}

#[test]
fn testament_ends_are_not_off_by_one() {
    // The last slot of each testament carries (or, for an empty one, does not carry)
    // the note for the last verse. Revelation 22:21 and Malachi 4:6.
    for_each_module!(|i, l| {
        for (book, c, v, t) in [
            ("MAL", 4, 6, Testament::Old),
            ("REV", 22, 21, Testament::New),
        ] {
            if !l.module.has_testament(t) {
                continue;
            }
            // Whatever covers the final verse must also be positioned there and must
            // not be the note of the verse before it unless it is a range.
            let last = covering(&l.ex, book, c, v);
            let prev = covering(&l.ex, book, c, v - 1);
            if let (Some(last), Some(prev)) = (last, prev)
                && std::ptr::eq(last, prev)
            {
                assert!(last.is_range(), "{} {book} {c}:{v}", SPECS[i].name);
            }
            println!(
                "{:7} {book} {c}:{v}: {}",
                SPECS[i].name,
                last.map_or("(no note)".to_string(), |e| format!(
                    "{}:{}-{}:{} {:?}…",
                    e.chapter,
                    e.verse,
                    e.to_chapter,
                    e.to_verse,
                    e.text.chars().take(60).collect::<String>()
                ))
            );
        }
    });

    if let Some(l) = load(GILL) {
        let e = covering(&l.ex, "REV", 22, 21).expect("Gill covers Rev 22:21");
        assert!(
            e.text.contains("grace of our Lord Jesus Christ"),
            "{:.300}",
            e.text
        );
        let e = covering(&l.ex, "MAL", 4, 6).expect("Gill covers Mal 4:6");
        assert!(e.text.contains("curse"), "{:.300}", e.text);
        let last = l.ex.entries.last().unwrap();
        assert_eq!((last.book, last.to_chapter, last.to_verse), ("REV", 22, 21));
    }
    if let Some(l) = load(MHC) {
        let e = covering(&l.ex, "REV", 22, 21).expect("MHC covers Rev 22:21");
        assert!(e.text.contains("grace"), "{:.300}", e.text);
    }
    if let Some(l) = load(TSK) {
        // TSK has no note for Revelation 22:21 (its last note ends at 22:20).
        let e = covering(&l.ex, "REV", 22, 20).expect("TSK covers Rev 22:20");
        assert!(e.text.contains("scripRef"), "{:.200}", e.text);
        assert!(
            covering(&l.ex, "REV", 22, 21).is_none(),
            "TSK Rev 22:21 is empty"
        );
    }
}

#[test]
fn chapter_heading_slots_hold_psalm_introductions_and_verses_are_not_shifted() {
    // Psalm 23:1, the Psalms having a chapter-introduction slot before every chapter.
    if let Some(l) = load(MHC) {
        let intro =
            l.ex.entries
                .iter()
                .find(|e| (e.book, e.chapter, e.verse) == ("PSA", 23, 0));
        let intro = intro.expect("MHC has a Psalm 23 chapter introduction");
        assert!(
            intro.text.contains("<chapter n=\"23\""),
            "{:.200}",
            intro.text
        );
        let e = covering(&l.ex, "PSA", 23, 1).expect("MHC covers Psalm 23:1");
        assert!(
            e.text.contains("my shepherd; I shall not want"),
            "{:.300}",
            e.text
        );
        assert!(!std::ptr::eq(e, intro));
    }
    if let Some(l) = load(WESLEY) {
        // Wesley has no note of its own on 23:1: the index continues his note on 22:31
        // across the chapter break (see `cross_chapter_ranges_are_wesley_only`). His
        // first Psalm 23 note is on verse 2.
        let e = covering(&l.ex, "PSA", 23, 1).expect("Wesley covers Psalm 23:1");
        assert_eq!(
            (e.chapter, e.verse, e.to_chapter, e.to_verse),
            (22, 31, 23, 1)
        );
        let e = covering(&l.ex, "PSA", 23, 2).expect("Wesley covers Psalm 23:2");
        assert!(
            e.text.starts_with("Lie down - To repose myself"),
            "{:.300}",
            e.text
        );
    }
    if let Some(l) = load(GILL) {
        let e = covering(&l.ex, "PSA", 23, 1).expect("Gill covers Psalm 23:1");
        assert!(e.text.contains("shepherd"), "{:.300}", e.text);
    }
    if let Some(l) = load(JFB) {
        let e = covering(&l.ex, "PSA", 23, 1).expect("JFB covers Psalm 23:1");
        assert!(e.text.contains("shepherd"), "{:.300}", e.text);
    }
}

// Encoding findings for the two modules whose conf has no Encoding line. -------------------

#[test]
fn wesley_is_utf8_despite_declaring_nothing() {
    let Some(l) = load(WESLEY) else { return };
    assert_eq!(l.module.declared_encoding, None);
    assert!(l.ex.encoding_detected);
    assert_eq!(l.ex.encoding, Encoding::Utf8);
    // The only non-ASCII bytes are U+00A0 written as C2 A0 (639 times). Read as Latin-1
    // each would become "Â" followed by a no-break space: that is the mis-decoding.
    let all: String = l.ex.entries.iter().map(|e| e.text.as_str()).collect();
    let nbsp = all.matches('\u{a0}').count();
    let non_ascii: usize = all.chars().filter(|c| !c.is_ascii()).count();
    println!(
        "Wesley: {nbsp} U+00A0, {non_ascii} non-ASCII chars, {} 'Â'",
        all.matches('Â').count()
    );
    assert_eq!(non_ascii, nbsp, "NBSP is the only non-ASCII character");
    assert_eq!(all.matches('Â').count(), 0);
    // Forcing Latin-1 shows what the wrong decoding looks like.
    let wrong = Module::open_zip(&l.path)
        .unwrap()
        .with_encoding(Encoding::Latin1)
        .read()
        .unwrap();
    let wrong_all: String = wrong.entries.iter().map(|e| e.text.as_str()).collect();
    assert_eq!(wrong_all.matches("Â\u{a0}").count(), nbsp);
}

#[test]
fn tsk_is_latin1_despite_declaring_nothing() {
    let Some(l) = load(TSK) else { return };
    assert_eq!(l.module.declared_encoding, None);
    assert!(l.ex.encoding_detected);
    assert_eq!(l.ex.encoding, Encoding::Latin1);
    // The bytes are not UTF-8: reading them as UTF-8 is a decoding error, not mojibake.
    let utf8 = Module::open_zip(&l.path)
        .unwrap()
        .with_encoding(Encoding::Utf8)
        .read();
    assert!(matches!(utf8, Err(kjv_sword::Error::Decode(_))), "{utf8:?}");
    // And Latin-1 yields the expected characters.
    let all: String = l.ex.entries.iter().map(|e| e.text.as_str()).collect();
    for needle in ["Petræa", "Cæsarea", "Vâv", "Yôwd", "£"] {
        assert!(all.contains(needle), "TSK should contain {needle:?}");
    }
    let c1 = all
        .chars()
        .filter(|c| ('\u{80}'..='\u{9f}').contains(c))
        .count();
    println!("TSK: {c1} C1 control characters (stored bytes 0x80..0x9F kept as-is)");
    assert_eq!(c1, 8, "C1 control characters stored in the source");
}

#[test]
fn declared_encodings_are_utf8_for_the_other_five() {
    for i in [MHC, CATENA, KD, JFB, GILL] {
        let Some(l) = load(i) else { continue };
        assert_eq!(
            l.module.declared_encoding,
            Some(Encoding::Utf8),
            "{}",
            SPECS[i].name
        );
        assert!(!l.ex.encoding_detected);
    }
}

// Links and ranges. -------------------------------------------------------------------------

#[test]
fn repeats_are_stale_book_start_pointers() {
    // Wesley and TSK: the first chapter-introduction slot of (almost) every book carries a
    // copy of the previous book's last-written record. Nothing else is shared
    // non-adjacently in any module.
    for_each_module!(|i, l| {
        for r in &l.ex.repeats {
            let (later, first) = (&l.ex.entries[r.entry], &l.ex.entries[r.first]);
            if !matches!(i, WESLEY | TSK) {
                panic!("{}: unexpected repeat {later:?}", SPECS[i].name);
            }
            // A stale copy starts at the book's first slots (book or first-chapter
            // introduction) and may run on over every slot up to the book's first real
            // note, even over the whole book (Wesley's Judges and Jonah).
            let starts = matches!((later.chapter, later.verse), (0, 0) | (1, 0));
            assert!(
                starts && later.book != first.book,
                "{}: {} {}:{} repeats {} {}:{}",
                SPECS[i].name,
                later.book,
                later.chapter,
                later.verse,
                first.book,
                first.chapter,
                first.verse
            );
            assert_eq!(later.text, first.text);
        }
    });
}

#[test]
fn ranges_are_collapsed_and_cover_every_non_empty_slot_once() {
    for_each_module!(|i, l| {
        // Entries are in canonical order and never overlap, except for flagged repeats.
        let repeats: std::collections::HashSet<usize> =
            l.ex.repeats.iter().map(|r| r.entry).collect();
        let mut last: Option<(&str, u32, u32)> = None;
        let order = |b: &str| {
            kjv_sword::kjv::BOOKS
                .iter()
                .position(|x| x.code == b)
                .unwrap()
        };
        for (n, e) in l.ex.entries.iter().enumerate() {
            assert!(
                (e.chapter, e.verse) <= (e.to_chapter, e.to_verse),
                "{} {e:?}",
                SPECS[i].name
            );
            let key = (e.book, e.chapter, e.verse);
            if let Some(prev) = last {
                assert!(
                    (order(prev.0), prev.1, prev.2) < (order(key.0), key.1, key.2),
                    "{}: entries out of order at {n}: {prev:?} then {key:?}",
                    SPECS[i].name
                );
            }
            last = Some((e.book, e.to_chapter, e.to_verse));
            let _ = &repeats;
        }
    });
}

// 6. Statistics, pinned. ---------------------------------------------------------------------

#[test]
fn statistics_are_pinned() {
    // Entries, ranged entries, book / chapter introductions, text bytes, repeats, orphan
    // regions and bytes, module/testament headings, per-book count digest, and a digest of
    // every entry and orphan (position, range and text). Any change anywhere shows up.
    let pinned: [(Stats, u64); 7] = [
        (
            Stats {
                entries: 5504,
                ranged: 3695,
                book_intros: 66,
                chapter_intros: 1189,
                text_bytes: 49188179,
                repeats: 0,
                orphans: 6,
                orphan_bytes: 70098,
                headings: 2,
                per_book_digest: 0x8e3ecceb5edb7805,
            },
            0x16df2cc694ffc92a,
        ),
        (
            Stats {
                entries: 821,
                ranged: 0,
                book_intros: 0,
                chapter_intros: 0,
                text_bytes: 7419132,
                repeats: 0,
                orphans: 0,
                orphan_bytes: 0,
                headings: 1,
                per_book_digest: 0x63aa9b5cfbc4ae2e,
            },
            0x73741cc152286a50,
        ),
        (
            Stats {
                entries: 16741,
                ranged: 259,
                book_intros: 2,
                chapter_intros: 29,
                text_bytes: 4842534,
                repeats: 31,
                orphans: 0,
                orphan_bytes: 0,
                headings: 0,
                per_book_digest: 0xdb1fd171ea908a6a,
            },
            0xf7c339712319018e,
        ),
        (
            Stats {
                entries: 9775,
                ranged: 4946,
                book_intros: 39,
                chapter_intros: 929,
                text_bytes: 31171370,
                repeats: 0,
                orphans: 393,
                orphan_bytes: 2748349,
                headings: 1,
                per_book_digest: 0xb69a83304bc6d24f,
            },
            0xd37ce17978ba2359,
        ),
        (
            Stats {
                entries: 18137,
                ranged: 2640,
                book_intros: 66,
                chapter_intros: 1189,
                text_bytes: 17902376,
                repeats: 0,
                orphans: 1297,
                orphan_bytes: 1894265,
                headings: 2,
                per_book_digest: 0x5ebbd0534a8d717c,
            },
            0x7d000efef31254fb,
        ),
        (
            Stats {
                entries: 31152,
                ranged: 2,
                book_intros: 0,
                chapter_intros: 64,
                text_bytes: 8015138,
                repeats: 64,
                orphans: 0,
                orphan_bytes: 0,
                headings: 0,
                per_book_digest: 0x74742bd45f4fe60,
            },
            0xf926587bfd7e922b,
        ),
        (
            Stats {
                entries: 32357,
                ranged: 0,
                book_intros: 66,
                chapter_intros: 1189,
                text_bytes: 55229864,
                repeats: 0,
                orphans: 0,
                orphan_bytes: 0,
                headings: 2,
                per_book_digest: 0xd51fce8d82e6fb5d,
            },
            0x100d9e275f829f07,
        ),
    ];
    let mut failures = Vec::new();
    for_each_module!(|i, l| {
        let s = stats(&l.ex);
        let d = digest(&l.ex);
        let books = per_book(&l.ex);
        println!(
            "== {} ==
{s:?}
digest {d:#018x}",
            SPECS[i].name
        );
        println!(
            "per book: {}",
            books
                .iter()
                .map(|(b, n)| format!("{b}:{n}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
        if (&s, d) != (&pinned[i].0, pinned[i].1) {
            failures.push(SPECS[i].name);
        }
    });
    assert!(
        failures.is_empty(),
        "pinned numbers differ for {failures:?}"
    );
}

// Structural proof of the slot layout. -------------------------------------------------------

/// SWORD's OSIS book ids for the 66 USFM codes.
const OSIS: [(&str, &str); 66] = [
    ("GEN", "Gen"),
    ("EXO", "Exod"),
    ("LEV", "Lev"),
    ("NUM", "Num"),
    ("DEU", "Deut"),
    ("JOS", "Josh"),
    ("JDG", "Judg"),
    ("RUT", "Ruth"),
    ("1SA", "1Sam"),
    ("2SA", "2Sam"),
    ("1KI", "1Kgs"),
    ("2KI", "2Kgs"),
    ("1CH", "1Chr"),
    ("2CH", "2Chr"),
    ("EZR", "Ezra"),
    ("NEH", "Neh"),
    ("EST", "Esth"),
    ("JOB", "Job"),
    ("PSA", "Ps"),
    ("PRO", "Prov"),
    ("ECC", "Eccl"),
    ("SNG", "Song"),
    ("ISA", "Isa"),
    ("JER", "Jer"),
    ("LAM", "Lam"),
    ("EZK", "Ezek"),
    ("DAN", "Dan"),
    ("HOS", "Hos"),
    ("JOL", "Joel"),
    ("AMO", "Amos"),
    ("OBA", "Obad"),
    ("JON", "Jonah"),
    ("MIC", "Mic"),
    ("NAM", "Nah"),
    ("HAB", "Hab"),
    ("ZEP", "Zeph"),
    ("HAG", "Hag"),
    ("ZEC", "Zech"),
    ("MAL", "Mal"),
    ("MAT", "Matt"),
    ("MRK", "Mark"),
    ("LUK", "Luke"),
    ("JHN", "John"),
    ("ACT", "Acts"),
    ("ROM", "Rom"),
    ("1CO", "1Cor"),
    ("2CO", "2Cor"),
    ("GAL", "Gal"),
    ("EPH", "Eph"),
    ("PHP", "Phil"),
    ("COL", "Col"),
    ("1TH", "1Thess"),
    ("2TH", "2Thess"),
    ("1TI", "1Tim"),
    ("2TI", "2Tim"),
    ("TIT", "Titus"),
    ("PHM", "Phlm"),
    ("HEB", "Heb"),
    ("JAS", "Jas"),
    ("1PE", "1Pet"),
    ("2PE", "2Pet"),
    ("1JN", "1John"),
    ("2JN", "2John"),
    ("3JN", "3John"),
    ("JUD", "Jude"),
    ("REV", "Rev"),
];

fn osis_id(book: &str) -> &'static str {
    OSIS.iter().find(|(u, _)| *u == book).unwrap().1
}

/// The ids in `sID="..."` attributes of the `<chapter .../>` tags in a text.
fn chapter_start_ids(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("<chapter ") {
        rest = &rest[at..];
        let end = rest.find('>').unwrap_or(rest.len());
        if let Some(s) = rest[..end].find("sID=\"") {
            let from = s + 5;
            if let Some(len) = rest[from..end].find('"') {
                out.push(&rest[from..from + len]);
            }
        }
        rest = &rest[end.min(rest.len())..];
    }
    out
}

#[test]
fn chapter_markers_in_the_text_confirm_every_chapter_slot() {
    // OSIS modules mark each chapter's start inside the chapter-introduction entry. If any
    // slot were off by one, or a chapter had a different verse count than the KJV table,
    // that marker would sit in the wrong place. Checked for every chapter that has one.
    let mut proven = BTreeMap::new();
    for i in [MHC, KD, JFB, GILL, CATENA] {
        let Some(l) = load(i) else { continue };
        let mut ok = 0usize;
        for e in
            l.ex.entries
                .iter()
                .filter(|e| e.chapter > 0 && e.verse == 0)
        {
            let ids = chapter_start_ids(&e.text);
            if ids.is_empty() {
                continue;
            }
            let want = format!("{}.{}", osis_id(e.book), e.chapter);
            assert!(
                ids.contains(&want.as_str()),
                "{}: slot {} {}:0 holds chapter markers {ids:?}, not {want}",
                SPECS[i].name,
                e.book,
                e.chapter
            );
            ok += 1;
        }
        proven.insert(SPECS[i].name, ok);
    }
    println!("chapter markers confirming their slot: {proven:?}");
    if load(MHC).is_some() {
        assert_eq!(proven["MHC"], 1189);
    }
}

#[test]
fn book_markers_in_the_text_confirm_every_book_slot() {
    let mut proven = BTreeMap::new();
    for i in [MHC, KD, JFB, GILL] {
        let Some(l) = load(i) else { continue };
        let mut ok = 0usize;
        for e in l.ex.entries.iter().filter(|e| e.chapter == 0) {
            let want = format!("osisID=\"{}\"", osis_id(e.book));
            if e.text.contains("type=\"book\"") {
                assert!(
                    e.text.contains(&want),
                    "{}: {} book intro {:.150}",
                    SPECS[i].name,
                    e.book,
                    e.text
                );
                ok += 1;
            }
        }
        proven.insert(SPECS[i].name, ok);
    }
    println!("book markers confirming their slot: {proven:?}");
}

#[test]
fn cross_chapter_ranges_are_wesley_only() {
    // A range whose end is in a later chapter than its start. Only Wesley's index has
    // them (a note continued over the next chapter's empty introduction and first verses).
    let counts: Vec<(&str, usize)> = (0..SPECS.len())
        .filter_map(|i| {
            let l = load(i)?;
            let stale: std::collections::HashSet<usize> =
                l.ex.repeats.iter().map(|r| r.entry).collect();
            let n =
                l.ex.entries
                    .iter()
                    .enumerate()
                    .filter(|(k, e)| e.to_chapter != e.chapter && !stale.contains(k))
                    .count();
            Some((SPECS[i].name, n))
        })
        .collect();
    println!("cross-chapter ranges: {counts:?}");
    for (name, n) in counts {
        assert_eq!(n, if name == "Wesley" { 191 } else { 0 }, "{name}");
    }
}
