//! Searching the library: each book's documents (a translation's verses, a
//! commentary's notes) folded once and kept while there's room, and a word index,
//! made with the archive, that says which books can hold a query's words, so a
//! search reads only those.
//!
//! The index lists, for every word (run of letters and digits in folded text), the
//! books it occurs in. A query is found only in a book holding, for each run of
//! letters and digits in the query, a word that contains it; so the index rules
//! books out without ever ruling out one with a match. Matches are then found by
//! scanning the folded text itself, so results are exact.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use crate::text::{fold, words};

// ---------------------------------------------------------------- folded books

/// A book's documents, folded, each followed by '\n' (folding turns every other
/// line break into a space, so a match never runs from one document into the next).
pub struct Corpus {
    text: String,
    starts: Vec<u32>,
}

impl Corpus {
    pub fn new<S: AsRef<str>>(docs: impl IntoIterator<Item = S>) -> Corpus {
        let mut text = String::new();
        let mut starts = Vec::new();
        for d in docs {
            starts.push(text.len() as u32);
            text.push_str(&fold(d.as_ref()));
            text.push('\n');
        }
        Corpus { text, starts }
    }

    /// How many documents it holds.
    pub fn len(&self) -> usize {
        self.starts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.starts.is_empty()
    }

    /// Memory it takes, roughly.
    pub fn bytes(&self) -> usize {
        self.text.len() + self.starts.len() * 4
    }

    /// The documents holding `folded` (a query already folded), in order.
    pub fn find(&self, folded: &str) -> Vec<usize> {
        let mut out = Vec::new();
        if folded.is_empty() || folded.contains('\n') {
            return out;
        }
        let mut at = 0;
        while at < self.text.len() {
            let Some(pos) = self.text[at..].find(folded) else { break };
            let abs = at + pos;
            let doc = self.starts.partition_point(|&s| s as usize <= abs) - 1;
            out.push(doc);
            at = self.starts.get(doc + 1).map_or(self.text.len(), |&s| s as usize);
        }
        out
    }
}

/// Folded books kept for searching again, the least recently used let go first once
/// they take more than the limit.
pub struct Corpora {
    inner: Mutex<Kept>,
}

struct Kept {
    map: HashMap<String, (Arc<Corpus>, u64)>,
    clock: u64,
    bytes: usize,
    limit: usize,
}

/// Folded text kept by default: every translation's and a few commentaries' worth.
pub const DEFAULT_CORPUS_BYTES: usize = 256 * 1024 * 1024;

impl Default for Corpora {
    fn default() -> Self {
        Corpora::new(DEFAULT_CORPUS_BYTES)
    }
}

impl Corpora {
    pub fn new(limit: usize) -> Corpora {
        Corpora { inner: Mutex::new(Kept { map: HashMap::new(), clock: 0, bytes: 0, limit }) }
    }

    /// The corpus for `key`, made by `make` if it isn't kept. (Made outside the lock:
    /// two searches may make the same book at once, and one copy is kept.)
    pub fn get_or(&self, key: &str, make: impl FnOnce() -> Result<Corpus, String>) -> Result<Arc<Corpus>, String> {
        {
            let mut k = self.inner.lock().unwrap();
            k.clock += 1;
            let now = k.clock;
            if let Some((c, used)) = k.map.get_mut(key) {
                *used = now;
                return Ok(c.clone());
            }
        }
        let made = Arc::new(make()?);
        let mut k = self.inner.lock().unwrap();
        k.clock += 1;
        let now = k.clock;
        if let Some((c, used)) = k.map.get_mut(key) {
            *used = now;
            return Ok(c.clone());
        }
        k.bytes += made.bytes();
        k.map.insert(key.to_string(), (made.clone(), now));
        k.trim();
        Ok(made)
    }

    pub fn set_limit(&self, bytes: usize) {
        let mut k = self.inner.lock().unwrap();
        k.limit = bytes;
        k.trim();
    }

    /// Bytes of folded text kept now.
    pub fn kept_bytes(&self) -> usize {
        self.inner.lock().unwrap().bytes
    }
}

impl Kept {
    fn trim(&mut self) {
        while self.bytes > self.limit && self.map.len() > 1 {
            let Some(oldest) = self.map.iter().min_by_key(|(_, (_, used))| *used).map(|(k, _)| k.clone()) else { break };
            if let Some((c, _)) = self.map.remove(&oldest) {
                self.bytes -= c.bytes();
            }
        }
    }
}

// ---------------------------------------------------------------- the word index

/// Where the archive keeps the index.
pub const INDEX_KEY: &str = "search/index.bin";
const MAGIC: &[u8; 8] = b"KJVSRCH1";

