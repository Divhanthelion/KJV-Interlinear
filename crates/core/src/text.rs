//! Text helpers for search, highlighting, and display.

/// Fold one char for case/typography-insensitive search.
/// Curly apostrophes match straight ones and "æ" matches "ae" (Cæsar ↔ Caesar).
fn fold_char(c: char, out: &mut String) {
    match c {
        '\u{2018}' | '\u{2019}' | '\u{201B}' | '\u{02BC}' => out.push('\''),
        '\u{201C}' | '\u{201D}' => out.push('"'),
        '\u{2010}'..='\u{2014}' => out.push('-'),
        'æ' | 'Æ' => out.push_str("ae"),
        c if c.is_whitespace() => out.push(' '),
        c => out.extend(c.to_lowercase()),
    }
}

/// Fold a whole string for search comparisons. Runs of whitespace fold to one
/// space, so "God  so" finds "God so" (verse text never has two in a row).
pub fn fold_for_search(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_whitespace() && out.ends_with(' ') {
            continue;
        }
        fold_char(c, &mut out);
    }
    out
}

/// Byte ranges in `haystack` where the folded `needle` occurs (non-overlapping).
pub fn find_folded_ranges(haystack: &str, needle: &str) -> Vec<(usize, usize)> {
    let needle = fold_for_search(needle);
    if needle.is_empty() {
        return Vec::new();
    }

    // Folded text plus, for each folded byte, the original char's byte range.
    let mut folded = String::with_capacity(haystack.len());
    let mut map: Vec<(usize, usize)> = Vec::with_capacity(haystack.len());
    for (i, c) in haystack.char_indices() {
        let before = folded.len();
        fold_char(c, &mut folded);
        for _ in before..folded.len() {
            map.push((i, i + c.len_utf8()));
        }
    }

    let mut ranges = Vec::new();
    let mut start = 0;
    while let Some(pos) = folded[start..].find(&needle) {
        let abs = start + pos;
        let end = abs + needle.len();
        ranges.push((map[abs].0, map[end - 1].1));
        start = end;
    }
    ranges
}

/// A run of verse text with how to draw it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Segment {
    pub text: String,
    /// Words of Christ
    pub red: bool,
    /// Matches the search query
    pub hit: bool,
}

/// Split `text` into runs at every boundary of the red-letter and search-hit byte ranges.
pub fn segments(text: &str, red: &[(usize, usize)], hits: &[(usize, usize)]) -> Vec<Segment> {
    let mut cuts = vec![0, text.len()];
    for &(start, end) in red.iter().chain(hits) {
        cuts.push(start);
        cuts.push(end);
    }
    cuts.sort_unstable();
    cuts.dedup();
    let covers = |ranges: &[(usize, usize)], at: usize| ranges.iter().any(|&(s, e)| s <= at && at < e);

    let mut out: Vec<Segment> = Vec::new();
    for span in cuts.windows(2) {
        let (start, end) = (span[0], span[1]);
        if start == end {
            continue;
        }
        let (red, hit) = (covers(red, start), covers(hits, start));
        match out.last_mut() {
            Some(last) if last.red == red && last.hit == hit => last.text.push_str(&text[start..end]),
            _ => out.push(Segment { text: text[start..end].to_string(), red, hit }),
        }
    }
    out
}

/// Tidy a STEP gloss for display: "and/ <obj.>" -> "and (obj.)".
/// Slashes mirror Hebrew prefix/suffix boundaries; <..> marks words best left untranslated.
pub fn format_gloss(gloss: &str) -> String {
    let text = gloss.replace('/', " ").replace('<', "(").replace('>', ")");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folded_search_matches_apostrophe_and_ae() {
        assert_eq!(fold_for_search("Moses\u{2019} seat"), fold_for_search("moses' SEAT"));
        assert_eq!(fold_for_search("Cæsar"), "caesar");
    }

    #[test]
    fn ranges_map_back_to_original_bytes() {
        let text = "unto Cæsar the things which are Cæsar\u{2019}s";
        let r = find_folded_ranges(text, "caesar's");
        assert_eq!(r.len(), 1);
        assert_eq!(&text[r[0].0..r[0].1], "Cæsar\u{2019}s");
        let r = find_folded_ranges(text, "CAESAR");
        assert_eq!(r.len(), 2);
        assert_eq!(&text[r[0].0..r[0].1], "Cæsar");
    }

    #[test]
    fn query_whitespace_collapses() {
        assert_eq!(fold_for_search("God  so\t loved"), "god so loved");
        let text = "For God so loved the world";
        let r = find_folded_ranges(text, "God  so");
        assert_eq!(r.iter().map(|&(s, e)| &text[s..e]).collect::<Vec<_>>(), ["God so"]);
    }

    #[test]
    fn segments_combine_red_and_hits() {
        let text = "And Jesus said, Follow me.";
        let red = [(16, 26)];
        let hits = [(20, 26)];
        let s = segments(text, &red, &hits);
        let parts: Vec<(&str, bool, bool)> = s.iter().map(|x| (x.text.as_str(), x.red, x.hit)).collect();
        assert_eq!(parts, vec![("And Jesus said, ", false, false), ("Foll", true, false), ("ow me.", true, true)]);
        // No ranges: one plain segment
        assert_eq!(segments("plain", &[], &[]).len(), 1);
    }

    #[test]
    fn gloss_formatting() {
        assert_eq!(format_gloss("and/ <obj.>"), "and (obj.)");
        assert_eq!(format_gloss("in/ beginning"), "in beginning");
        assert_eq!(format_gloss("[The] book"), "[The] book");
    }
}
