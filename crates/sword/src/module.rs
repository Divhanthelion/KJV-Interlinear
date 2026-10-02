//! Reading zCom / zCom4 commentary modules.
//!
//! # On-disk format (as observed and proven against the seven modules in the tests)
//!
//! A compressed SWORD module keeps, per testament (`ot`, `nt`), three files whose
//! extension starts with the block type (`b` = BOOK, `c` = CHAPTER):
//!
//! * `*zs`, block index: one 12-byte record per block, three little-endian `u32`s:
//!   (offset into `*zz`, compressed size, uncompressed size).
//! * `*zz`, block data: the blocks, each a zlib stream (`CompressType=ZIP`), back to back.
//! * `*zv`, verse index: one record per slot, in versification order. zCom records are
//!   10 bytes: (block `u32`, offset `u32`, size `u16`). zCom4 records are 12 bytes: the
//!   same with a `u32` size. A size of 0 means the slot has no text.
//!
//! The slots of a testament are, in order: the module heading, the testament heading,
//! then for every book its introduction, and for every chapter its introduction followed
//! by its verses (see [`Testament::slots`]). Their number is checked against the index
//! length on every read.
//!
//! An entry's text is bytes `[offset, offset + size)` of its decompressed block. Entries
//! are stored back to back with no separator bytes. A note that covers several verses is
//! stored once and every slot it covers points at it.
//!
//! # Bytes the index does not reach
//!
//! Some modules (MHC, KD, JFB here) contain block bytes that no verse record points at.
//! They are real commentary text, so they are never dropped: [`Extraction::orphans`]
//! returns them, positioned between the entries that surround them in the block.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;

use flate2::read::ZlibDecoder;

use crate::conf::Conf;
use crate::error::Error;
use crate::kjv::{HEADING_SLOTS, Slot, Testament};

/// Which SWORD driver wrote the module; it fixes the verse-index record size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Driver {
    /// 10-byte records, 16-bit entry sizes (an entry over 65,535 bytes cannot be stored).
    ZCom,
    /// 12-byte records, 32-bit entry sizes.
    ZCom4,
}

impl Driver {
    pub fn record_size(self) -> usize {
        match self {
            Driver::ZCom => 10,
            Driver::ZCom4 => 12,
        }
    }
}

/// How much text is compressed together; it only changes the data-file extensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockType {
    Book,
    Chapter,
}

impl BlockType {
    fn letter(self) -> char {
        match self {
            BlockType::Book => 'b',
            BlockType::Chapter => 'c',
        }
    }
}

/// Text encoding of the stored bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    /// ISO 8859-1: every byte is the code point of the same value. Lossless; bytes
    /// 0x80..=0x9F become C1 control characters, exactly as stored.
    Latin1,
}

/// One note (or a range of verses sharing one note).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// USFM code, e.g. `"JHN"`.
    pub book: &'static str,
    /// 0 for a book introduction.
    pub chapter: u32,
    /// 0 for a chapter introduction (or a book introduction when `chapter == 0`).
    pub verse: u32,
    /// End of the linked range (equal to the start for a single slot).
    pub to_chapter: u32,
    pub to_verse: u32,
    /// The module's markup (OSIS or ThML) exactly as stored, decoded per the encoding.
    pub text: String,
}

impl Entry {
    /// True if the entry covers more than one slot.
    pub fn is_range(&self) -> bool {
        (self.chapter, self.verse) != (self.to_chapter, self.to_verse)
    }
}

/// Which of the two non-text records opening each testament's index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadingKind {
    /// Slot 0.
    Module,
    /// Slot 1.
    Testament,
}

/// A non-empty module or testament heading (in these modules, an importer milestone).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub testament: Testament,
    pub kind: HeadingKind,
    pub text: String,
}

/// A later slot whose index record is identical to an earlier, non-adjacent slot's.
///
/// Adjacent slots (and slots separated only by empty chapter introductions) with the
/// same record are collapsed into one ranged [`Entry`]. Anything else is reported here
/// and still returned as its own entry, so nothing is lost; the importer decides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repeat {
    /// Index in [`Extraction::entries`] of the later (repeating) entry.
    pub entry: usize,
    /// Index of the earlier entry with the same text.
    pub first: usize,
}

