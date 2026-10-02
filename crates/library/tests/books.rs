//! Every book in the translations has a place in the app's book list.

use std::path::Path;

use kjv_library::books;

#[test]
fn every_book_in_the_library_is_known() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/library/bibles");
    let mut unknown = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        let index = std::fs::read_to_string(entry.path().join("index.toml")).unwrap();
        for line in index.lines() {
            if let Some(code) = line.strip_prefix("code = ") {
                let code = code.trim_matches('"');
                if books::by_code(code).is_none() {
                    unknown.push(format!("{} in {}", code, entry.file_name().to_string_lossy()));
                }
            }
        }
    }
    assert!(unknown.is_empty(), "not in books::BOOKS: {:?}", unknown);
}
