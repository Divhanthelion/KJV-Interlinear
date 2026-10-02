//! The verse alignment tables (data/library/alignment/), read as the app reads them.

use std::path::Path;
use std::sync::OnceLock;

use kjv_library::alignment::Ref;
use kjv_library::{Library, library::build};

fn library() -> &'static Library {
    static LIB: OnceLock<Library> = OnceLock::new();
    LIB.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let (bytes, _) = build::archive(&root, &|b| zstd::encode_all(b, 1).unwrap()).unwrap();
        Library::open(bytes).unwrap()
    })
}

fn r(book: &str, chapter: u32, verse: &str) -> Ref {
    (book.to_string(), chapter, verse.to_string())
}

fn map(from: &str, to: &str, at: Ref) -> Vec<Ref> {
    library().map(from, to, &at).unwrap()
}

#[test]
fn every_row_names_real_verses() {
    let lib = library();
    let mut problems = Vec::new();
    for b in lib.bibles().iter().filter(|b| b.id != "kjv") {
        let a = lib.alignment(&b.id).unwrap();
        for native in a.listed() {
            if !lib.has_verse(&b.id, native) {
                problems.push(format!("{}: {:?} is not a verse there", b.id, native));
            }
            for k in a.to_kjv(native, &|_| true) {
                if !lib.has_verse("kjv", &k) {
                    problems.push(format!("{}: {:?} maps to {:?}, not a KJV verse", b.id, native, k));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.iter().take(30).cloned().collect::<Vec<_>>().join("\n"));
}

#[test]
fn the_hard_cases() {
    // The Vulgate's Psalms (Douay-Rheims)
    assert_eq!(map("kjv", "dra", r("PSA", 23, "1")), [r("PSA", 22, "1")]);
    assert_eq!(map("dra", "kjv", r("PSA", 9, "22")), [r("PSA", 10, "1")]);
    assert_eq!(map("kjv", "dra", r("PSA", 116, "10")), [r("PSA", 115, "1")]);
    assert_eq!(map("dra", "kjv", r("PSA", 147, "1")), [r("PSA", 147, "12")]);
    // Between two translations that both renumber (through the KJV): Douay-Rheims 22:1
    // holds the title and verse 1, which Brenton prints as 22:0 and 22:1
    assert_eq!(map("dra", "brenton", r("PSA", 22, "1")), [r("PSA", 22, "0"), r("PSA", 22, "1")]);
    assert_eq!(map("kjv", "brenton", r("PSA", 23, "1")), [r("PSA", 22, "1")]);
    // The WEB prints the Romans doxology at 14:24-26
    assert_eq!(map("kjv", "web", r("ROM", 16, "25")), [r("ROM", 14, "24")]);
    assert_eq!(map("web", "kjv", r("ROM", 14, "26")), [r("ROM", 16, "27")]);
    // Swapped verses
    assert_eq!(map("web", "kjv", r("MAT", 23, "13")), [r("MAT", 23, "14")]);
    // Susanna and Esther's additions, which the KJV prints in its Apocrypha
    assert_eq!(map("dra", "kjv", r("DAN", 13, "1")), [r("SUS", 1, "1")]);
    assert_eq!(map("dra", "kjv", r("EST", 11, "2")), [r("ESG", 11, "2")]);
    // Hebrew numbering (the Orthodox Jewish Bible's Malachi 3:19 is the KJV's 4:1)
    assert_eq!(map("ojb", "kjv", r("MAL", 3, "19")), [r("MAL", 4, "1")]);
    // A verse the BSB leaves out
    assert_eq!(map("kjv", "bsb", r("MAT", 17, "21")), Vec::<Ref>::new());
    // The same number everywhere else
    assert_eq!(map("kjv", "web", r("JHN", 3, "16")), [r("JHN", 3, "16")]);
}
