//! The KJV versification (the SWORD "KJV" v11n): chapters and verses per book.
//!
//! Generated once from the repository's KJV text (`old_testament/*.txt`,
//! `new_testament/*.txt`, Psalm title lines excluded because SWORD's KJV scheme
//! does not give them verse slots). Book order is canonical, as in
//! `crates/core/src/api.rs`.

/// One book of the KJV versification.
#[derive(Debug, Clone, Copy)]
pub struct Book {
    /// USFM / SWORD OSIS-style code, e.g. `"JHN"`.
    pub code: &'static str,
    /// Display name, e.g. `"1 Samuel"`.
    pub name: &'static str,
    /// Verses in each chapter (index 0 is chapter 1).
    pub verses: &'static [u16],
}

/// Number of Old Testament books (the first 39 of [`BOOKS`]).
pub const OT_BOOKS: usize = 39;

/// All 66 books in canonical order.
pub const BOOKS: [Book; 66] = [
    Book {
        code: "GEN",
        name: "Genesis",
        verses: &[
            31, 25, 24, 26, 32, 22, 24, 22, 29, 32, 32, 20, 18, 24, 21, 16, 27, 33, 38, 18, 34, 24,
            20, 67, 34, 35, 46, 22, 35, 43, 55, 32, 20, 31, 29, 43, 36, 30, 23, 23, 57, 38, 34, 34,
            28, 34, 31, 22, 33, 26,
        ],
    },
    Book {
        code: "EXO",
        name: "Exodus",
        verses: &[
            22, 25, 22, 31, 23, 30, 25, 32, 35, 29, 10, 51, 22, 31, 27, 36, 16, 27, 25, 26, 36, 31,
            33, 18, 40, 37, 21, 43, 46, 38, 18, 35, 23, 35, 35, 38, 29, 31, 43, 38,
        ],
    },
    Book {
        code: "LEV",
        name: "Leviticus",
        verses: &[
            17, 16, 17, 35, 19, 30, 38, 36, 24, 20, 47, 8, 59, 57, 33, 34, 16, 30, 37, 27, 24, 33,
            44, 23, 55, 46, 34,
        ],
    },
    Book {
        code: "NUM",
        name: "Numbers",
        verses: &[
            54, 34, 51, 49, 31, 27, 89, 26, 23, 36, 35, 16, 33, 45, 41, 50, 13, 32, 22, 29, 35, 41,
            30, 25, 18, 65, 23, 31, 40, 16, 54, 42, 56, 29, 34, 13,
        ],
    },
    Book {
        code: "DEU",
        name: "Deuteronomy",
        verses: &[
            46, 37, 29, 49, 33, 25, 26, 20, 29, 22, 32, 32, 18, 29, 23, 22, 20, 22, 21, 20, 23, 30,
            25, 22, 19, 19, 26, 68, 29, 20, 30, 52, 29, 12,
        ],
    },
    Book {
        code: "JOS",
        name: "Joshua",
        verses: &[
            18, 24, 17, 24, 15, 27, 26, 35, 27, 43, 23, 24, 33, 15, 63, 10, 18, 28, 51, 9, 45, 34,
            16, 33,
        ],
    },
    Book {
        code: "JDG",
        name: "Judges",
        verses: &[
            36, 23, 31, 24, 31, 40, 25, 35, 57, 18, 40, 15, 25, 20, 20, 31, 13, 31, 30, 48, 25,
        ],
    },
    Book {
        code: "RUT",
        name: "Ruth",
        verses: &[22, 23, 18, 22],
    },
    Book {
        code: "1SA",
        name: "1 Samuel",
        verses: &[
            28, 36, 21, 22, 12, 21, 17, 22, 27, 27, 15, 25, 23, 52, 35, 23, 58, 30, 24, 42, 15, 23,
            29, 22, 44, 25, 12, 25, 11, 31, 13,
        ],
    },
    Book {
        code: "2SA",
        name: "2 Samuel",
        verses: &[
            27, 32, 39, 12, 25, 23, 29, 18, 13, 19, 27, 31, 39, 33, 37, 23, 29, 33, 43, 26, 22, 51,
            39, 25,
        ],
    },
    Book {
        code: "1KI",
        name: "1 Kings",
        verses: &[
            53, 46, 28, 34, 18, 38, 51, 66, 28, 29, 43, 33, 34, 31, 34, 34, 24, 46, 21, 43, 29, 53,
        ],
    },
    Book {
        code: "2KI",
        name: "2 Kings",
        verses: &[
            18, 25, 27, 44, 27, 33, 20, 29, 37, 36, 21, 21, 25, 29, 38, 20, 41, 37, 37, 21, 26, 20,
            37, 20, 30,
        ],
    },
    Book {
        code: "1CH",
        name: "1 Chronicles",
        verses: &[
            54, 55, 24, 43, 26, 81, 40, 40, 44, 14, 47, 40, 14, 17, 29, 43, 27, 17, 19, 8, 30, 19,
            32, 31, 31, 32, 34, 21, 30,
        ],
    },
    Book {
        code: "2CH",
        name: "2 Chronicles",
        verses: &[
            17, 18, 17, 22, 14, 42, 22, 18, 31, 19, 23, 16, 22, 15, 19, 14, 19, 34, 11, 37, 20, 12,
            21, 27, 28, 23, 9, 27, 36, 27, 21, 33, 25, 33, 27, 23,
        ],
    },
    Book {
        code: "EZR",
        name: "Ezra",
        verses: &[11, 70, 13, 24, 17, 22, 28, 36, 15, 44],
    },
    Book {
        code: "NEH",
        name: "Nehemiah",
        verses: &[11, 20, 32, 23, 19, 19, 73, 18, 38, 39, 36, 47, 31],
    },
    Book {
        code: "EST",
        name: "Esther",
        verses: &[22, 23, 15, 17, 14, 14, 10, 17, 32, 3],
    },
    Book {
        code: "JOB",
        name: "Job",
        verses: &[
            22, 13, 26, 21, 27, 30, 21, 22, 35, 22, 20, 25, 28, 22, 35, 22, 16, 21, 29, 29, 34, 30,
            17, 25, 6, 14, 23, 28, 25, 31, 40, 22, 33, 37, 16, 33, 24, 41, 30, 24, 34, 17,
        ],
    },
    Book {
        code: "PSA",
        name: "Psalms",
        verses: &[
            6, 12, 8, 8, 12, 10, 17, 9, 20, 18, 7, 8, 6, 7, 5, 11, 15, 50, 14, 9, 13, 31, 6, 10,
            22, 12, 14, 9, 11, 12, 24, 11, 22, 22, 28, 12, 40, 22, 13, 17, 13, 11, 5, 26, 17, 11,
            9, 14, 20, 23, 19, 9, 6, 7, 23, 13, 11, 11, 17, 12, 8, 12, 11, 10, 13, 20, 7, 35, 36,
            5, 24, 20, 28, 23, 10, 12, 20, 72, 13, 19, 16, 8, 18, 12, 13, 17, 7, 18, 52, 17, 16,
            15, 5, 23, 11, 13, 12, 9, 9, 5, 8, 28, 22, 35, 45, 48, 43, 13, 31, 7, 10, 10, 9, 8, 18,
            19, 2, 29, 176, 7, 8, 9, 4, 8, 5, 6, 5, 6, 8, 8, 3, 18, 3, 3, 21, 26, 9, 8, 24, 13, 10,
            7, 12, 15, 21, 10, 20, 14, 9, 6,
        ],
    },
    Book {
        code: "PRO",
        name: "Proverbs",
        verses: &[
            33, 22, 35, 27, 23, 35, 27, 36, 18, 32, 31, 28, 25, 35, 33, 33, 28, 24, 29, 30, 31, 29,
            35, 34, 28, 28, 27, 28, 27, 33, 31,
        ],
    },
    Book {
        code: "ECC",
        name: "Ecclesiastes",
        verses: &[18, 26, 22, 16, 20, 12, 29, 17, 18, 20, 10, 14],
    },
    Book {
        code: "SNG",
        name: "Song of Solomon",
        verses: &[17, 17, 11, 16, 16, 13, 13, 14],
    },
    Book {
        code: "ISA",
        name: "Isaiah",
        verses: &[
            31, 22, 26, 6, 30, 13, 25, 22, 21, 34, 16, 6, 22, 32, 9, 14, 14, 7, 25, 6, 17, 25, 18,
            23, 12, 21, 13, 29, 24, 33, 9, 20, 24, 17, 10, 22, 38, 22, 8, 31, 29, 25, 28, 28, 25,
            13, 15, 22, 26, 11, 23, 15, 12, 17, 13, 12, 21, 14, 21, 22, 11, 12, 19, 12, 25, 24,
        ],
    },
    Book {
        code: "JER",
        name: "Jeremiah",
        verses: &[
            19, 37, 25, 31, 31, 30, 34, 22, 26, 25, 23, 17, 27, 22, 21, 21, 27, 23, 15, 18, 14, 30,
            40, 10, 38, 24, 22, 17, 32, 24, 40, 44, 26, 22, 19, 32, 21, 28, 18, 16, 18, 22, 13, 30,
            5, 28, 7, 47, 39, 46, 64, 34,
        ],
    },
    Book {
        code: "LAM",
        name: "Lamentations",
        verses: &[22, 22, 66, 22, 22],
    },
    Book {
        code: "EZK",
        name: "Ezekiel",
        verses: &[
            28, 10, 27, 17, 17, 14, 27, 18, 11, 22, 25, 28, 23, 23, 8, 63, 24, 32, 14, 49, 32, 31,
            49, 27, 17, 21, 36, 26, 21, 26, 18, 32, 33, 31, 15, 38, 28, 23, 29, 49, 26, 20, 27, 31,
            25, 24, 23, 35,
        ],
    },
    Book {
        code: "DAN",
        name: "Daniel",
        verses: &[21, 49, 30, 37, 31, 28, 28, 27, 27, 21, 45, 13],
    },
    Book {
        code: "HOS",
        name: "Hosea",
        verses: &[11, 23, 5, 19, 15, 11, 16, 14, 17, 15, 12, 14, 16, 9],
    },
    Book {
        code: "JOL",
        name: "Joel",
        verses: &[20, 32, 21],
    },
    Book {
        code: "AMO",
        name: "Amos",
        verses: &[15, 16, 15, 13, 27, 14, 17, 14, 15],
    },
    Book {
        code: "OBA",
        name: "Obadiah",
        verses: &[21],
    },
    Book {
        code: "JON",
        name: "Jonah",
        verses: &[17, 10, 10, 11],
    },
    Book {
        code: "MIC",
        name: "Micah",
        verses: &[16, 13, 12, 13, 15, 16, 20],
    },
    Book {
        code: "NAM",
        name: "Nahum",
        verses: &[15, 13, 19],
    },
    Book {
        code: "HAB",
        name: "Habakkuk",
        verses: &[17, 20, 19],
    },
    Book {
        code: "ZEP",
        name: "Zephaniah",
        verses: &[18, 15, 20],
    },
    Book {
        code: "HAG",
        name: "Haggai",
        verses: &[15, 23],
    },
    Book {
        code: "ZEC",
        name: "Zechariah",
        verses: &[21, 13, 10, 14, 11, 15, 14, 23, 17, 12, 17, 14, 9, 21],
    },
    Book {
        code: "MAL",
        name: "Malachi",
        verses: &[14, 17, 18, 6],
    },
    Book {
        code: "MAT",
        name: "Matthew",
        verses: &[
            25, 23, 17, 25, 48, 34, 29, 34, 38, 42, 30, 50, 58, 36, 39, 28, 27, 35, 30, 34, 46, 46,
            39, 51, 46, 75, 66, 20,
        ],
    },
    Book {
        code: "MRK",
        name: "Mark",
        verses: &[
            45, 28, 35, 41, 43, 56, 37, 38, 50, 52, 33, 44, 37, 72, 47, 20,
        ],
    },
    Book {
        code: "LUK",
        name: "Luke",
        verses: &[
            80, 52, 38, 44, 39, 49, 50, 56, 62, 42, 54, 59, 35, 35, 32, 31, 37, 43, 48, 47, 38, 71,
            56, 53,
        ],
    },
    Book {
        code: "JHN",
        name: "John",
        verses: &[
            51, 25, 36, 54, 47, 71, 53, 59, 41, 42, 57, 50, 38, 31, 27, 33, 26, 40, 42, 31, 25,
        ],
    },
    Book {
        code: "ACT",
        name: "Acts",
        verses: &[
            26, 47, 26, 37, 42, 15, 60, 40, 43, 48, 30, 25, 52, 28, 41, 40, 34, 28, 41, 38, 40, 30,
            35, 27, 27, 32, 44, 31,
        ],
    },
    Book {
        code: "ROM",
        name: "Romans",
        verses: &[
            32, 29, 31, 25, 21, 23, 25, 39, 33, 21, 36, 21, 14, 23, 33, 27,
        ],
    },
    Book {
        code: "1CO",
        name: "1 Corinthians",
        verses: &[
            31, 16, 23, 21, 13, 20, 40, 13, 27, 33, 34, 31, 13, 40, 58, 24,
        ],
    },
    Book {
        code: "2CO",
        name: "2 Corinthians",
        verses: &[24, 17, 18, 18, 21, 18, 16, 24, 15, 18, 33, 21, 14],
    },
    Book {
        code: "GAL",
        name: "Galatians",
        verses: &[24, 21, 29, 31, 26, 18],
    },
    Book {
        code: "EPH",
        name: "Ephesians",
        verses: &[23, 22, 21, 32, 33, 24],
    },
    Book {
        code: "PHP",
        name: "Philippians",
        verses: &[30, 30, 21, 23],
    },
    Book {
        code: "COL",
        name: "Colossians",
        verses: &[29, 23, 25, 18],
    },
    Book {
        code: "1TH",
        name: "1 Thessalonians",
        verses: &[10, 20, 13, 18, 28],
    },
    Book {
        code: "2TH",
        name: "2 Thessalonians",
        verses: &[12, 17, 18],
    },
    Book {
        code: "1TI",
        name: "1 Timothy",
        verses: &[20, 15, 16, 16, 25, 21],
    },
    Book {
        code: "2TI",
        name: "2 Timothy",
        verses: &[18, 26, 17, 22],
    },
    Book {
        code: "TIT",
        name: "Titus",
        verses: &[16, 15, 15],
    },
    Book {
        code: "PHM",
        name: "Philemon",
        verses: &[25],
    },
    Book {
        code: "HEB",
        name: "Hebrews",
        verses: &[14, 18, 19, 16, 14, 20, 28, 13, 28, 39, 40, 29, 25],
    },
    Book {
        code: "JAS",
        name: "James",
        verses: &[27, 26, 18, 17, 20],
    },
    Book {
        code: "1PE",
        name: "1 Peter",
        verses: &[25, 25, 22, 19, 14],
    },
    Book {
        code: "2PE",
        name: "2 Peter",
        verses: &[21, 22, 18],
    },
    Book {
        code: "1JN",
        name: "1 John",
        verses: &[10, 29, 24, 21, 21],
    },
    Book {
        code: "2JN",
        name: "2 John",
        verses: &[13],
    },
    Book {
        code: "3JN",
        name: "3 John",
        verses: &[14],
    },
    Book {
        code: "JUD",
        name: "Jude",
        verses: &[25],
    },
    Book {
        code: "REV",
        name: "Revelation",
        verses: &[
            20, 29, 22, 11, 14, 17, 17, 13, 21, 11, 19, 17, 18, 20, 8, 21, 18, 24, 21, 15, 27, 21,
        ],
    },
];