/// Block bytes that no verse record points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Orphan {
    pub testament: Testament,
    pub block: u32,
    /// Byte offset in the decompressed block.
    pub offset: usize,
    /// Length in bytes of the stored data.
    pub len: usize,
    pub text: String,
    /// The entry whose bytes end exactly where this region starts, if any. (`None` at the
    /// start of a block or when the neighbour is a module/testament heading.)
    pub after: Option<usize>,
    /// The entry whose bytes start exactly where this region ends. (`None` for a region
    /// that runs to the end of its block.)
    pub before: Option<usize>,
}

/// Counts proving how a testament's files were read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestamentReport {
    pub testament: Testament,
    pub record_size: usize,
    /// Records found in the verse index; equal to `slot_count` or the read fails.
    pub records: usize,
    pub slot_count: usize,
    /// Slots with size 0.
    pub empty_slots: usize,
    pub blocks: usize,
    /// Sum of declared (= actual) uncompressed block sizes.
    pub block_bytes: u64,
    /// Length of the block data file, and how much of it the block index covers.
    pub data_file_len: u64,
    pub data_covered: u64,
    /// Bytes of the data file that no block covers (Wesley's NT file starts with ten),
    /// and whether any of them is not zero.
    pub data_gap_bytes: u64,
    pub data_gap_nonzero: bool,
    /// Distinct (block, offset, size) records, headings included.
    pub distinct_entries: usize,
    /// Sum of those records' sizes.
    pub entry_bytes: u64,
    pub orphan_regions: usize,
    pub orphan_bytes: u64,
}

/// Everything read from a module.
#[derive(Debug, Clone)]
pub struct Extraction {
    /// Notes in canonical order (Old Testament then New, book by book): per book the
    /// introduction, then each chapter's introduction followed by its verse notes.
    pub entries: Vec<Entry>,
    pub headings: Vec<Heading>,
    pub repeats: Vec<Repeat>,
    pub orphans: Vec<Orphan>,
    /// One per testament present in the module.
    pub reports: Vec<TestamentReport>,
    /// The encoding the bytes were decoded with.
    pub encoding: Encoding,
    /// True when the module declares no `Encoding` and the reader chose it by testing
    /// every block for valid UTF-8 (valid: UTF-8; otherwise Latin-1).
    pub encoding_detected: bool,
}

impl Extraction {
    /// The entries without the [`Repeat`]s: every record once, at its first position.
    pub fn distinct_entries(&self) -> Vec<&Entry> {
        let repeated: std::collections::HashSet<usize> =
            self.repeats.iter().map(|r| r.entry).collect();
        self.entries
            .iter()
            .enumerate()
            .filter(|(i, _)| !repeated.contains(i))
            .map(|(_, e)| e)
            .collect()
    }
}

struct TestamentFiles {
    index: Vec<u8>,
    verses: Vec<u8>,
    data: Vec<u8>,
}

/// An opened module: configuration plus the raw index and data files in memory.
pub struct Module {
    /// The `[Section]` name from the conf.
    pub name: String,
    pub conf: Conf,
    pub driver: Driver,
    pub block_type: BlockType,
    /// The conf's `Encoding`, if it has one.
    pub declared_encoding: Option<Encoding>,
    forced_encoding: Option<Encoding>,
    files: [Option<TestamentFiles>; 2],
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct Rec {
    block: u32,
    offset: u32,
    size: u32,
}

struct RawTestament {
    testament: Testament,
    recs: Vec<Rec>,
    blocks: Vec<Vec<u8>>,
    block_bytes: u64,
    data_file_len: u64,
    data_covered: u64,
    data_gap_bytes: u64,
    data_gap_nonzero: bool,
}

fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

fn decode(
    encoding: Encoding,
    bytes: &[u8],
    what: impl FnOnce() -> String,
) -> Result<String, Error> {
    match encoding {
        Encoding::Latin1 => Ok(latin1(bytes)),
        Encoding::Utf8 => std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|e| Error::Decode(format!("{}: {e}", what()))),
    }
}

