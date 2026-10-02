//! Aligning one translation's verses with the KJV's, by what they say.
//!
//! Translations number verses differently: the Douay-Rheims follows the Vulgate's
//! Psalms, the Septuagint orders Jeremiah differently, Jewish editions count Psalm
//! titles as verses, some translations join, split, swap, or leave out verses. Every
//! text in the library is English, so a verse can be matched to its KJV counterpart
//! by content: the distinctive words they share (names, numbers, rarer words).
//!
//! Two stages:
//! 1. [`align_blocks`] lines the verses up in order (a sequence alignment allowing
//!    one-to-one matches, joins of two verses, and verses with no counterpart, with a
//!    small preference for verses that carry the same number: [`align`]), keeps what
//!    strong, consecutive matches anchor, and lines up what is left again, as often as
//!    that finds more: each pass recovers another block of text printed in another
//!    order (the Septuagint's Jeremiah, Proverbs, and Exodus 35-40) or in a related
//!    book (Nehemiah as the Septuagint's Ezra 11-23).
//! 2. [`refine`] keeps the matches the content supports, splits joins whose halves
//!    don't both match, and pairs whatever is left by best similarity anywhere in the
//!    book and its related books (single verses moved, additions printed elsewhere);
//!    last of all, leftovers with the same number on both sides are paired "by number".
//!
//! Every group carries how it was matched, so the alignment tables can be reviewed.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::BuildHasherDefault;

/// Words too common to tell verses apart.
const STOP: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "been", "but", "by", "for", "from", "had", "has", "have", "he", "her",
    "him", "his", "i", "in", "is", "it", "its", "me", "my", "not", "of", "on", "or", "our", "out", "said", "say",
    "says", "shall", "she", "so", "that", "the", "their", "them", "then", "there", "they", "this", "thou", "thee",
    "thy", "to", "unto", "up", "upon", "us", "was", "we", "were", "which", "who", "will", "with", "ye", "you", "your",
    "all", "also", "do", "did", "if", "into", "no", "one", "saith", "hath", "hast", "art", "when", "what", "o",
    "lord", "god", "man", "men", "came", "come", "went", "go", "let", "made", "make", "may", "now", "these", "those",
];

/// The comparable words of a verse: lower case, letters and digits, no stop words,
/// cut to their first five letters so "begat"/"begot", "saith"/"says" and spelling
/// variants more often meet.
pub fn words(text: &str) -> Vec<String> {
    let stop: HashSet<&str> = STOP.iter().copied().collect();
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .filter(|w| !stop.contains(w.as_str()))
        .map(|w| w.chars().take(5).collect())
        .collect()
}

/// A verse's comparable words and their weights. The hasher is fixed (not seeded per
/// run) so sums happen in the same order every time and alignments are reproducible.
pub type Weights = HashMap<String, f32, BuildHasherDefault<DefaultHasher>>;

/// Weighted word sets for two runs of verses, weighted by how rare each word is
/// across both (inverse document frequency).
pub fn weigh(a: &[String], b: &[String]) -> (Vec<Weights>, Vec<Weights>) {
    let docs: Vec<Vec<String>> = a.iter().chain(b).map(|t| words(t)).collect();
    let n = docs.len().max(1) as f32;
    let mut df: HashMap<&str, usize> = HashMap::new();
    for d in &docs {
        for w in d.iter().map(String::as_str).collect::<HashSet<_>>() {
            *df.entry(w).or_default() += 1;
        }
    }
    let weights = |d: &Vec<String>| -> Weights {
        let mut m = Weights::default();
        for w in d {
            let idf = (n / df[w.as_str()] as f32).ln() + 1.0;
            // Numbers are strong evidence (lists, counts, ages)
            let boost = if w.chars().all(|c| c.is_ascii_digit()) { 1.5 } else { 1.0 };
            m.insert(w.clone(), idf * boost);
        }
        m
    };
    let (da, db) = docs.split_at(a.len());
    (da.iter().map(weights).collect(), db.iter().map(weights).collect())
}

/// Cosine similarity of weighted word sets, 0..=1 (0 when either is empty).
pub fn cosine(a: &Weights, b: &Weights) -> f32 {
    let (small, large) = if a.len() < b.len() { (a, b) } else { (b, a) };
    let dot: f32 = small.iter().filter_map(|(w, x)| large.get(w).map(|y| x * y)).sum();
    let na: f32 = a.values().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.values().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 { 0.0 } else { dot / (na * nb) }
}

fn joined(a: &Weights, b: &Weights) -> Weights {
    let mut m = a.clone();
    for (w, x) in b {
        m.entry(w.clone()).and_modify(|y| *y = y.max(*x)).or_insert(*x);
    }
    m
}

/// How a group was matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum How {
    /// In order, supported by content
    Content,
    /// In order between strong matches (or the ends of the book) with the same number
    /// of verses on both sides, though weakly similar itself (paraphrase, a short verse)
    Framed,
    /// Paired across the book by best similarity (reordered or relocated text)
    Moved,
    /// Left over on both sides with the same number; content unconfirmed
    Number,
    /// No counterpart
    Unmatched,
}

