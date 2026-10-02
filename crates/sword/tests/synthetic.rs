//! Tests on small modules built in memory, so the reader's rules are checked without the
//! cached CrossWire downloads: slot layout, range collapsing, repeats, orphans, encodings
//! and every refusal.

use std::io::Write;
use std::path::PathBuf;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use kjv_sword::{Driver, Encoding, Error, Module, Slot, Testament};

type Rec = (u32, u32, u32);

/// A testament being assembled: one record per slot plus the decompressed blocks.
struct Builder {
    testament: Testament,
    slots: Vec<Slot>,
    recs: Vec<Rec>,
    blocks: Vec<Vec<u8>>,
}

impl Builder {
    fn new(testament: Testament) -> Builder {
        Builder {
            testament,
            slots: testament.slots(),
            recs: vec![(0, 0, 0); testament.slot_count()],
            blocks: vec![Vec::new()],
        }
    }

    fn index(&self, book: &str, chapter: u32, verse: u32) -> usize {
        let at = self
            .slots
            .iter()
            .position(|s| (s.book, s.chapter, s.verse) == (book, chapter, verse))
            .unwrap_or_else(|| panic!("no slot {book} {chapter}:{verse}"));
        at + kjv_sword::kjv::HEADING_SLOTS
    }

    fn block(&mut self, n: usize) {
        while self.blocks.len() <= n {
            self.blocks.push(Vec::new());
        }
    }

    /// Appends `bytes` to `block` and returns the record that points at them.
    fn append(&mut self, block: usize, bytes: &[u8]) -> Rec {
        self.block(block);
        let offset = self.blocks[block].len() as u32;
        self.blocks[block].extend_from_slice(bytes);
        (block as u32, offset, bytes.len() as u32)
    }

    /// Stores `text` once and points every listed slot at it.
    fn note(&mut self, block: usize, text: &[u8], covers: &[(&str, u32, u32)]) -> Rec {
        let rec = self.append(block, text);
        for &(b, c, v) in covers {
            let at = self.index(b, c, v);
            self.recs[at] = rec;
        }
        rec
    }

    /// Points a slot at an existing record.
    fn link(&mut self, rec: Rec, slot: (&str, u32, u32)) {
        let at = self.index(slot.0, slot.1, slot.2);
        self.recs[at] = rec;
    }

    /// Bytes in a block that no record points at.
    fn stray(&mut self, block: usize, bytes: &[u8]) {
        self.append(block, bytes);
    }
}

struct Options {
    /// Write the files in the 12-byte zCom4 layout.
    zcom4: bool,
    /// What the conf's `ModDrv` says, when it should differ from the files.
    conf_driver: Option<&'static str>,
    block_letter: char,
    extra_conf: &'static str,
    name: &'static str,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            zcom4: false,
            conf_driver: None,
            block_letter: 'b',
            extra_conf: "Encoding=UTF-8\n",
            name: "t",
        }
    }
}

fn zlib(bytes: &[u8]) -> Vec<u8> {
    let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
    e.write_all(bytes).unwrap();
    e.finish().unwrap()
}

/// Serialises a testament to its three files. `corrupt` can alter the pieces.
fn files(b: &Builder, zcom4: bool) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let (mut zs, mut zv, mut zz) = (Vec::new(), Vec::new(), Vec::new());
    for block in &b.blocks {
        let packed = zlib(block);
        zs.extend((zz.len() as u32).to_le_bytes());
        zs.extend((packed.len() as u32).to_le_bytes());
        zs.extend((block.len() as u32).to_le_bytes());
        zz.extend(packed);
    }
    for &(block, offset, size) in &b.recs {
        zv.extend(block.to_le_bytes());
        zv.extend(offset.to_le_bytes());
        if zcom4 {
            zv.extend(size.to_le_bytes());
        } else {
            zv.extend(
                u16::try_from(size)
                    .expect("zCom size is 16 bits")
                    .to_le_bytes(),
            );
        }
    }
    (zs, zv, zz)
}

