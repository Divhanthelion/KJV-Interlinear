//! Aligning one translation's verses with the KJV's, by what they say.
//!
//! Translations number verses differently: the Douay-Rheims follows the Vulgate's
//! Psalms, the Septuagint orders Jeremiah differently, Jewish editions count Psalm
//! titles as verses, some translations join, split, swap, or leave out verses. Every
//! text in the library is English, so a verse can be matched to its KJV counterpart
//! by content: the distinctive words they share (names, numbers, rarer words).
//!
//! Two passes:
//! 1. [`align`] lines the verses up in order (a sequence alignment allowing one-to-one
//!    matches, joins of two verses, and verses with no counterpart), with a small
//!    preference for verses that carry the same number.
//! 2. [`refine`] keeps the matches the content supports, splits joins whose halves
//!    don't both match, and pairs whatever is left by best similarity anywhere in the
//!    book and its related books (reordered chapters, additions printed elsewhere);
//!    last of all, leftovers with the same number on both sides are paired "by number".
//!
//! Every group carries how it was matched, so the alignment tables can be reviewed.

use std::collections::{HashMap, HashSet};

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

/// A verse's comparable words and their weights.
pub type Weights = HashMap<String, f32>;

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
        let mut m = Weights::new();
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

/// Align `a` with `b` in order. `same(i, j)`: whether a[i] and b[j] carry the same
/// verse number. Every index of each side appears in exactly one group.
pub fn align(a: &[Weights], b: &[Weights], same: &dyn Fn(usize, usize) -> bool) -> Vec<Group> {
    let (n, m) = (a.len(), b.len());
    let mut score = vec![vec![f32::NEG_INFINITY; m + 1]; n + 1];
    let mut back = vec![vec![0u8; m + 1]; n + 1];
    score[0][0] = 0.0;
    // A band around the diagonal keeps long books fast; generous enough for the
    // largest drift between traditions
    let band = 80 + n.abs_diff(m);
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

/// The second pass. `groups` aligned `a` with the first verses of `b`; `b` may
/// continue with related books' verses (KJV Susanna for Douay-Rheims Daniel 13).
/// Returns groups in `a`'s order, then `b`'s unmatched verses.
pub fn refine(a: &[Weights], b: &[Weights], groups: Vec<Group>, same: &dyn Fn(usize, usize) -> bool) -> Vec<Group> {
    // Joins whose halves don't both match are split; the stray half is left over
    let mut split: Vec<Group> = Vec::new();
    for g in groups {
        match (g.a.as_slice(), g.b.as_slice()) {
            ([i], [j, k]) => {
                let (sj, sk) = (cosine(&a[*i], &b[*j]), cosine(&a[*i], &b[*k]));
                if sj < JOIN_HALF || sk < JOIN_HALF {
                    let keep = if sj >= sk { *j } else { *k };
                    split.push(group(a, b, vec![*i], vec![keep], How::Content));
                } else {
                    split.push(g);
                }
            }
            ([i, k], [j]) => {
                let (si, sk) = (cosine(&a[*i], &b[*j]), cosine(&a[*k], &b[*j]));
                if si < JOIN_HALF || sk < JOIN_HALF {
                    let (keep, stray) = if si >= sk { (*i, *k) } else { (*k, *i) };
                    split.push(group(a, b, vec![keep], vec![*j], How::Content));
                    split.push(group(a, b, vec![stray], vec![], How::Unmatched));
                } else {
                    split.push(g);
                }
            }
            _ => split.push(g),
        }
    }
    split.sort_by_key(|g| g.a.first().copied().unwrap_or(usize::MAX));
    let groups = split;

    // Weak matches stay where strong matches (or the ends of the book) frame them with
    // the same number of verses on both sides
    let strong = |g: &Group| !g.a.is_empty() && !g.b.is_empty() && g.score >= STRONG;
    let mut left_a: Vec<usize> = Vec::new();
    let mut left_b: HashSet<usize> = (0..b.len()).collect();
    let mut kept: Vec<Group> = Vec::new();
    for (k, g) in groups.iter().enumerate() {
        if g.a.is_empty() || g.b.is_empty() {
            left_a.extend(g.a.iter().copied());
            continue;
        }
        let framed = strong(g) || {
            let prev = groups[..k].iter().rposition(strong);
            let next = groups[k + 1..].iter().position(strong).map(|x| x + k + 1);
            let between = &groups[prev.map_or(0, |p| p + 1)..next.unwrap_or(groups.len())];
            between.iter().map(|g| g.a.len()).sum::<usize>() == between.iter().map(|g| g.b.len()).sum::<usize>()
        };
        if framed {
            let mut g = g.clone();
            if !strong(&g) {
                g.how = How::Framed;
            }
            for j in &g.b {
                left_b.remove(j);
            }
            kept.push(g);
        } else {
            left_a.extend(g.a.iter().copied());
        }
    }

    // Leftovers paired by best similarity anywhere, strongest first
    let mut pairs: Vec<(f32, usize, usize)> = Vec::new();
    for &i in &left_a {
        for &j in &left_b {
            let s = cosine(&a[i], &b[j]);
            if s >= RECOVER {
                pairs.push((s, i, j));
            }
        }
    }
    pairs.sort_by(|x, y| y.0.total_cmp(&x.0).then(x.1.cmp(&y.1)).then(x.2.cmp(&y.2)));
    let mut done_a: HashSet<usize> = HashSet::new();
    for (s, i, j) in pairs {
        if done_a.contains(&i) || !left_b.contains(&j) {
            continue;
        }
        done_a.insert(i);
        left_b.remove(&j);
        kept.push(Group { a: vec![i], b: vec![j], score: s, how: How::Moved });
    }

    // Last: leftovers carrying the same number on both sides
    let mut rest_a: Vec<usize> = left_a.into_iter().filter(|i| !done_a.contains(i)).collect();
    rest_a.sort_unstable();
    for i in rest_a {
        let partner = left_b.iter().copied().filter(|&j| same(i, j)).min();
        match partner.filter(|&j| a[i].is_empty() || cosine(&a[i], &b[j]) >= NUMBER_MIN) {
            Some(j) => {
                left_b.remove(&j);
                kept.push(Group { a: vec![i], b: vec![j], score: cosine(&a[i], &b[j]), how: How::Number });
            }
            None => kept.push(Group { a: vec![i], b: vec![], score: 0.0, how: How::Unmatched }),
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
        refine(&a, &b, align(&a, &b, same), same)
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