/// Which books each word occurs in. Books ("chunks") are named as the corpora are:
/// "bible/web/GEN", "comm/mhc/GEN".
pub struct Index {
    chunks: Vec<String>,
    numbers: HashMap<String, u32>,
    /// Every word, each followed by '\n', in sorted order
    vocab: String,
    word_starts: Vec<u32>,
    /// Where each word's books are in `postings` (one more than there are words)
    offsets: Vec<u32>,
    /// Each word's book numbers, as differences from the one before, in LEB128
    postings: Vec<u8>,
}

/// A set of books, by number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkSet(Vec<u64>);

impl ChunkSet {
    fn empty(n: usize) -> ChunkSet {
        ChunkSet(vec![0; n.div_ceil(64)])
    }

    fn insert(&mut self, i: u32) {
        self.0[i as usize / 64] |= 1 << (i % 64);
    }

    pub fn contains(&self, i: u32) -> bool {
        self.0.get(i as usize / 64).is_some_and(|w| w & (1 << (i % 64)) != 0)
    }

    fn intersect(&mut self, other: &ChunkSet) {
        for (a, b) in self.0.iter_mut().zip(&other.0) {
            *a &= b;
        }
    }

    pub fn len(&self) -> usize {
        self.0.iter().map(|w| w.count_ones() as usize).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|&w| w == 0)
    }
}

/// A book's words, sorted, once each: what the index is made from.
pub fn chunk_words<S: AsRef<str>>(docs: impl IntoIterator<Item = S>) -> Vec<String> {
    let mut set = std::collections::BTreeSet::new();
    for d in docs {
        let folded = fold(d.as_ref());
        for w in words(&folded) {
            if !set.contains(w) {
                set.insert(w.to_string());
            }
        }
    }
    set.into_iter().collect()
}

fn put_u32(out: &mut Vec<u8>, n: u32) {
    out.extend_from_slice(&n.to_le_bytes());
}

fn put_varint(out: &mut Vec<u8>, mut n: u32) {
    while n >= 0x80 {
        out.push((n as u8 & 0x7f) | 0x80);
        n >>= 7;
    }
    out.push(n as u8);
}

/// The index for `chunks`: each book's name and its words (from [`chunk_words`]).
pub fn encode_index(chunks: &[(String, Vec<String>)]) -> Vec<u8> {
    let mut words: BTreeMap<&str, Vec<u32>> = BTreeMap::new();
    for (i, (_, ws)) in chunks.iter().enumerate() {
        for w in ws {
            words.entry(w.as_str()).or_default().push(i as u32);
        }
    }
    let mut out = MAGIC.to_vec();
    put_u32(&mut out, chunks.len() as u32);
    for (name, _) in chunks {
        put_u32(&mut out, name.len() as u32);
        out.extend_from_slice(name.as_bytes());
    }
    let mut vocab = String::new();
    let mut offsets = Vec::with_capacity(words.len() + 1);
    let mut postings = Vec::new();
    for (w, list) in &words {
        vocab.push_str(w);
        vocab.push('\n');
        offsets.push(postings.len() as u32);
        let mut last = 0;
        for &c in list {
            put_varint(&mut postings, c - last);
            last = c;
        }
    }
    offsets.push(postings.len() as u32);
    put_u32(&mut out, vocab.len() as u32);
    out.extend_from_slice(vocab.as_bytes());
    put_u32(&mut out, words.len() as u32);
    for o in offsets {
        put_u32(&mut out, o);
    }
    put_u32(&mut out, postings.len() as u32);
    out.extend_from_slice(&postings);
    out
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.bytes.len()).ok_or("search index: cut short")?;
        let s = &self.bytes[self.at..end];
        self.at = end;
        Ok(s)
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn text(&mut self, n: usize) -> Result<String, String> {
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| "search index: bad text".to_string())
    }
}

impl Index {
    pub fn parse(bytes: &[u8]) -> Result<Index, String> {
        let mut r = Reader { bytes, at: 0 };
        if r.take(8)? != MAGIC {
            return Err("search index: not an index".into());
        }
        let n = r.u32()? as usize;
        let mut chunks = Vec::with_capacity(n);
        for _ in 0..n {
            let len = r.u32()? as usize;
            chunks.push(r.text(len)?);
        }
        let vlen = r.u32()? as usize;
        let vocab = r.text(vlen)?;
        let words = r.u32()? as usize;
        let mut offsets = Vec::with_capacity(words + 1);
        for _ in 0..=words {
            offsets.push(r.u32()?);
        }
        let plen = r.u32()? as usize;
        let postings = r.take(plen)?.to_vec();
        let mut word_starts = Vec::with_capacity(words);
        let mut start = 0u32;
        for (i, b) in vocab.bytes().enumerate() {
            if b == b'\n' {
                word_starts.push(start);
                start = i as u32 + 1;
            }
        }
        if word_starts.len() != words || offsets.last().copied() != Some(postings.len() as u32) {
            return Err("search index: inconsistent".into());
        }
        let numbers = chunks.iter().enumerate().map(|(i, c)| (c.clone(), i as u32)).collect();
        Ok(Index { chunks, numbers, vocab, word_starts, offsets, postings })
    }