fn parse_encoding(value: &str) -> Result<Encoding, Error> {
    match value.trim().to_ascii_lowercase().as_str() {
        "utf-8" | "utf8" => Ok(Encoding::Utf8),
        "latin-1" | "latin1" | "iso-8859-1" => Ok(Encoding::Latin1),
        other => Err(Error::Unsupported(format!("Encoding={other}"))),
    }
}

impl Module {
    /// Opens a module straight from its `.zip` (as CrossWire distributes it): one
    /// `mods.d/<name>.conf` and the data files under the conf's `DataPath`.
    pub fn open_zip(path: &Path) -> Result<Module, Error> {
        let file = BufReader::new(File::open(path)?);
        Module::from_zip(zip::ZipArchive::new(file)?)
    }

    fn from_zip<R: Read + Seek>(mut zip: zip::ZipArchive<R>) -> Result<Module, Error> {
        // (name as stored, normalised name)
        let names: Vec<(String, String)> = zip
            .file_names()
            .map(|n| (n.to_string(), n.replace('\\', "/")))
            .collect();
        let mut read = |wanted: &str| -> Result<Option<Vec<u8>>, Error> {
            let found = names
                .iter()
                .find(|(_, norm)| norm == wanted)
                .or_else(|| names.iter().find(|(_, n)| n.eq_ignore_ascii_case(wanted)));
            let Some((orig, _)) = found else {
                return Ok(None);
            };
            let mut out = Vec::new();
            zip.by_name(orig)?.read_to_end(&mut out)?;
            Ok(Some(out))
        };

        let confs: Vec<&(String, String)> = names
            .iter()
            .filter(|(_, n)| {
                let l = n.to_ascii_lowercase();
                l.starts_with("mods.d/") && l.ends_with(".conf")
            })
            .collect();
        let conf_name = match confs.as_slice() {
            [one] => one.1.clone(),
            [] => return Err(Error::Conf("no mods.d/*.conf in the archive".into())),
            many => {
                let list: Vec<_> = many.iter().map(|c| c.1.as_str()).collect();
                return Err(Error::Conf(format!("several conf files: {list:?}")));
            }
        };
        let conf_bytes = read(&conf_name)?.expect("listed above");
        let conf_text = match std::str::from_utf8(&conf_bytes) {
            Ok(s) => s.to_owned(),
            Err(_) => latin1(&conf_bytes),
        };
        let (section, conf) = Conf::parse_with_section(&conf_text);
        let name = section.ok_or_else(|| Error::Conf(format!("{conf_name} has no [Section]")))?;

        let driver = match conf.get("ModDrv") {
            Some("zCom") => Driver::ZCom,
            Some("zCom4") => Driver::ZCom4,
            other => return Err(Error::Unsupported(format!("ModDrv={other:?}"))),
        };
        match conf.get("CompressType") {
            Some("ZIP") => {}
            other => return Err(Error::Unsupported(format!("CompressType={other:?}"))),
        }
        let block_type = match conf.get("BlockType") {
            Some("BOOK") => BlockType::Book,
            // SWORD's default when the line is absent.
            Some("CHAPTER") | None => BlockType::Chapter,
            Some(other) => return Err(Error::Unsupported(format!("BlockType={other}"))),
        };
        match conf.get("Versification") {
            None | Some("KJV") => {}
            Some(other) => return Err(Error::Unsupported(format!("Versification={other}"))),
        }
        if conf.get("CipherKey").is_some() {
            return Err(Error::Unsupported("encrypted module (CipherKey)".into()));
        }
        let declared_encoding = conf.get("Encoding").map(parse_encoding).transpose()?;

        let data_path = conf
            .get("DataPath")
            .ok_or_else(|| Error::Conf("no DataPath".into()))?
            .replace('\\', "/");
        let mut dir = data_path
            .trim_start_matches("./")
            .trim_start_matches('/')
            .to_string();
        if !dir.is_empty() && !dir.ends_with('/') {
            dir.push('/');
        }

        let mut files: [Option<TestamentFiles>; 2] = [None, None];
        for (slot, t) in files.iter_mut().zip(Testament::BOTH) {
            let stem = t.file_stem();
            let l = block_type.letter();
            let wanted = ["zs", "zv", "zz"].map(|ext| format!("{dir}{stem}.{l}{ext}"));
            let got = wanted.clone().map(|n| read(&n));
            let [index, verses, data] = got;
            match (index?, verses?, data?) {
                (None, None, None) => {}
                (Some(index), Some(verses), Some(data)) => {
                    *slot = Some(TestamentFiles {
                        index,
                        verses,
                        data,
                    })
                }
                (i, v, d) => {
                    let missing: Vec<_> = [(&i, &wanted[0]), (&v, &wanted[1]), (&d, &wanted[2])]
                        .into_iter()
                        .filter(|(o, _)| o.is_none())
                        .map(|(_, n)| n.as_str())
                        .collect();
                    return Err(Error::MissingFile(format!("{missing:?}")));
                }
            }
        }
        if files.iter().all(Option::is_none) {
            return Err(Error::MissingFile(format!(
                "no ot/nt data files under {dir:?} for BlockType {block_type:?}"
            )));
        }
        Ok(Module {
            name,
            conf,
            driver,
            block_type,
            declared_encoding,
            forced_encoding: None,
            files,
        })
    }