fn write_zip(path: &PathBuf, conf: &str, entries: &[(String, Vec<u8>)]) {
    let file = std::fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("mods.d/t.conf", opts).unwrap();
    zip.write_all(conf.as_bytes()).unwrap();
    for (name, data) in entries {
        zip.start_file(name.as_str(), opts).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap();
}

fn temp(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

/// Builds a module zip containing the given testaments and opens it.
fn module(file: &str, opts: &Options, built: &[&Builder]) -> Result<Module, Error> {
    module_with(file, opts, built, |_| {})
}

/// Like [`module`], letting a test damage the files before they are zipped.
fn module_with(
    file: &str,
    opts: &Options,
    built: &[&Builder],
    damage: impl Fn(&mut Vec<(String, Vec<u8>)>),
) -> Result<Module, Error> {
    // `extra_conf` comes first so that a test can override a standard line (the first
    // occurrence of a key wins).
    let conf = format!(
        "[{}]\n{}DataPath=./modules/comments/zcom/t/\nModDrv={}\nCompressType=ZIP\nBlockType={}\n",
        opts.name,
        opts.extra_conf,
        opts.conf_driver
            .unwrap_or(if opts.zcom4 { "zCom4" } else { "zCom" }),
        if opts.block_letter == 'b' {
            "BOOK"
        } else {
            "CHAPTER"
        },
    );
    let mut entries = Vec::new();
    for b in built {
        let (zs, zv, zz) = files(b, opts.zcom4);
        let stem = b.testament.file_stem();
        let l = opts.block_letter;
        entries.push((format!("modules/comments/zcom/t/{stem}.{l}zs"), zs));
        entries.push((format!("modules/comments/zcom/t/{stem}.{l}zv"), zv));
        entries.push((format!("modules/comments/zcom/t/{stem}.{l}zz"), zz));
    }
    damage(&mut entries);
    let path = temp(file);
    write_zip(&path, &conf, &entries);
    Module::open_zip(&path)
}

fn ot() -> Builder {
    Builder::new(Testament::Old)
}

#[test]
fn reads_single_verses_ranges_and_introductions_in_canonical_order() {
    let mut b = ot();
    // Stored out of canonical order on purpose: the reader must follow the index.
    b.note(1, b"Gen 1:1 note", &[("GEN", 1, 1)]);
    b.note(
        1,
        b" leading and trailing space & &amp; entity ",
        &[("GEN", 1, 3), ("GEN", 1, 4), ("GEN", 1, 5)],
    );
    b.note(0, b"book intro", &[("GEN", 0, 0)]);
    b.note(0, b"chapter 2 intro", &[("GEN", 2, 0)]);
    b.note(2, b"Exodus 1:1", &[("EXO", 1, 1)]);
    let m = module("basic.zip", &Options::default(), &[&b]).unwrap();
    assert_eq!(m.name, "t");
    assert_eq!(m.driver, Driver::ZCom);
    let ex = m.read().unwrap();
    let got: Vec<_> = ex
        .entries
        .iter()
        .map(|e| {
            (
                e.book,
                e.chapter,
                e.verse,
                e.to_chapter,
                e.to_verse,
                e.text.as_str(),
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![
            ("GEN", 0, 0, 0, 0, "book intro"),
            ("GEN", 1, 1, 1, 1, "Gen 1:1 note"),
            (
                "GEN",
                1,
                3,
                1,
                5,
                " leading and trailing space & &amp; entity "
            ),
            ("GEN", 2, 0, 2, 0, "chapter 2 intro"),
            ("EXO", 1, 1, 1, 1, "Exodus 1:1"),
        ]
    );
    assert!(ex.repeats.is_empty() && ex.orphans.is_empty() && ex.headings.is_empty());
    assert_eq!(ex.entries.len(), m.entries().unwrap().len());
    let r = &ex.reports[0];
    assert_eq!(
        (r.testament, r.records, r.slot_count, r.record_size),
        (Testament::Old, 24_115, 24_115, 10)
    );
    assert_eq!(r.empty_slots, 24_115 - 7); // seven slots carry a note
    assert_eq!(r.entry_bytes + r.orphan_bytes, r.block_bytes);
    assert!(!m.has_testament(Testament::New));
}

#[test]
fn headings_are_returned_separately() {
    let mut b = ot();
    let rec = b.append(0, b"<milestone/>");
    b.recs[1] = rec;
    let rec = b.append(0, b"module heading");
    b.recs[0] = rec;
    b.note(1, b"x", &[("GEN", 1, 1)]);
    let ex = module("headings.zip", &Options::default(), &[&b])
        .unwrap()
        .read()
        .unwrap();
    assert_eq!(ex.entries.len(), 1);
    let h: Vec<_> = ex
        .headings
        .iter()
        .map(|h| (format!("{:?}", h.kind), h.text.as_str()))
        .collect();
    assert_eq!(
        h,
        [
            ("Module".to_string(), "module heading"),
            ("Testament".to_string(), "<milestone/>")
        ]
    );
}

#[test]
fn slot_layout_is_modules_then_testament_then_book_chapter_verse() {
    // Genesis 1:1 is record 4: module heading, testament heading, Genesis intro, chapter
    // 1 intro, then verse 1. Genesis 2:0 follows 1:31, Exodus 0:0 follows 50:26.
    let b = ot();
    assert_eq!(b.index("GEN", 0, 0), 2);
    assert_eq!(b.index("GEN", 1, 0), 3);
    assert_eq!(b.index("GEN", 1, 1), 4);
    assert_eq!(b.index("GEN", 2, 0), b.index("GEN", 1, 31) + 1);
    assert_eq!(b.index("EXO", 0, 0), b.index("GEN", 50, 26) + 1);
    assert_eq!(b.index("MAL", 4, 6), 24_115 - 1);
    let nt = Builder::new(Testament::New);
    assert_eq!(nt.index("REV", 22, 21), 8_246 - 1);
    assert_eq!(nt.index("MAT", 1, 1), 4);
}

#[test]
fn adjacent_slots_collapse_across_empty_chapter_introductions() {
    let mut b = ot();
    // One note over 12:20, (empty 13:0), 13:1, 13:2. It collapses into one cross-chapter
    // range because the only slot in between is an empty introduction.
    b.note(
        0,
        b"spans the break",
        &[("GEN", 12, 20), ("GEN", 13, 1), ("GEN", 13, 2)],
    );
    // A note covering a chapter's introduction slot and its verses is one entry from 5:0.
    b.note(
        0,
        b"whole chapter",
        &[("GEN", 5, 0), ("GEN", 5, 1), ("GEN", 5, 2)],
    );
    let ex = module("bridge.zip", &Options::default(), &[&b])
        .unwrap()
        .read()
        .unwrap();
    let got: Vec<_> = ex
        .entries
        .iter()
        .map(|e| (e.chapter, e.verse, e.to_chapter, e.to_verse))
        .collect();
    assert_eq!(got, [(5, 0, 5, 2), (12, 20, 13, 2)]);
    assert!(ex.repeats.is_empty());
}

#[test]
fn a_non_empty_slot_in_between_breaks_a_range_and_is_reported_as_a_repeat() {
    let mut b = ot();
    let shared = b.note(0, b"shared", &[("GEN", 1, 1), ("GEN", 1, 4)]);
    b.note(0, b"between", &[("GEN", 1, 2)]);
    // Same record again after a different note, in another chapter, and in another book.
    b.link(shared, ("GEN", 2, 1));
    b.link(shared, ("EXO", 1, 0));
    let ex = module("repeat.zip", &Options::default(), &[&b])
        .unwrap()
        .read()
        .unwrap();
    let got: Vec<_> = ex
        .entries
        .iter()
        .map(|e| {
            (
                e.book,
                e.chapter,
                e.verse,
                e.to_chapter,
                e.to_verse,
                e.text.as_str(),
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            ("GEN", 1, 1, 1, 1, "shared"),
            ("GEN", 1, 2, 1, 2, "between"),
            ("GEN", 1, 4, 1, 4, "shared"),
            ("GEN", 2, 1, 2, 1, "shared"),
            ("EXO", 1, 0, 1, 0, "shared"),
        ]
    );
    // Nothing is dropped, but every later copy is flagged against the first.
    let flagged: Vec<_> = ex.repeats.iter().map(|r| (r.entry, r.first)).collect();
    assert_eq!(flagged, [(2, 0), (3, 0), (4, 0)]);
    assert_eq!(ex.distinct_entries().len(), 2);
}

#[test]
fn a_range_never_crosses_a_book() {
    let mut b = ot();
    b.note(
        0,
        b"last",
        &[("GEN", 50, 26), ("EXO", 0, 0), ("EXO", 1, 0), ("EXO", 1, 1)],
    );
    let ex = module("book.zip", &Options::default(), &[&b])
        .unwrap()
        .read()
        .unwrap();
    let got: Vec<_> = ex
        .entries
        .iter()
        .map(|e| (e.book, e.chapter, e.verse, e.to_chapter, e.to_verse))
        .collect();
    assert_eq!(got, [("GEN", 50, 26, 50, 26), ("EXO", 0, 0, 1, 1)]);
    assert_eq!(ex.repeats.len(), 1);
}

#[test]
fn unreferenced_block_bytes_come_back_as_orphans_with_their_neighbours() {
    let mut b = ot();
    b.note(1, b"AAAA", &[("GEN", 1, 1), ("GEN", 1, 2)]); // offsets 0..4
    b.stray(1, b"<orphan one/>"); // 4..17
    b.note(1, b"BBBB", &[("GEN", 1, 3)]); // 17..21
    b.stray(1, b"tail"); // 21..25, runs to the end of the block
    b.stray(2, b"whole block"); // a block nothing points at
    let ex = module("orphan.zip", &Options::default(), &[&b])
        .unwrap()
        .read()
        .unwrap();
    assert_eq!(ex.entries.len(), 2);
    let o: Vec<_> = ex
        .orphans
        .iter()
        .map(|o| (o.block, o.offset, o.len, o.text.as_str(), o.after, o.before))
        .collect();
    assert_eq!(
        o,
        [
            (1, 4, 13, "<orphan one/>", Some(0), Some(1)),
            (1, 21, 4, "tail", Some(1), None),
            (2, 0, 11, "whole block", None, None),
        ]
    );
    let r = &ex.reports[0];
    assert_eq!((r.entry_bytes, r.orphan_bytes, r.block_bytes), (8, 28, 36));
    assert_eq!(r.orphan_regions, 3);
}

#[test]
fn overlapping_entries_are_an_error() {
    let mut b = ot();
    b.note(1, b"0123456789", &[("GEN", 1, 1)]);
    let at = b.index("GEN", 1, 2);
    b.recs[at] = (1, 5, 8); // starts inside the first entry and runs past its end
    b.blocks[1].extend_from_slice(b"abc");
    let err = module("overlap.zip", &Options::default(), &[&b])
        .unwrap()
        .read()
        .unwrap_err();
    assert!(
        matches!(err, Error::Corrupt(ref m) if m.contains("overlap")),
        "{err}"
    );
}

#[test]
fn zcom4_has_twelve_byte_records_and_32_bit_sizes() {
    let mut b = ot();
    let big = vec![b'x'; 70_000]; // would not fit a 16-bit size
    b.note(1, &big, &[("GEN", 1, 1)]);
    let opts = Options {
        zcom4: true,
        ..Options::default()
    };
    let m = module("zcom4.zip", &opts, &[&b]).unwrap();
    assert_eq!(m.driver, Driver::ZCom4);
    let ex = m.read().unwrap();
    assert_eq!(ex.entries[0].text.len(), 70_000);
    assert_eq!(ex.reports[0].record_size, 12);
}

#[test]
fn the_record_count_must_match_the_kjv_slot_count_exactly() {
    let mut b = ot();
    b.note(1, b"x", &[("GEN", 1, 1)]);
    // One record too few, one too many, and a size that is not a multiple of the record.
    for (name, change) in [("few", -10isize), ("many", 10), ("ragged", -3)] {
        let err = module_with(
            &format!("count-{name}.zip"),
            &Options::default(),
            &[&b],
            |files| {
                let zv = &mut files
                    .iter_mut()
                    .find(|(n, _)| n.ends_with(".bzv"))
                    .unwrap()
                    .1;
                if change < 0 {
                    zv.truncate(zv.len() - change.unsigned_abs());
                } else {
                    zv.extend(vec![0u8; change as usize]);
                }
            },
        )
        .unwrap()
        .read()
        .unwrap_err();
        assert!(
            matches!(
                err,
                Error::SlotCount {
                    expected: 24_115,
                    ..
                }
            ),
            "{name}: {err}"
        );
    }
    // A module written as zCom4 but declared zCom (10-byte records) fails the same way.
    let mut b4 = Builder::new(Testament::New);
    b4.note(1, b"x", &[("MAT", 1, 1)]);
    // 8,246 twelve-byte records are not 8,246 ten-byte records.
    let wrong = Options {
        zcom4: true,
        conf_driver: Some("zCom"),
        ..Options::default()
    };
    let err = module("wrong-driver.zip", &wrong, &[&b4])
        .unwrap()
        .read()
        .unwrap_err();
    assert!(
        matches!(
            err,
            Error::SlotCount {
                expected: 8_246,
                record_size: 10,
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn damaged_blocks_and_entries_are_errors() {
    let mut b = ot();
    b.note(1, b"hello", &[("GEN", 1, 1)]);

    // A block whose declared uncompressed size is wrong.
    let err = module_with("size.zip", &Options::default(), &[&b], |f| {
        let zs = &mut f.iter_mut().find(|(n, _)| n.ends_with(".bzs")).unwrap().1;
        zs[8 + 12] = zs[8 + 12].wrapping_add(1); // block 1's uncompressed size
    })
    .unwrap()
    .read()
    .unwrap_err();
    assert!(
        matches!(err, Error::Corrupt(ref m) if m.contains("block 1")),
        "{err}"
    );

    // Compressed data that is not zlib.
    let err = module_with("zlib.zip", &Options::default(), &[&b], |f| {
        let zz = &mut f.iter_mut().find(|(n, _)| n.ends_with(".bzz")).unwrap().1;
        for byte in zz.iter_mut() {
            *byte = 0xff;
        }
    })
    .unwrap()
    .read()
    .unwrap_err();
    assert!(matches!(err, Error::Corrupt(_)), "{err}");

    // An entry that runs past the end of its block, and one in a block that is missing.
    let mut past = ot();
    past.note(1, b"hello", &[("GEN", 1, 1)]);
    let at = past.index("GEN", 1, 1);
    past.recs[at].2 += 1;
    let err = module("past.zip", &Options::default(), &[&past])
        .unwrap()
        .read()
        .unwrap_err();
    assert!(
        matches!(err, Error::Corrupt(ref m) if m.contains("outside block")),
        "{err}"
    );
    let mut missing = ot();
    missing.note(1, b"hello", &[("GEN", 1, 1)]);
    let at = missing.index("GEN", 1, 1);
    missing.recs[at].0 = 9;
    let err = module("nobl.zip", &Options::default(), &[&missing])
        .unwrap()
        .read()
        .unwrap_err();
    assert!(
        matches!(err, Error::Corrupt(ref m) if m.contains("does not exist")),
        "{err}"
    );
}

#[test]
fn encodings_declared_detected_and_never_guessed_wrongly() {
    let mut b = ot();
    b.note(1, "caf\u{e9} \u{a0}\u{2014}".as_bytes(), &[("GEN", 1, 1)]); // UTF-8 bytes
    // Declared UTF-8.
    let m = module("enc-utf8.zip", &Options::default(), &[&b]).unwrap();
    assert_eq!(m.declared_encoding, Some(Encoding::Utf8));
    let ex = m.read().unwrap();
    assert_eq!((ex.encoding, ex.encoding_detected), (Encoding::Utf8, false));
    assert_eq!(ex.entries[0].text, "caf\u{e9} \u{a0}\u{2014}");

    // No Encoding line and all blocks valid UTF-8: detected as UTF-8.
    let none = Options {
        extra_conf: "",
        ..Options::default()
    };
    let ex = module("enc-detect-utf8.zip", &none, &[&b])
        .unwrap()
        .read()
        .unwrap();
    assert_eq!((ex.encoding, ex.encoding_detected), (Encoding::Utf8, true));
    assert_eq!(ex.entries[0].text, "caf\u{e9} \u{a0}\u{2014}");

    // No Encoding line and bytes that are not UTF-8: Latin-1, byte for byte.
    let mut l = ot();
    l.note(1, b"caf\xe9 \xa3 \x94", &[("GEN", 1, 1)]);
    let ex = module("enc-detect-latin1.zip", &none, &[&l])
        .unwrap()
        .read()
        .unwrap();
    assert_eq!(
        (ex.encoding, ex.encoding_detected),
        (Encoding::Latin1, true)
    );
    assert_eq!(ex.entries[0].text, "caf\u{e9} \u{a3} \u{94}");

    // Declared UTF-8 with invalid bytes is an error, not replacement characters.
    let err = module("enc-bad.zip", &Options::default(), &[&l])
        .unwrap()
        .read()
        .unwrap_err();
    assert!(matches!(err, Error::Decode(_)), "{err}");

    // Declared Latin-1 over valid UTF-8 follows the declaration (mojibake is the module's
    // own claim), and an override beats the declaration.
    let declared = Options {
        extra_conf: "Encoding=Latin-1\n",
        ..Options::default()
    };
    let ex = module("enc-latin1.zip", &declared, &[&b])
        .unwrap()
        .read()
        .unwrap();
    assert_eq!(
        ex.entries[0].text,
        "caf\u{c3}\u{a9} \u{c2}\u{a0}\u{e2}\u{80}\u{94}"
    );
    let ex = module("enc-override.zip", &declared, &[&b])
        .unwrap()
        .with_encoding(Encoding::Utf8)
        .read()
        .unwrap();
    assert_eq!(ex.entries[0].text, "caf\u{e9} \u{a0}\u{2014}");
}

#[test]
fn chapter_blocks_use_the_c_file_extensions() {
    let mut b = Builder::new(Testament::New);
    b.note(1, b"Matt 1:1", &[("MAT", 1, 1)]);
    let opts = Options {
        block_letter: 'c',
        ..Options::default()
    };
    let m = module("chapter.zip", &opts, &[&b]).unwrap();
    assert_eq!(m.block_type, kjv_sword::BlockType::Chapter);
    assert!(m.has_testament(Testament::New) && !m.has_testament(Testament::Old));
    assert_eq!(m.entries().unwrap()[0].text, "Matt 1:1");
    // CHAPTER files under a conf that says BOOK are not found (no guessing by extension).
    let err = module_with(
        "chapter-files-book-conf.zip",
        &Options::default(),
        &[&b],
        |f| {
            for (name, _) in f.iter_mut() {
                *name = name.replace(".bz", ".cz");
            }
        },
    )
    .err()
    .unwrap();
    assert!(matches!(err, Error::MissingFile(_)), "{err}");
}

#[test]
fn refuses_what_it_cannot_prove() {
    let mut b = ot();
    b.note(1, b"x", &[("GEN", 1, 1)]);
    let refused = |extra: &'static str| {
        let opts = Options {
            extra_conf: extra,
            ..Options::default()
        };
        module("refuse.zip", &opts, &[&b])
            .err()
            .expect("should be refused")
    };
    assert!(matches!(refused("Encoding=UTF-16\n"), Error::Unsupported(m) if m.contains("utf-16")));
    assert!(matches!(refused("Versification=Vulg\n"), Error::Unsupported(m) if m.contains("Vulg")));
    assert!(matches!(refused("CipherKey=\n"), Error::Unsupported(_)));
    assert!(matches!(
        refused("BlockType=VERSE\n"),
        Error::Unsupported(_)
    ));
    // Only part of a testament's files present.
    let err = module_with("partial.zip", &Options::default(), &[&b], |f| {
        f.retain(|(n, _)| !n.ends_with(".bzz"))
    })
    .err()
    .unwrap();
    assert!(matches!(err, Error::MissingFile(_)), "{err}");
    // No data at all.
    let err = module_with("none.zip", &Options::default(), &[&b], |f| f.clear())
        .err()
        .unwrap();
    assert!(matches!(err, Error::MissingFile(_)), "{err}");
    // Not a zip, and a missing file.
    let junk = temp("junk.zip");
    std::fs::write(&junk, b"not a zip").unwrap();
    assert!(matches!(Module::open_zip(&junk), Err(Error::Zip(_))));
    assert!(matches!(
        Module::open_zip(&temp("does-not-exist.zip")),
        Err(Error::Io(_))
    ));
}

#[test]
fn a_module_with_a_nonstandard_section_name_and_conf_is_read() {
    let mut b = ot();
    b.note(1, b"x", &[("GEN", 1, 1)]);
    let opts = Options {
        name: "SIGILL",
        extra_conf: "Encoding=UTF-8\nHistory_1.0=a\nHistory_1.1=b\nAbout=one \\\ntwo\n",
        ..Options::default()
    };
    let m = module("section.zip", &opts, &[&b]).unwrap();
    assert_eq!(m.name, "SIGILL");
    assert_eq!(m.conf.get_all("History_1.0"), ["a"]);
    assert_eq!(m.conf.get("About"), Some("one \ntwo"));
    assert_eq!(m.conf.get("ModDrv"), Some("zCom"));
}