/// One step of an alignment: verses `a` of the first side correspond to verses `b`
/// of the second (indexes into the inputs). One side may be empty: no counterpart.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub a: Vec<usize>,
    pub b: Vec<usize>,
    /// Similarity of the matched text, 0..=1
    pub score: f32,
    pub how: How,
}

/// Cost of leaving a verse unmatched in the in-order pass.
const GAP: f32 = 0.18;
/// Cost of a join; above a gap's, so a join wins only when its second verse really
/// matches (similarity above JOIN - GAP).
const JOIN: f32 = 0.28;
/// Preference for matching verses that carry the same number.
const SAME_NUMBER: f32 = 0.1;
/// A match at least this similar is trusted on its own.
pub const STRONG: f32 = 0.3;
/// The least similarity for pairing leftovers across the book.
pub const RECOVER: f32 = 0.25;
/// Each half of a join must be at least this similar to its partner to stay joined.
const JOIN_HALF: f32 = 0.2;
/// Pairing leftovers by number needs at least this much agreement, unless the verse
/// is empty in this translation (left out, with a footnote): different recensions
/// (the Vulgate's Tobias and the Greek Tobit) share numbers but not text.
const NUMBER_MIN: f32 = 0.1;
/// A leftover moves away from the verse its number stands for only when the other
/// verse is this much more similar.
const MOVE_MARGIN: f32 = 0.15;

/// Align `a` with `b` in order. `same(i, j)`: whether a[i] and b[j] carry the same
/// verse number. Every index of each side appears in exactly one group.
pub fn align(a: &[Weights], b: &[Weights], same: &dyn Fn(usize, usize) -> bool) -> Vec<Group> {
    // A band around the diagonal keeps long books fast; generous enough for the drift
    // between traditions (text printed far from its place is found by later passes)
    align_within(a, b, same, 80 + a.len().abs_diff(b.len()))
}

/// [`align`], considering only pairs within `band` of the diagonal.
fn align_within(a: &[Weights], b: &[Weights], same: &dyn Fn(usize, usize) -> bool, band: usize) -> Vec<Group> {
    let (n, m) = (a.len(), b.len());
    let mut score = vec![vec![f32::NEG_INFINITY; m + 1]; n + 1];
    let mut back = vec![vec![0u8; m + 1]; n + 1];
    score[0][0] = 0.0;
    let ratio = if n == 0 { 1.0 } else { m as f32 / n as f32 };
    for i in 0..=n {
        let centre = (i as f32 * ratio) as usize;
        let (lo, hi) = (centre.saturating_sub(band), (centre + band).min(m));
        for j in lo..=hi {
            let here = score[i][j];
            if here == f32::NEG_INFINITY {
                continue;
            }
            let mut relax = |ni: usize, nj: usize, s: f32, mv: u8| {
                if ni <= n && nj <= m && s > score[ni][nj] {
                    score[ni][nj] = s;
                    back[ni][nj] = mv;
                }
            };
            if i < n && j < m {
                let bonus = if same(i, j) { SAME_NUMBER } else { 0.0 };
                relax(i + 1, j + 1, here + cosine(&a[i], &b[j]) + bonus, 1);
            }
            if i < n {
                relax(i + 1, j, here - GAP, 2);
            }
            if j < m {
                relax(i, j + 1, here - GAP, 3);
            }
            // A join is worth what each half matches on its own
            if i < n && j + 1 < m {
                relax(i + 1, j + 2, here + cosine(&a[i], &b[j]) + cosine(&a[i], &b[j + 1]) - JOIN, 4);
            }
            if i + 1 < n && j < m {
                relax(i + 2, j + 1, here + cosine(&a[i], &b[j]) + cosine(&a[i + 1], &b[j]) - JOIN, 5);
            }
            // Two verses in the opposite order (Matthew 23:13-14 in some editions)
            // Two verses in the opposite order (Matthew 23:13-14 in some editions): only
            // when both crossed pairs match strongly on their own
            if i + 1 < n && j + 1 < m {
                let (x, y) = (cosine(&a[i], &b[j + 1]), cosine(&a[i + 1], &b[j]));
                if x >= STRONG && y >= STRONG {
                    relax(i + 2, j + 2, here + x + y, 6);
                }
            }
        }
    }
    let mut groups = Vec::new();
    let (mut i, mut j) = (n, m);
    while i > 0 || j > 0 {
        let (ga, gb) = match back[i][j] {
            1 => (vec![i - 1], vec![j - 1]),
            2 => (vec![i - 1], vec![]),
            3 => (vec![], vec![j - 1]),
            4 => (vec![i - 1], vec![j - 2, j - 1]),
            5 => (vec![i - 2, i - 1], vec![j - 1]),
            6 => {
                // Crossed: a[i-2] with b[j-1], a[i-1] with b[j-2]
                groups.push(group(a, b, vec![i - 1], vec![j - 2], How::Content));
                groups.push(group(a, b, vec![i - 2], vec![j - 1], How::Content));
                i -= 2;
                j -= 2;
                continue;
            }
            _ => unreachable!("alignment backtrack left the band at {} {}", i, j),
        };
        i -= ga.len();
        j -= gb.len();
        groups.push(group(a, b, ga, gb, How::Content));
    }
    groups.reverse();
    groups
}

