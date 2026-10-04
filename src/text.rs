//! Text helpers shared by search, highlighting, and original-language display.

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

/// Fold a whole string for search comparisons.
pub fn fold_for_search(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
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

/// Hebrew points, accents, and joiners that attach to the preceding letter.
fn is_hebrew_mark(c: char) -> bool {
    matches!(
        c,
        '\u{0591}'
            ..='\u{05BD}'
                | '\u{05BF}'
                | '\u{05C1}'
                | '\u{05C2}'
                | '\u{05C4}'
                | '\u{05C5}'
                | '\u{05C7}'
                | '\u{034F}'
                | '\u{200C}'
                | '\u{200D}'
    )
}

/// Reorder a Hebrew word for egui, which lays text out left-to-right only.
///
/// Letters are reversed as clusters (letter + its points) so the word reads
/// right-to-left on screen while vowels stay on their letters.
pub fn hebrew_visual(word: &str) -> String {
    let mut clusters: Vec<String> = Vec::new();
    for c in word.chars() {
        match clusters.last_mut() {
            Some(last) if is_hebrew_mark(c) => last.push(c),
            _ => clusters.push(c.to_string()),
        }
    }
    clusters.reverse();
    clusters.concat()
}

fn is_hebrew_letter(c: char) -> bool {
    matches!(c, '\u{05D0}'..='\u{05EA}' | '\u{05EF}'..='\u{05F4}' | '\u{FB1D}'..='\u{FB4F}')
}

/// Characters that end a Hebrew run inside left-to-right text.
fn is_strong_ltr(c: char) -> bool {
    c.is_ascii_alphanumeric() || (c.is_alphabetic() && !is_hebrew_letter(c) && !is_hebrew_mark(c))
}

fn mirror(c: char) -> char {
    match c {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        '<' => '>',
        '>' => '<',
        c => c,
    }
}

/// Reorder mixed English/Hebrew text for egui's left-to-right layout: each run of
/// Hebrew (with the spaces and punctuation between its words) is shown right-to-left.
/// For a single line of text; wrapped lines may break a long Hebrew run oddly.
pub fn visual_bidi(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if !is_hebrew_letter(chars[i]) {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        // Run: from this letter to the last Hebrew letter (plus its marks) before any
        // left-to-right letter or digit
        let mut end = i + 1;
        let mut j = i + 1;
        while j < chars.len() && !is_strong_ltr(chars[j]) {
            if is_hebrew_letter(chars[j]) || is_hebrew_mark(chars[j]) {
                end = j + 1;
            }
            j += 1;
        }
        let run: String = chars[i..end].iter().map(|&c| mirror(c)).collect();
        out.push_str(&hebrew_visual(&run));
        i = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folded_search_matches_apostrophe_and_ae() {
        assert_eq!(
            fold_for_search("Moses\u{2019} seat"),
            fold_for_search("moses' SEAT")
        );
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
    fn hebrew_visual_keeps_points_on_letters() {
        // אֱלֹהִים: alef+hataf-segol, lamed+holam, he+hiriq, yod, final mem
        let word = "\u{05D0}\u{05B1}\u{05DC}\u{05B9}\u{05D4}\u{05B4}\u{05D9}\u{05DD}";
        let visual = hebrew_visual(word);
        assert_eq!(
            visual,
            "\u{05DD}\u{05D9}\u{05D4}\u{05B4}\u{05DC}\u{05B9}\u{05D0}\u{05B1}"
        );
        // Reversing twice restores logical order.
        assert_eq!(hebrew_visual(&visual), word);
    }

    #[test]
    fn visual_bidi_reverses_only_hebrew_runs() {
        // יהוה inside English
        assert_eq!(
            visual_bidi("name of ye.ho.vah (\u{05D9}\u{05D4}\u{05D5}\u{05D4} \"LORD\" H3068G)"),
            "name of ye.ho.vah (\u{05D4}\u{05D5}\u{05D4}\u{05D9} \"LORD\" H3068G)"
        );
        // Two Hebrew words separated by ", " form one run: word order reverses too
        assert_eq!(
            visual_bidi("for \u{05D0}\u{05D1}, \u{05D2}\u{05D3} etc"),
            "for \u{05D3}\u{05D2} ,\u{05D1}\u{05D0} etc"
        );
        assert_eq!(visual_bidi("plain English"), "plain English");
    }
}
