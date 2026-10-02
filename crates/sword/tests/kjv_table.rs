//! The versification table (`src/kjv.rs`) must agree with the repository's KJV text:
//! same chapters, same verse counts, nothing missing. Psalm title lines (`N:0`) are not
//! verse slots in SWORD's KJV scheme and are skipped.

use std::path::PathBuf;

use kjv_sword::kjv::{BOOKS, OT_BOOKS};

/// "1 Samuel" is stored as "First Samuel.txt" and so on.
fn file_stem(name: &str) -> String {
    for (digit, word) in [("1 ", "First "), ("2 ", "Second "), ("3 ", "Third ")] {
        if let Some(rest) = name.strip_prefix(digit) {
            return format!("{word}{rest}");
        }
    }
    name.to_string()
}

#[test]
fn table_matches_the_repository_kjv_text() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    if !root.join("old_testament").is_dir() {
        eprintln!("SKIP: old_testament/ not found next to the workspace root");
        return;
    }
    let (mut verses, mut chapters) = ([0usize; 2], [0usize; 2]);
    for (i, book) in BOOKS.iter().enumerate() {
        let dir = if i < OT_BOOKS {
            "old_testament"
        } else {
            "new_testament"
        };
        let t = usize::from(i >= OT_BOOKS);
        let path = root.join(dir).join(format!("{}.txt", file_stem(book.name)));
        let text =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut found: Vec<Vec<u32>> = Vec::new();
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let (reference, _) = line
                .split_once(' ')
                .unwrap_or_else(|| panic!("{}: {line:.60}", book.name));
            let (c, v) = reference
                .split_once(':')
                .unwrap_or_else(|| panic!("{}: {line:.60}", book.name));
            let (c, v): (usize, u32) = (
                c.parse().unwrap(),
                v.split('-').next().unwrap().parse().unwrap(),
            );
            if v == 0 {
                assert_eq!(
                    book.code, "PSA",
                    "only Psalms have title lines: {} {c}:0",
                    book.name
                );
                continue;
            }
            while found.len() < c {
                found.push(Vec::new());
            }
            found[c - 1].push(v);
        }
        assert_eq!(
            found.len(),
            book.verses.len(),
            "{}: chapter count",
            book.name
        );
        for (c, vs) in found.iter().enumerate() {
            let expected: Vec<u32> = (1..=u32::from(book.verses[c])).collect();
            assert_eq!(vs, &expected, "{} {}: verse numbers", book.name, c + 1);
        }
        chapters[t] += book.verses.len();
        verses[t] += book.verses.iter().map(|&v| v as usize).sum::<usize>();
    }
    assert_eq!(verses, [23_145, 7_957]);
    assert_eq!(chapters, [929, 260]);
}