/// Old or New Testament. SWORD keeps a separate set of index files for each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Testament {
    Old,
    New,
}

/// A verse-level position in the KJV scheme. `chapter == 0` is a book introduction,
/// `verse == 0` (with `chapter > 0`) is a chapter introduction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Slot {
    pub book: &'static str,
    pub chapter: u32,
    pub verse: u32,
}

/// The index of every testament starts with two non-text records: the module heading
/// (slot 0) and the testament heading (slot 1).
pub const HEADING_SLOTS: usize = 2;

impl Testament {
    pub const BOTH: [Testament; 2] = [Testament::Old, Testament::New];

    /// The books of this testament in canonical order.
    pub fn books(self) -> &'static [Book] {
        match self {
            Testament::Old => &BOOKS[..OT_BOOKS],
            Testament::New => &BOOKS[OT_BOOKS..],
        }
    }

    /// Number of chapters in the testament.
    pub fn chapters(self) -> usize {
        self.books().iter().map(|b| b.verses.len()).sum()
    }

    /// Number of verses in the testament.
    pub fn verses(self) -> usize {
        self.books()
            .iter()
            .flat_map(|b| b.verses.iter())
            .map(|&v| v as usize)
            .sum()
    }

    /// Records in this testament's verse index: the two headings, one introduction per
    /// book, one introduction per chapter, and every verse.
    ///
    /// OT: 2 + 39 + 929 + 23,145 = 24,115. NT: 2 + 27 + 260 + 7,957 = 8,246.
    pub fn slot_count(self) -> usize {
        HEADING_SLOTS + self.books().len() + self.chapters() + self.verses()
    }

    /// File-name prefix SWORD uses for this testament's files (`ot`, `nt`).
    pub fn file_stem(self) -> &'static str {
        match self {
            Testament::Old => "ot",
            Testament::New => "nt",
        }
    }

    /// The text slots in index order, i.e. record `i + HEADING_SLOTS` of the verse index
    /// is `slots()[i]`. For each book: the book introduction `(book, 0, 0)`, then for each
    /// chapter its introduction `(book, c, 0)` followed by its verses.
    pub fn slots(self) -> Vec<Slot> {
        let mut out = Vec::with_capacity(self.slot_count() - HEADING_SLOTS);
        for book in self.books() {
            out.push(Slot {
                book: book.code,
                chapter: 0,
                verse: 0,
            });
            for (c, &n) in book.verses.iter().enumerate() {
                let chapter = c as u32 + 1;
                out.push(Slot {
                    book: book.code,
                    chapter,
                    verse: 0,
                });
                for verse in 1..=u32::from(n) {
                    out.push(Slot {
                        book: book.code,
                        chapter,
                        verse,
                    });
                }
            }
        }
        out
    }
}