    /// Overrides the text encoding (the module's own declaration, or detection when it
    /// has none).
    pub fn with_encoding(mut self, encoding: Encoding) -> Module {
        self.forced_encoding = Some(encoding);
        self
    }

    /// Whether the module has data for this testament.
    pub fn has_testament(&self, t: Testament) -> bool {
        self.files[t as usize].is_some()
    }

    /// Notes in canonical order with ranges collapsed. Use [`Module::read`] to also get
    /// the headings, repeats and orphans that make the extraction complete.
    pub fn entries(&self) -> Result<Vec<Entry>, Error> {
        Ok(self.read()?.entries)
    }

    /// Reads every note, plus everything else in the module that is not a note (see
    /// [`Extraction`]).
    pub fn read(&self) -> Result<Extraction, Error> {
        let mut raws = Vec::new();
        for t in Testament::BOTH {
            if let Some(files) = &self.files[t as usize] {
                raws.push(self.parse_testament(t, files)?);
            }
        }

        let (encoding, encoding_detected) = match self.forced_encoding.or(self.declared_encoding) {
            Some(e) => (e, false),
            None => {
                let all_utf8 = raws
                    .iter()
                    .flat_map(|r| r.blocks.iter())
                    .all(|b| std::str::from_utf8(b).is_ok());
                (
                    if all_utf8 {
                        Encoding::Utf8
                    } else {
                        Encoding::Latin1
                    },
                    true,
                )
            }
        };

        let mut out = Extraction {
            entries: Vec::new(),
            headings: Vec::new(),
            repeats: Vec::new(),
            orphans: Vec::new(),
            reports: Vec::new(),
            encoding,
            encoding_detected,
        };
        for raw in &raws {
            self.build_testament(raw, encoding, &mut out)?;
        }
        Ok(out)
    }