fn group(a: &[Weights], b: &[Weights], ga: Vec<usize>, gb: Vec<usize>, how: How) -> Group {
    let score = match (ga.as_slice(), gb.as_slice()) {
        ([i], [j]) => cosine(&a[*i], &b[*j]),
        ([i], [j, k]) => cosine(&a[*i], &joined(&b[*j], &b[*k])),
        ([i, k], [j]) => cosine(&joined(&a[*i], &a[*k]), &b[*j]),
        _ => 0.0,
    };
    let how = if ga.is_empty() || gb.is_empty() { How::Unmatched } else { how };
    Group { a: ga, b: gb, score, how }
}

/// A match this similar is trusted alone; a weaker strong match needs a strong
/// neighbour to anchor it.
const ANCHOR_ALONE: f32 = 0.6;
/// Weak matches between two anchors are kept when they are this many groups or fewer
/// (and the same number of verses on both sides).
const FRAME_MAX: usize = 12;
/// A later pass takes only runs of at least this many consecutive anchored matches:
/// text printed in another order moves in blocks; a stray verse is refine's to place.
const BLOCK: usize = 3;
/// At most this many passes.
const PASSES: usize = 8;

fn matched(g: &Group) -> bool {
    !g.a.is_empty() && !g.b.is_empty()
}

/// The groups of an in-order alignment that the content anchors: strong matches with a
/// strong neighbour (or very strong on their own), and the weak matches between two of
/// them with the same number of verses on both sides. Text aligned in the wrong order
/// matches strongly here and there by chance (a formula, a name), but seldom twice in
/// a row. `at` gives each side's indexes in the whole book, when the alignment is of
/// leftovers: then the weak matches must also fill the whole gap between their anchors
/// in the book, on both sides (leftovers that are neighbours may be far apart there).
fn anchored(groups: &[Group], at: Option<(&[usize], &[usize])>) -> Vec<bool> {
    let strong: Vec<bool> = groups.iter().map(|g| matched(g) && g.score >= STRONG).collect();
    let anchor: Vec<bool> = (0..groups.len())
        .map(|k| {
            strong[k] && (groups[k].score >= ANCHOR_ALONE || (k > 0 && strong[k - 1]) || (k + 1 < groups.len() && strong[k + 1]))
        })
        .collect();
    let mut keep = anchor.clone();
    let mut k = 0;
    while k < groups.len() {
        if anchor[k] {
            k += 1;
            continue;
        }
        // A run of groups between two anchors
        let start = k;
        while k < groups.len() && !anchor[k] {
            k += 1;
        }
        let run = &groups[start..k];
        let (na, nb) = (run.iter().map(|g| g.a.len()).sum::<usize>(), run.iter().map(|g| g.b.len()).sum::<usize>());
        let whole = |(at_a, at_b): (&[usize], &[usize])| {
            let (p, q) = (&groups[start - 1], &groups[k]);
            let gap = |at: &[usize], before: &[usize], after: &[usize]| match (before.last(), after.first()) {
                (Some(&x), Some(&y)) => at[y].checked_sub(at[x] + 1),
                _ => None,
            };
            gap(at_a, &p.a, &q.a) == Some(na) && gap(at_b, &p.b, &q.b) == Some(nb)
        };
        if start > 0 && k < groups.len() && na == nb && run.len() <= FRAME_MAX && at.is_none_or(whole) {
            for (x, g) in run.iter().enumerate() {
                keep[start + x] = matched(g);
            }
        }
    }
    keep
}

/// Among `keep`, the runs of at least [`BLOCK`] consecutive matches.
fn blocks(groups: &[Group], keep: &[bool]) -> Vec<bool> {
    let mut out = vec![false; groups.len()];
    let mut k = 0;
    while k < groups.len() {
        let start = k;
        while k < groups.len() && keep[k] && matched(&groups[k]) {
            k += 1;
        }
        if k - start >= BLOCK {
            out[start..k].fill(true);
        }
        k = k.max(start + 1);
    }
    out
}