/// Looks a book up by its USFM code.
pub fn book(code: &str) -> Option<&'static Book> {
    BOOKS.iter().find(|b| b.code == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totals_match_the_kjv() {
        assert_eq!(BOOKS.len(), 66);
        assert_eq!(Testament::Old.books().len(), 39);
        assert_eq!(Testament::New.books().len(), 27);
        assert_eq!(Testament::Old.verses(), 23_145);
        assert_eq!(Testament::New.verses(), 7_957);
        assert_eq!(Testament::Old.chapters(), 929);
        assert_eq!(Testament::New.chapters(), 260);
        assert_eq!(Testament::Old.slot_count(), 24_115);
        assert_eq!(Testament::New.slot_count(), 8_246);
        for t in Testament::BOTH {
            assert_eq!(t.slots().len() + HEADING_SLOTS, t.slot_count());
        }
    }

    #[test]
    fn spot_checks() {
        let psalms = book("PSA").unwrap();
        assert_eq!(psalms.verses.len(), 150);
        assert_eq!(psalms.verses[2], 8); // Psalm 3: the title is not a verse slot
        assert_eq!(psalms.verses[118], 176);
        assert_eq!(book("JHN").unwrap().verses[2], 36);
        assert_eq!(book("3JN").unwrap().verses, &[14]);
        assert_eq!(book("REV").unwrap().verses[21], 21);
        assert_eq!(book("MAL").unwrap().verses[3], 6);
        assert_eq!(book("EST").unwrap().verses[9], 3);
    }

    #[test]
    fn codes_are_unique_and_canonical() {
        let mut codes: Vec<_> = BOOKS.iter().map(|b| b.code).collect();
        assert_eq!(
            (codes[0], codes[38], codes[39], codes[65]),
            ("GEN", "MAL", "MAT", "REV")
        );
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), 66);
    }
}