    fn parse_testament(&self, t: Testament, files: &TestamentFiles) -> Result<RawTestament, Error> {
        let rs = self.driver.record_size();
        let expected = t.slot_count();
        let found = files.verses.len() / rs;
        if !files.verses.len().is_multiple_of(rs) || found != expected {
            return Err(Error::SlotCount {
                testament: t,
                expected,
                found,
                file_len: files.verses.len(),
                record_size: rs,
            });
        }
        let recs: Vec<Rec> = files
            .verses
            .chunks_exact(rs)
            .map(|c| Rec {
                block: le32(c, 0),
                offset: le32(c, 4),
                size: match self.driver {
                    Driver::ZCom => u32::from(u16::from_le_bytes([c[8], c[9]])),
                    Driver::ZCom4 => le32(c, 8),
                },
            })
            .collect();

        if !files.index.len().is_multiple_of(12) {
            return Err(Error::Corrupt(format!(
                "{t:?} block index is {} bytes, not a multiple of 12",
                files.index.len()
            )));
        }
        let mut blocks = Vec::with_capacity(files.index.len() / 12);
        let mut block_bytes = 0u64;
        let mut data_covered = 0u64;
        for (n, rec) in files.index.as_chunks::<12>().0.iter().enumerate() {
            let (off, csize, usize_) = (
                le32(rec, 0) as usize,
                le32(rec, 4) as usize,
                le32(rec, 8) as usize,
            );
            let compressed = off
                .checked_add(csize)
                .and_then(|end| files.data.get(off..end))
                .ok_or_else(|| {
                    Error::Corrupt(format!("{t:?} block {n} lies outside the data file"))
                })?;
            let mut buf = Vec::with_capacity(usize_.min(1 << 28));
            ZlibDecoder::new(compressed)
                .read_to_end(&mut buf)
                .map_err(|e| Error::Corrupt(format!("{t:?} block {n} will not decompress: {e}")))?;
            if buf.len() != usize_ {
                return Err(Error::Corrupt(format!(
                    "{t:?} block {n} decompresses to {} bytes, index says {usize_}",
                    buf.len()
                )));
            }
            block_bytes += usize_ as u64;
            data_covered += csize as u64;
            blocks.push(buf);
        }
        // Account for the data file itself: blocks must not overlap; anything between
        // them is reported (and flagged if it is not all zero).
        let mut spans: Vec<(usize, usize)> = files
            .index
            .as_chunks::<12>()
            .0
            .iter()
            .map(|r| (le32(r, 0) as usize, le32(r, 4) as usize))
            .collect();
        spans.sort_unstable();
        let (mut pos, mut gap_bytes, mut gap_nonzero) = (0usize, 0u64, false);
        for (off, len) in spans.into_iter().chain([(files.data.len(), 0)]) {
            if off < pos {
                return Err(Error::Corrupt(format!(
                    "{t:?} blocks overlap in the data file at {off}"
                )));
            }
            gap_bytes += (off - pos) as u64;
            gap_nonzero |= files.data[pos..off].iter().any(|&b| b != 0);
            pos = off + len;
        }
        for (i, r) in recs.iter().enumerate().filter(|(_, r)| r.size > 0) {
            let block = blocks.get(r.block as usize).ok_or_else(|| {
                Error::Corrupt(format!("{t:?} slot {i}: block {} does not exist", r.block))
            })?;
            let end = r.offset as usize + r.size as usize;
            if end > block.len() {
                return Err(Error::Corrupt(format!(
                    "{t:?} slot {i}: entry [{}, {end}) is outside block {} ({} bytes)",
                    r.offset,
                    r.block,
                    block.len()
                )));
            }
        }
        Ok(RawTestament {
            testament: t,
            recs,
            blocks,
            block_bytes,
            data_file_len: files.data.len() as u64,
            data_covered,
            data_gap_bytes: gap_bytes,
            data_gap_nonzero: gap_nonzero,
        })
    }