/// Joins whose halves don't both match are split; the stray half is left over.
fn split_joins(a: &[Weights], b: &[Weights], groups: Vec<Group>) -> Vec<Group> {
    let mut split: Vec<Group> = Vec::new();
    for g in groups {
        match (g.a.as_slice(), g.b.as_slice()) {
            ([i], [j, k]) => {
                let (sj, sk) = (cosine(&a[*i], &b[*j]), cosine(&a[*i], &b[*k]));
                if sj < JOIN_HALF || sk < JOIN_HALF {
                    let keep = if sj >= sk { *j } else { *k };
                    split.push(group(a, b, vec![*i], vec![keep], g.how));
                } else {
                    split.push(g);
                }
            }
            ([i, k], [j]) => {
                let (si, sk) = (cosine(&a[*i], &b[*j]), cosine(&a[*k], &b[*j]));
                if si < JOIN_HALF || sk < JOIN_HALF {
                    let (keep, stray) = if si >= sk { (*i, *k) } else { (*k, *i) };
                    split.push(group(a, b, vec![keep], vec![*j], g.how));
                    split.push(group(a, b, vec![stray], vec![], How::Unmatched));
                } else {
                    split.push(g);
                }
            }
            _ => split.push(g),
        }
    }
    split
}

/// What an in-order alignment's content supports: strong matches, and weak ones
/// between strong matches (or the ends of the book) with the same number of verses on
/// both sides, marked [`How::Framed`]; every other verse of `a` left over on its own.
fn judge(a: &[Weights], b: &[Weights], groups: Vec<Group>) -> Vec<Group> {
    let mut groups = split_joins(a, b, groups);
    // In `a`'s order; `b`'s unmatched verses last, where they still count against a run
    // after the last strong match
    groups.sort_by_key(|g| g.a.first().copied().unwrap_or(usize::MAX));
    let strong = |g: &Group| matched(g) && g.score >= STRONG;
    let mut out: Vec<Group> = Vec::new();
    for (k, g) in groups.iter().enumerate() {
        if g.a.is_empty() {
            continue;
        }
        let framed = matched(g) && {
            let prev = groups[..k].iter().rposition(strong);
            let next = groups[k + 1..].iter().position(strong).map(|x| x + k + 1);
            let between = &groups[prev.map_or(0, |p| p + 1)..next.unwrap_or(groups.len())];
            strong(g) || between.iter().map(|g| g.a.len()).sum::<usize>() == between.iter().map(|g| g.b.len()).sum::<usize>()
        };
        if framed {
            let mut g = g.clone();
            if !strong(&g) {
                g.how = How::Framed;
            }
            out.push(g);
        } else {
            out.extend(g.a.iter().map(|&i| Group { a: vec![i], b: vec![], score: 0.0, how: How::Unmatched }));
        }
    }
    out
}

/// Align `a` with `b` (`b[..in_order]` the book itself, then related books) and judge
/// what the content supports. The first pass lines the whole book up in order, as
/// [`align`]. Each later pass lines up, in order and with no band, the verses on both
/// sides that no anchored match has taken (the related books' too), and takes the
/// blocks it finds: text printed in another order. The first pass is judged as it
/// always has been ([`judge`]) and keeps every match no block touched, so a book in
/// which no block is found aligns exactly as an in-order book does.
///
/// Returns the matches kept and every other verse of `a` on its own, for [`refine`].
pub fn align_blocks(a: &[Weights], b: &[Weights], in_order: usize, same: &dyn Fn(usize, usize) -> bool) -> Vec<Group> {
    let first = align(a, &b[..in_order], same);
    let anchors = anchored(&first, None);
    let mut rest_a: Vec<usize> = first.iter().zip(&anchors).filter(|(_, k)| !**k).flat_map(|(g, _)| g.a.clone()).collect();
    let mut rest_b: Vec<usize> = first.iter().zip(&anchors).filter(|(_, k)| !**k).flat_map(|(g, _)| g.b.clone()).collect();
    rest_a.sort_unstable();
    rest_b.sort_unstable();
    rest_b.extend(in_order..b.len());
    let mut found: Vec<Group> = Vec::new();
    for _ in 1..PASSES {
        if rest_a.is_empty() || rest_b.is_empty() {
            break;
        }
        let sa: Vec<Weights> = rest_a.iter().map(|&i| a[i].clone()).collect();
        let sb: Vec<Weights> = rest_b.iter().map(|&j| b[j].clone()).collect();
        let sub_same = |i: usize, j: usize| same(rest_a[i], rest_b[j]);
        let groups = align_within(&sa, &sb, &sub_same, sa.len().max(sb.len()));
        let keep = blocks(&groups, &anchored(&groups, Some((&rest_a, &rest_b))));
        let (mut used_a, mut used_b) = (HashSet::new(), HashSet::new());
        for (g, k) in groups.into_iter().zip(keep) {
            if !k {
                continue;
            }
            let ga: Vec<usize> = g.a.iter().map(|&i| rest_a[i]).collect();
            let gb: Vec<usize> = g.b.iter().map(|&j| rest_b[j]).collect();
            used_a.extend(ga.iter().copied());
            used_b.extend(gb.iter().copied());
            let how = if g.score >= STRONG { How::Content } else { How::Framed };
            found.push(Group { a: ga, b: gb, how, ..g });
        }
        if used_a.is_empty() {
            break;
        }
        rest_a.retain(|i| !used_a.contains(i));
        rest_b.retain(|j| !used_b.contains(j));
    }
    // A block matched just as the first pass matched it isn't text found elsewhere:
    // the first pass keeps it, for judging its neighbours
    let in_place: HashSet<(Vec<usize>, Vec<usize>)> = first.iter().filter(|g| matched(g)).map(|g| (g.a.clone(), g.b.clone())).collect();
    found.retain(|g| !in_place.contains(&(g.a.clone(), g.b.clone())));
    // The first pass, judged as it always has been, less the matches that touch a verse
    // a block took (whose other verses are left over)
    let taken_a: HashSet<usize> = found.iter().flat_map(|g| g.a.iter().copied()).collect();
    let taken_b: HashSet<usize> = found.iter().flat_map(|g| g.b.iter().copied()).collect();
    let mut out: Vec<Group> = Vec::new();
    for g in judge(a, b, first) {
        if g.a.iter().any(|i| taken_a.contains(i)) || g.b.iter().any(|j| taken_b.contains(j)) {
            out.extend(g.a.iter().filter(|i| !taken_a.contains(i)).map(|&i| Group { a: vec![i], b: vec![], score: 0.0, how: How::Unmatched }));
        } else {
            out.push(g);
        }
    }
    out.extend(split_joins(a, b, found).into_iter().map(|g| match g.how {
        // A stray half of a join is left over like any other verse
        How::Unmatched => g,
        _ if g.score < STRONG => Group { how: How::Framed, ..g },
        _ => g,
    }));
    out.sort_by_key(|g| g.a.first().copied().unwrap_or(usize::MAX));
    out
}