    /// The number of book `name` ("bible/web/GEN"), if the index has it.
    pub fn chunk(&self, name: &str) -> Option<u32> {
        self.numbers.get(name).copied()
    }

    pub fn chunks(&self) -> &[String] {
        &self.chunks
    }

    /// The books whose words contain `part`.
    fn containing(&self, part: &str) -> ChunkSet {
        let mut set = ChunkSet::empty(self.chunks.len());
        let mut at = 0;
        while at < self.vocab.len() {
            let Some(pos) = self.vocab[at..].find(part) else { break };
            let abs = at + pos;
            let w = self.word_starts.partition_point(|&s| s as usize <= abs) - 1;
            let (from, to) = (self.offsets[w] as usize, self.offsets[w + 1] as usize);
            let mut last = 0u32;
            let mut n = 0u32;
            let mut shift = 0;
            for &b in &self.postings[from..to] {
                n |= ((b & 0x7f) as u32) << shift;
                if b & 0x80 == 0 {
                    last += n;
                    set.insert(last);
                    n = 0;
                    shift = 0;
                } else {
                    shift += 7;
                }
            }
            at = self.word_starts.get(w + 1).map_or(self.vocab.len(), |&s| s as usize);
        }
        set
    }

    /// The books that can hold `folded` (a query already folded), or None when the
    /// query has no letters or digits to look up (then any book can).
    pub fn candidates(&self, folded: &str) -> Option<ChunkSet> {
        let mut parts: Vec<&str> = words(folded).collect();
        if parts.is_empty() {
            return None;
        }
        // Longest first: usually the rarest, so the set is small soonest
        parts.sort_by_key(|p| std::cmp::Reverse(p.len()));
        parts.dedup();
        let mut set = self.containing(parts[0]);
        for p in &parts[1..] {
            if set.is_empty() {
                break;
            }
            set.intersect(&self.containing(p));
        }
        Some(set)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_finds_each_document_once() {
        let c = Corpus::new(["In the beginning God created", "God said, Let there be light: and there was light.", "And God saw the light"]);
        assert_eq!(c.find("light"), [1, 2]);
        assert_eq!(c.find(&fold("GOD")), [0, 1, 2]);
        assert_eq!(c.find("created god"), Vec::<usize>::new(), "never across documents");
        assert_eq!(c.find(""), Vec::<usize>::new());
        assert_eq!(c.len(), 3);
    }

    #[test]
    fn index_rules_out_only_books_without_the_words() {
        let books = [
            ("bible/a/GEN", vec!["In the beginning God created the heaven"]),
            ("bible/a/EXO", vec!["Now these are the names"]),
            ("comm/x/GEN", vec!["Melchizedek, king of Salem — and Cæsar’s"]),
        ];
        let chunks: Vec<(String, Vec<String>)> = books.iter().map(|(n, d)| (n.to_string(), chunk_words(d.iter()))).collect();
        let index = Index::parse(&encode_index(&chunks)).unwrap();
        let has = |q: &str| {
            let set = index.candidates(&fold(q)).unwrap();
            (0..3u32).filter(|&i| set.contains(i)).collect::<Vec<_>>()
        };
        assert_eq!(has("the"), [0, 1]);
        assert_eq!(has("eginni"), [0], "inside a word");
        assert_eq!(has("God created"), [0]);
        assert_eq!(has("melchiz"), [2]);
        assert_eq!(has("caesar's"), [2], "folded, punctuation set aside");
        assert_eq!(has("zebra"), Vec::<u32>::new());
        assert!(index.candidates("—").is_none(), "no letters: every book");
        assert_eq!(index.chunk("comm/x/GEN"), Some(2));
        assert!(Index::parse(b"nonsense").is_err());
    }

    #[test]
    fn kept_corpora_stay_under_the_limit() {
        let kept = Corpora::new(100);
        for i in 0..10 {
            kept.get_or(&format!("k{i}"), || Ok(Corpus::new(["0123456789012345678901234567890123456789"]))).unwrap();
        }
        assert!(kept.kept_bytes() <= 100, "{}", kept.kept_bytes());
        // The most recent is kept
        let mut made = false;
        kept.get_or("k9", || {
            made = true;
            Ok(Corpus::new(["x"]))
        })
        .unwrap();
        assert!(!made);
    }
}