    fn build_testament(
        &self,
        raw: &RawTestament,
        enc: Encoding,
        out: &mut Extraction,
    ) -> Result<(), Error> {
        let t = raw.testament;
        let slots: Vec<Slot> = t.slots();
        let text_recs = &raw.recs[HEADING_SLOTS..];
        let bytes_of = |r: Rec| {
            &raw.blocks[r.block as usize][r.offset as usize..r.offset as usize + r.size as usize]
        };

        for (i, kind) in [HeadingKind::Module, HeadingKind::Testament]
            .into_iter()
            .enumerate()
        {
            let r = raw.recs[i];
            if r.size > 0 {
                let text = decode(enc, bytes_of(r), || format!("{t:?} {kind:?} heading"))?;
                out.headings.push(Heading {
                    testament: t,
                    kind,
                    text,
                });
            }
        }

        // Collapse runs. A run extends over following slots with the identical record,
        // within one book, skipping only empty chapter/book introductions.
        struct Group {
            rec: Rec,
            first: usize,
            last: usize,
        }
        let mut groups: Vec<Group> = Vec::new();
        for (j, &rec) in text_recs.iter().enumerate() {
            if rec.size == 0 {
                continue;
            }
            if let Some(g) = groups.last_mut() {
                let bridge = (g.last + 1..j).all(|k| slots[k].verse == 0 && text_recs[k].size == 0);
                if g.rec == rec && slots[g.last].book == slots[j].book && bridge {
                    g.last = j;
                    continue;
                }
            }
            groups.push(Group {
                rec,
                first: j,
                last: j,
            });
        }

        let base = out.entries.len();
        let mut primary: HashMap<Rec, usize> = HashMap::new();
        for g in &groups {
            let (a, b) = (slots[g.first], slots[g.last]);
            let text = decode(enc, bytes_of(g.rec), || {
                format!(
                    "{} {}:{} (block {}, offset {})",
                    a.book, a.chapter, a.verse, g.rec.block, g.rec.offset
                )
            })?;
            let index = out.entries.len();
            out.entries.push(Entry {
                book: a.book,
                chapter: a.chapter,
                verse: a.verse,
                to_chapter: b.chapter,
                to_verse: b.verse,
                text,
            });
            match primary.get(&g.rec) {
                Some(&first) => out.repeats.push(Repeat {
                    entry: index,
                    first,
                }),
                None => {
                    primary.insert(g.rec, index);
                }
            }
        }
        debug_assert!(out.entries.len() >= base);

        // Account for every byte of every block.
        let mut per_block: Vec<Vec<Rec>> = vec![Vec::new(); raw.blocks.len()];
        let mut distinct = std::collections::HashSet::new();
        let mut entry_bytes = 0u64;
        for &r in raw.recs.iter().filter(|r| r.size > 0) {
            if distinct.insert(r) {
                entry_bytes += u64::from(r.size);
                per_block[r.block as usize].push(r);
            }
        }
        let (mut orphan_regions, mut orphan_bytes) = (0usize, 0u64);
        for (b, recs) in per_block.iter_mut().enumerate() {
            recs.sort_by_key(|r| (r.offset, r.size));
            let block = &raw.blocks[b];
            let mut pos = 0usize;
            let mut prev: Option<Rec> = None;
            let mut gaps: Vec<(usize, usize, Option<Rec>, Option<Rec>)> = Vec::new();
            for &r in recs.iter() {
                if (r.offset as usize) < pos {
                    return Err(Error::Corrupt(format!(
                        "{t:?} block {b}: entries overlap at offset {}",
                        r.offset
                    )));
                }
                if r.offset as usize > pos {
                    gaps.push((pos, r.offset as usize, prev, Some(r)));
                }
                pos = r.offset as usize + r.size as usize;
                prev = Some(r);
            }
            if pos < block.len() {
                gaps.push((pos, block.len(), prev, None));
            }
            for (start, end, after, before) in gaps {
                let text = decode(enc, &block[start..end], || {
                    format!("{t:?} block {b} unindexed bytes at {start}")
                })?;
                orphan_regions += 1;
                orphan_bytes += (end - start) as u64;
                out.orphans.push(Orphan {
                    testament: t,
                    block: b as u32,
                    offset: start,
                    len: end - start,
                    text,
                    after: after.and_then(|r| primary.get(&r).copied()),
                    before: before.and_then(|r| primary.get(&r).copied()),
                });
            }
        }

        out.reports.push(TestamentReport {
            testament: t,
            record_size: self.driver.record_size(),
            records: raw.recs.len(),
            slot_count: t.slot_count(),
            empty_slots: raw.recs.iter().filter(|r| r.size == 0).count(),
            blocks: raw.blocks.len(),
            block_bytes: raw.block_bytes,
            data_file_len: raw.data_file_len,
            data_covered: raw.data_covered,
            data_gap_bytes: raw.data_gap_bytes,
            data_gap_nonzero: raw.data_gap_nonzero,
            distinct_entries: distinct.len(),
            entry_bytes,
            orphan_regions,
            orphan_bytes,
        });
        Ok(())
    }
}