/// The second pass. `groups` are [`align_blocks`]'s: the matches kept, and every other
/// verse of `a` on its own; `b` may continue with related books' verses (KJV Susanna
/// for Douay-Rheims Daniel 13). Returns groups in `a`'s order, then `b`'s unmatched
/// verses.
/// `partners(i)`: the indexes in `b` that a[i]'s number stands for (one verse, a
/// bridged verse's whole range, or none).
pub fn refine(a: &[Weights], b: &[Weights], groups: Vec<Group>, partners: &dyn Fn(usize) -> Vec<usize>) -> Vec<Group> {
    let mut left_a: Vec<usize> = Vec::new();
    let mut left_b: HashSet<usize> = (0..b.len()).collect();
    let mut kept: Vec<Group> = Vec::new();
    for g in groups {
        if matched(&g) {
            for j in &g.b {
                left_b.remove(j);
            }
            kept.push(g);
        } else {
            left_a.extend(g.a.iter().copied());
        }
    }

    // Leftovers. A verse pairs with the KJV verse(s) its number stands for when the
    // text agrees a little, the verse is empty here, or a neighbour is paired the same
    // way; it moves to another verse only when that verse is clearly more similar
    // (reordered or relocated text). Moves are taken strongest first; number pairs
    // spread through runs of leftovers from paired neighbours.
    let mut paired: HashMap<usize, Vec<usize>> = kept.iter().filter(|g| g.a.len() == 1).map(|g| (g.a[0], g.b.clone())).collect();
    let mut rest: Vec<usize> = left_a;
    rest.sort_unstable();
    rest.dedup();

    let best_move = |i: usize, exclude: &[usize], left_b: &HashSet<usize>| -> Option<(f32, usize)> {
        left_b
            .iter()
            .copied()
            .filter(|j| !exclude.contains(j))
            .map(|j| (cosine(&a[i], &b[j]), j))
            .filter(|(s, _)| *s >= RECOVER)
            .max_by(|x, y| x.0.total_cmp(&y.0).then(y.1.cmp(&x.1)))
    };
    // The KJV verses `i`'s number stands for, if all are still free
    let by_number = |i: usize, left_b: &HashSet<usize>| -> Option<Vec<usize>> {
        let p = partners(i);
        (!p.is_empty() && p.iter().all(|j| left_b.contains(j))).then_some(p)
    };
    let number_score = |i: usize, p: &[usize]| -> f32 {
        let joined_b = p.iter().fold(Weights::default(), |acc, &j| joined(&acc, &b[j]));
        cosine(&a[i], &joined_b)
    };

    loop {
        let mut changed = false;
        // Number pairs that nothing clearly beats
        let mut still = Vec::new();
        for i in rest {
            let Some(p) = by_number(i, &left_b) else {
                still.push(i);
                continue;
            };
            let s = number_score(i, &p);
            let neighbour = (i > 0 && paired.get(&(i - 1)).is_some_and(|q| *q == partners(i - 1)))
                || paired.get(&(i + 1)).is_some_and(|q| *q == partners(i + 1));
            // An empty verse (left out here, its number kept) is the weakest evidence:
            // it waits until every move has been made
            let supported = !a[i].is_empty() && (s >= NUMBER_MIN || neighbour);
            let beaten = best_move(i, &p, &left_b).is_some_and(|(m, _)| m >= s + MOVE_MARGIN);
            if supported && !beaten {
                for j in &p {
                    left_b.remove(j);
                }
                paired.insert(i, p.clone());
                kept.push(Group { a: vec![i], b: p, score: s, how: How::Number });
                changed = true;
            } else {
                still.push(i);
            }
        }
        rest = still;
        // Then the single strongest move, so number pairs can follow from it
        let mut best: Option<(f32, usize, usize)> = None;
        for &i in &rest {
            if let Some((s, j)) = best_move(i, &[], &left_b)
                && best.is_none_or(|(bs, bi, bj)| s > bs || (s == bs && (i, j) < (bi, bj)))
            {
                best = Some((s, i, j));
            }
        }
        if let Some((s, i, j)) = best {
            left_b.remove(&j);
            paired.insert(i, vec![j]);
            rest.retain(|&x| x != i);
            kept.push(Group { a: vec![i], b: vec![j], score: s, how: How::Moved });
            changed = true;
        }
        if !changed {
            break;
        }
    }
    // Last, empty verses take the verse their number stands for, if it's still free
    let mut unmatched = Vec::new();
    for i in rest {
        match by_number(i, &left_b).filter(|_| a[i].is_empty()) {
            Some(p) => {
                for j in &p {
                    left_b.remove(j);
                }
                kept.push(Group { a: vec![i], b: p, score: 0.0, how: How::Number });
            }
            None => unmatched.push(i),
        }
    }
    kept.extend(unmatched.into_iter().map(|i| Group { a: vec![i], b: vec![], score: 0.0, how: How::Unmatched }));

    // A bridged verse matched to part of its range takes the rest of it, if free
    for g in kept.iter_mut() {
        let [i] = g.a[..] else { continue };
        let p = partners(i);
        if p.len() > 1 && !g.b.is_empty() && g.b.iter().all(|j| p.contains(j)) && p.iter().all(|j| g.b.contains(j) || left_b.contains(j)) {
            for j in &p {
                left_b.remove(j);
            }
            g.b = p;
        }
    }
    kept.sort_by_key(|g| g.a.first().copied().unwrap_or(usize::MAX));
    let mut rest_b: Vec<usize> = left_b.into_iter().collect();
    rest_b.sort_unstable();
    kept.extend(rest_b.into_iter().map(|j| Group { a: vec![], b: vec![j], score: 0.0, how: How::Unmatched }));
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    fn pairs(g: &[Group]) -> Vec<(Vec<usize>, Vec<usize>)> {
        g.iter().map(|g| (g.a.clone(), g.b.clone())).collect()
    }

    fn run(other: &[String], kjv: &[String], same: &dyn Fn(usize, usize) -> bool) -> Vec<Group> {
        let (a, b) = weigh(other, kjv);
        let partners = |i: usize| (0..kjv.len()).filter(|&j| same(i, j)).collect::<Vec<_>>();
        refine(&a, &b, align_blocks(&a, &b, kjv.len(), same), &partners)
    }

    #[test]
    fn blocks_printed_in_another_order() {
        // The Septuagint prints Jeremiah's oracles against the nations (KJV 46-51) after
        // 25:13, and the KJV's 26-45 after them: two blocks, swapped
        let first = [
            "Thus saith the LORD of hosts; Behold, I will break the bow of Elam, the chief of their might.",
            "And upon Elam will I bring the four winds from the four quarters of heaven, and will scatter them toward all those winds",
            "For I will cause Elam to be dismayed before their enemies, and before them that seek their life",
            "And I will set my throne in Elam, and will destroy from thence the king and the princes",
            "But it shall come to pass in the latter days, that I will bring again the captivity of Elam",
        ];
        let second = [
            "In the beginning of the reign of Jehoiakim the son of Josiah king of Judah came this word from the LORD",
            "Stand in the court of the LORD's house, and speak unto all the cities of Judah, which come to worship",
            "If so be they will hearken, and turn every man from his evil way, that I may repent me of the evil",
            "And thou shalt say unto them, If ye will not hearken to me, to walk in my law, which I have set before you",
            "To hearken to the words of my servants the prophets, whom I sent unto you, both rising up early",
            "Then will I make this house like Shiloh, and will make this city a curse to all the nations of the earth.",
        ];
        let kjv: Vec<String> = second.iter().chain(&first).map(|x| x.to_string()).collect();
        let other: Vec<String> = first.iter().chain(&second).map(|x| x.replace("LORD", "Lord").replace("thence", "there")).collect();
        let g = run(&other, &kjv, &|i, j| i == j);
        let mut got = pairs(&g);
        got.sort();
        let want: Vec<(Vec<usize>, Vec<usize>)> = (0..5).map(|i| (vec![i], vec![i + 6])).chain((0..6).map(|i| (vec![i + 5], vec![i]))).collect();
        assert_eq!(got, want, "{:?}", g);
        assert!(g.iter().all(|g| g.how == How::Content), "{:?}", g);
    }

    #[test]
    fn titles_counted_as_verses_and_joined_verses() {
        // KJV: title + 3 verses. Other: the title is verse 1, and verses 2-3 are joined
        let kjv = s(&[
            "To the chief Musician, A Psalm of David, when Nathan the prophet came unto him",
            "Have mercy upon me, O God, according to thy lovingkindness",
            "Wash me throughly from mine iniquity, and cleanse me from my sin.",
            "For I acknowledge my transgressions: and my sin is ever before me.",
        ]);
        let other = s(&[
            "For the Leader. A Psalm of David; when Nathan the prophet came unto him",
            "Be gracious unto me, O God, according to Thy mercy; wash me thoroughly from mine iniquity, and cleanse me from my sin.",
            "For I know my transgressions; and my sin is ever before me.",
        ]);
        let g = run(&other, &kjv, &|_, _| false);
        assert_eq!(pairs(&g), [(vec![0], vec![0]), (vec![1], vec![1, 2]), (vec![2], vec![3])]);
        assert!(g.iter().all(|g| g.score > 0.3 && g.how == How::Content), "{:?}", g);
    }

    #[test]
    fn a_verse_with_no_counterpart() {
        // BSB leaves out Matthew 17:21
        let kjv = s(&[
            "And Jesus said unto them, Because of your unbelief: for verily I say unto you, If ye have faith as a grain of mustard seed",
            "Howbeit this kind goeth not out but by prayer and fasting.",
            "And while they abode in Galilee, Jesus said unto them, The Son of man shall be betrayed into the hands of men",
        ]);
        let bsb = s(&[
            "Because you have so little faith, He answered. For truly I tell you, if you have faith the size of a mustard seed",
            "As they were gathering in Galilee, Jesus told them, The Son of Man is about to be delivered into the hands of men.",
        ]);
        // BSB numbers them 20 and 22; the KJV 20, 21, 22
        let g = run(&bsb, &kjv, &|i, j| (i, j) == (0, 0) || (i, j) == (1, 2));
        assert_eq!(pairs(&g), [(vec![0], vec![0]), (vec![1], vec![2]), (vec![], vec![1])]);
    }

    #[test]
    fn swapped_verses_and_relocated_text() {
        // Philippians 1:16-17 in the opposite order, and a doxology printed elsewhere
        let kjv = s(&[
            "Some indeed preach Christ even of envy and strife; and some also of good will:",
            "The one preach Christ of contention, not sincerely, supposing to add affliction to my bonds:",
            "But the other of love, knowing that I am set for the defence of the gospel.",
            "Now to him that is of power to stablish you according to my gospel, and the preaching of Jesus Christ",
        ]);
        let other = s(&[
            "Now to him who is able to establish you according to my Good News and the preaching of Jesus Christ",
            "Some indeed preach Christ even out of envy and strife, and some also out of good will.",
            "The latter do it out of love, knowing that I am appointed for the defense of the Good News.",
            "The former proclaim Christ out of selfish ambition, not sincerely, supposing to add affliction to my chains.",
        ]);
        let g = run(&other, &kjv, &|i, j| i == j);
        let mut got = pairs(&g);
        got.sort();
        assert_eq!(got, [(vec![0], vec![3]), (vec![1], vec![0]), (vec![2], vec![2]), (vec![3], vec![1])], "{:?}", g);
    }

    #[test]
    fn an_empty_verse_does_not_take_text_printed_elsewhere() {
        // The WEB prints Romans 16:25-27 at 14:24-26 and keeps 16:25 empty
        let kjv = s(&[
            "The grace of our Lord Jesus Christ be with you all. Amen.",
            "Now to him that is of power to stablish you according to my gospel, and the preaching of Jesus Christ",
        ]);
        let web = s(&["Now to him who is able to establish you according to my Good News and the preaching of Jesus Christ", "The grace of our Lord Jesus Christ be with you all! Amen.", ""]);
        // WEB 16:24 is KJV 16:24 and WEB 16:25 is KJV 16:25 by number; WEB 14:24 has no KJV number
        let g = run(&web, &kjv, &|i, j| (i, j) == (1, 0) || (i, j) == (2, 1));
        let mut got = pairs(&g);
        got.sort();
        assert_eq!(got, [(vec![0], vec![1]), (vec![1], vec![0]), (vec![2], vec![])], "{:?}", g);
    }

    #[test]
    fn leftovers_with_the_same_number_pair_last() {
        // An empty verse (left out, with a footnote) keeps its number's place
        let kjv = s(&["Two women shall be grinding together", "Two men shall be in the field; the one shall be taken"]);
        let other = s(&["There will be two grinding grain together", ""]);
        let g = run(&other, &kjv, &|i, j| i == j);
        assert_eq!(pairs(&g), [(vec![0], vec![0]), (vec![1], vec![1])]);
        // Kept in place: the ends of the run frame it
        assert_eq!(g[1].how, How::Framed);
    }
}

