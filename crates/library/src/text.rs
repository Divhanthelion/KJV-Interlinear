//! Text folded for searching: case and typography set aside, so "Moses’" finds
//! "moses'" and "Caesar" finds "Cæsar".

/// Fold one char for case/typography-insensitive search.
/// Curly apostrophes match straight ones and "æ" matches "ae" (Cæsar ↔ Caesar).
pub fn fold_char(c: char, out: &mut String) {
    match c {
        '\u{2018}' | '\u{2019}' | '\u{201B}' | '\u{02BC}' => out.push('\''),
        '\u{201C}' | '\u{201D}' => out.push('"'),
        '\u{2010}'..='\u{2014}' => out.push('-'),
        'æ' | 'Æ' => out.push_str("ae"),
        c if c.is_whitespace() => out.push(' '),
        c => out.extend(c.to_lowercase()),
    }
}

/// Fold a whole string for search comparisons.
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        fold_char(c, &mut out);
    }
    out
}

/// Byte ranges in `haystack` where the folded `needle` occurs (non-overlapping).
pub fn find_folded(haystack: &str, needle: &str) -> Vec<(usize, usize)> {
    let needle = fold(needle);
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

/// The words of folded text, for the search index: its runs of letters and digits.
/// Every run of letters and digits in a folded query lies inside one of these in any
/// text the query is found in, which is what lets the index rule texts out.
pub fn words(folded: &str) -> impl Iterator<Item = &str> {
    folded.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folded_search_matches_apostrophe_and_ae() {
        assert_eq!(fold("Moses\u{2019} seat"), fold("moses' SEAT"));
        assert_eq!(fold("Cæsar"), "caesar");
    }

    #[test]
    fn ranges_map_back_to_original_bytes() {
        let text = "unto Cæsar the things which are Cæsar\u{2019}s";
        let r = find_folded(text, "caesar's");
        assert_eq!(r.len(), 1);
        assert_eq!(&text[r[0].0..r[0].1], "Cæsar\u{2019}s");
        let r = find_folded(text, "CAESAR");
        assert_eq!(r.len(), 2);
        assert_eq!(&text[r[0].0..r[0].1], "Cæsar");
    }

    #[test]
    fn words_split_at_everything_but_letters_and_digits() {
        let folded = fold("The LORD’s—house, 3:16; Cæsar");
        let w: Vec<&str> = words(&folded).collect();
        assert_eq!(w, ["the", "lord", "s", "house", "3", "16", "caesar"]);
    }
}