// ---------------------------------------------------------------- whole translations

/// The KJV book a translation's book is aligned with in order, and the KJV books
/// whose leftover verses it may also match: Douay-Rheims Daniel 13 is the KJV's
/// Susanna, its Esther 10:4-16:24 the KJV's Additions to Esther.
pub fn kjv_books_for(code: &str) -> Option<(&str, &'static [&'static str])> {
    Some(match code {
        "EST" => ("EST", &["ESG"]),
        "ESG" => ("ESG", &["EST"]),
        "DAN" => ("DAN", &["S3Y", "SUS", "BEL"]),
        "DAG" => ("DAN", &["S3Y", "SUS", "BEL"]),
        "S3Y" | "SUS" | "BEL" => (code, &["DAN"]),
        // The KJV prints the Letter of Jeremiah as Baruch 6
        "LJE" => ("BAR", &[]),
        // Books the KJV doesn't have
        "3MA" | "4MA" | "PS2" | "PSS" => return None,
        other => (other, &[]),
    })
}

/// A verse as the aligner sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Verse {
    pub chapter: u32,
    /// "16", "1-2", "0" for a title
    pub number: String,
    pub title: bool,
    pub text: String,
}

/// One row of a translation's alignment: its verses `native` (in its book `book`)
/// correspond to the KJV's verses `kjv` (book code, chapter, number).
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub book: String,
    pub native: Vec<(u32, String)>,
    pub kjv: Vec<(String, u32, String)>,
    pub how: How,
    pub score: f32,
}

impl Row {
    /// One native verse matched to the KJV verse of the same book and number.
    pub fn is_same_number(&self) -> bool {
        self.native.len() == 1
            && self.kjv.len() == 1
            && self.kjv[0].0 == self.book
            && self.kjv[0].1 == self.native[0].0
            && self.kjv[0].2 == self.native[0].1
    }
}

/// Align book `code` of a translation (`native`) with the KJV, whose books come from
/// `kjv(code)`. Returns every native verse in exactly one row, then rows for the
/// KJV verses (of the book itself and its related books) left without a counterpart.
pub fn align_book(code: &str, native: &[Verse], kjv: &dyn Fn(&str) -> Option<Vec<Verse>>) -> Vec<Row> {
    let unmatched = |v: &Verse| Row { book: code.to_string(), native: vec![(v.chapter, v.number.clone())], kjv: vec![], how: How::Unmatched, score: 0.0 };
    let Some((primary, mut related)) = kjv_books_for(code) else {
        return native.iter().map(unmatched).collect();
    };
    // The Septuagint's Ezra (2 Esdras) goes on with Nehemiah as chapters 11-23
    if code == "EZR" && native.iter().any(|v| v.chapter > 10) {
        related = &["NEH"];
    }
    let Some(first) = kjv(primary) else {
        return native.iter().map(unmatched).collect();
    };
    let in_order = first.len();
    let mut k: Vec<(String, Verse)> = first.into_iter().map(|v| (primary.to_string(), v)).collect();
    for r in related {
        k.extend(kjv(r).unwrap_or_default().into_iter().map(|v| (r.to_string(), v)));
    }
    let ta: Vec<String> = native.iter().map(|v| v.text.clone()).collect();
    let tk: Vec<String> = k.iter().map(|v| v.1.text.clone()).collect();
    let (wa, wk) = weigh(&ta, &tk);
    // The KJV verses a native verse's number stands for: the same-numbered verse, or
    // every verse of a bridged verse's range ("24-30"). Worked out once per verse.
    let index: HashMap<(u32, &str, bool), usize> =
        k[..in_order].iter().enumerate().map(|(j, (_, w))| ((w.chapter, w.number.as_str(), w.title), j)).collect();
    let lists: Vec<Vec<usize>> = native
        .iter()
        .map(|v| {
            let range = v.number.split_once('-').and_then(|(lo, hi)| Some((lo.parse::<u32>().ok()?, hi.parse::<u32>().ok()?)));
            match range {
                Some((lo, hi)) if !v.title => (lo..=hi).filter_map(|x| index.get(&(v.chapter, x.to_string().as_str(), false)).copied()).collect(),
                _ => index.get(&(v.chapter, v.number.as_str(), v.title)).copied().into_iter().collect(),
            }
        })
        .collect();
    let partners = |i: usize| -> Vec<usize> { lists[i].clone() };
    let same = |i: usize, j: usize| lists[i].contains(&j);
    let groups = refine(&wa, &wk, align_blocks(&wa, &wk, in_order, &same), &partners);
    groups
        .into_iter()
        .map(|g| Row {
            book: code.to_string(),
            native: g.a.iter().map(|&i| (native[i].chapter, native[i].number.clone())).collect(),
            kjv: g.b.iter().map(|&j| (k[j].0.clone(), k[j].1.chapter, k[j].1.number.clone())).collect(),
            how: g.how,
            score: g.score,
        })
        .collect()
}
