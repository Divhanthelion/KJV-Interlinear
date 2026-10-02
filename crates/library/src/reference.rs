//! Scripture references: parsing what people (and commentaries) write, and OSIS.
//!
//! ```
//! use kjv_library::reference::{parse, Range};
//! let r = parse("Rom. 8:28—9:5; 1 Cor 13").unwrap();
//! assert_eq!(r[0].osis(), "ROM.8.28-ROM.9.5");
//! assert_eq!(r[1].osis(), "1CO.13");
//! ```
//!
//! Book identifiers are the USFM codes of [`crate::books`]. Nothing here is checked
//! against a verse count: versification differs between translations, so a reference
//! to "Psalm 151" or "Jude 40" is well formed. What is checked is that chapters and
//! verses are positive and that a range runs forward.
//!
//! # Encoding of a [`Range`]
//!
//! A range is a book plus a start and an end, each `(chapter, verse)`:
//!
//! | meaning | start | end |
//! |---|---|---|
//! | one verse, John 3:16 | `(3, 16)` | `(3, 16)` |
//! | a verse range, Rom 8:28-29 | `(8, 28)` | `(8, 29)` |
//! | across chapters, Rom 8:28–9:5 | `(8, 28)` | `(9, 5)` |
//! | a whole chapter, Psalm 23 | `(23, 0)` | `(23, END)` |
//! | whole chapters, Matt 5–7 | `(5, 0)` | `(7, END)` |
//! | a whole book, Jude | `(1, 0)` | `(END, END)` |
//!
//! Verse 0 as a *start* means "from the beginning of the chapter" and [`END`]
//! (`u32::MAX`) as an *end* means "to the end of the chapter" (or of the book, when the
//! chapter is `END` too). Verse 0 and chapter 0 are never produced from text: "Rom 0:1"
//! is an error.
//!
//! # What is understood
//!
//! * Book names, abbreviations and old forms, case-insensitive, with or without periods
//!   and spaces: `John`, `Jn`, `Joh`, `1 Cor`, `I Cor.`, `1Co`, `First Corinthians`,
//!   `Song of Solomon`, `Cant.`, `Ecclus.`, `Pr Man`, `4 Macc`, and the OSIS spellings.
//! * `3:16`, `3.16`, `3:16-18`, `3:16–4:2` (hyphen, en dash, em dash), `Matt 5-7`,
//!   `Ps 23`, `Jude 3` (a single-chapter book's bare number is a verse), a bare book
//!   name (the whole book), `v. 3`, `vv. 3-5`.
//! * Lists: `Gen 1:1, 3, 5-7` (commas continue the verses), `Ps 51:1-3; 52` (a semicolon
//!   starts a new passage, so a bare number is a chapter), `Mark 1:1; Luke 3:1`, and
//!   whitespace-separated passages (`Ge 15:1 17:1`). A passage with no book takes the
//!   previous passage's book.
//!
//! "3:16ff" and "3:16f" are reported as unsupported rather than guessed at. References
//! with no book ("4:7", "ch. 3:16", "ver. 5", a bare "13") are an error unless the caller
//! says where they are written ([`Context`]); after a book has been named, `ch. 4` and
//! `v. 5` refer to that book. A bare book name is accepted after the first passage only
//! when a comma or semicolon sets it off, so "Gen 1:1 is" is not Genesis and Isaiah.

use std::collections::HashMap;
use std::fmt;
use std::sync::OnceLock;

use crate::books::{BOOKS, by_code};

/// "To the end of the chapter" (as a verse) or "to the end of the book" (as a chapter).
pub const END: u32 = u32::MAX;

/// A passage: one verse, a verse range, whole chapters, or a whole book. See the
/// module documentation for the encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Range {
    /// USFM code, as in [`crate::books`].
    pub book: &'static str,
    /// `(chapter, verse)`; verse 0 means the start of the chapter.
    pub start: (u32, u32),
    /// `(chapter, verse)`; verse [`END`] means the end of the chapter.
    pub end: (u32, u32),
}

impl Range {
    /// A validated range: chapters and verses at least 1 (a start verse may be 0 and an
    /// end verse or chapter may be [`END`]), and the end not before the start.
    pub fn new(book: &str, start: (u32, u32), end: (u32, u32)) -> Result<Range, Error> {
        let book = by_code(book).ok_or_else(|| Error::UnknownBook(book.to_string()))?.code;
        if start.0 == 0 || end.0 == 0 || end.1 == 0 || start.0 == END {
            return Err(Error::Invalid(format!("chapter and verse numbers start at 1 ({book} {start:?}-{end:?})")));
        }
        if (end.0 == END) != (end.1 == END && end.0 == END) {
            return Err(Error::Invalid("a range that ends at the end of the book".into()));
        }
        if start > end {
            return Err(Error::Invalid(format!(
                "the range runs backwards: {} to {}",
                position(start),
                position(end)
            )));
        }
        Ok(Range { book, start, end })
    }

    /// One verse.
    pub fn verse(book: &str, chapter: u32, verse: u32) -> Result<Range, Error> {
        if verse == 0 {
            return Err(Error::Invalid("verse numbers start at 1".into()));
        }
        Range::new(book, (chapter, verse), (chapter, verse))
    }

    /// A whole chapter.
    pub fn chapter(book: &str, chapter: u32) -> Result<Range, Error> {
        Range::new(book, (chapter, 0), (chapter, END))
    }

    /// A whole book.
    pub fn whole_book(book: &str) -> Result<Range, Error> {
        Range::new(book, (1, 0), (END, END))
    }

    pub fn is_whole_book(&self) -> bool {
        self.start == (1, 0) && self.end == (END, END)
    }

    /// True for one or more complete chapters (and not a whole book).
    pub fn is_whole_chapters(&self) -> bool {
        self.start.1 == 0 && self.end.1 == END && self.end.0 != END
    }

    /// True for a single verse.
    pub fn is_single_verse(&self) -> bool {
        self.start == self.end && self.start.1 != 0 && self.start.1 != END
    }

    /// Does this range contain the verse? (Chapter 0 and verse 0 are not verses.)
    pub fn contains(&self, book: &str, chapter: u32, verse: u32) -> bool {
        self.book == book && self.start <= (chapter, verse) && (chapter, verse) <= self.end
    }

    /// OSIS-style text: `JHN.3.16`, `ROM.8.28-ROM.8.29`, `PSA.23`, `MAT.5-MAT.7`, `JUD`.
    /// (Book codes are the USFM ones used throughout the library.)
    pub fn osis(&self) -> String {
        self.to_string()
    }
}

fn position(p: (u32, u32)) -> String {
    match p {
        (END, _) => "the end of the book".into(),
        (c, END) => format!("the end of chapter {c}"),
        (c, 0) => format!("the start of chapter {c}"),
        (c, v) => format!("{c}:{v}"),
    }
}

impl fmt::Display for Range {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let b = self.book;
        if self.is_whole_book() {
            return f.write_str(b);
        }
        let (c, v) = self.start;
        let (c2, v2) = self.end;
        // A start with verse 0 and an end with verse END is a whole chapter (or chapters).
        if v == 0 && v2 == END {
            return if c == c2 { write!(f, "{b}.{c}") } else { write!(f, "{b}.{c}-{b}.{c2}") };
        }
        if v == 0 { write!(f, "{b}.{c}")? } else { write!(f, "{b}.{c}.{v}")? }
        if (c, v) == (c2, v2) {
            return Ok(());
        }
        match (c2, v2) {
            (END, _) => write!(f, "-{b}"),
            (_, END) => write!(f, "-{b}.{c2}"),
            _ => write!(f, "-{b}.{c2}.{v2}"),
        }
    }
}

/// Why a reference could not be understood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Nothing to parse.
    Empty,
    /// The text where a book name was expected is not one.
    UnknownBook(String),
    /// Understood but deliberately not handled ("3:16ff", cross-book ranges).
    Unsupported(String),
    /// Numbers out of range or a range running backwards.
    Invalid(String),
    /// Something that is not part of a reference follows one.
    Trailing(String),
    /// Malformed text.
    Syntax(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Empty => f.write_str("empty reference"),
            Error::UnknownBook(s) => write!(f, "unknown book {s:?}"),
            Error::Unsupported(s) => write!(f, "unsupported reference form: {s}"),
            Error::Invalid(s) => write!(f, "invalid reference: {s}"),
            Error::Trailing(s) => write!(f, "unexpected text after the reference: {s:?}"),
            Error::Syntax(s) => write!(f, "malformed reference: {s}"),
        }
    }
}

impl std::error::Error for Error {}

// ------------------------------------------------------------------------------------
// Book names
// ------------------------------------------------------------------------------------

/// Spellings of each book, as lowercase letters and digits with no spaces or periods
/// (`1cor`, `songofsolomon`). The names, display names, abbreviations and USFM codes in
/// [`crate::books`] and all the OSIS spellings are added to these automatically.
const ALIASES: &[(&str, &[&str])] = &[
    ("GEN", &["gen", "ge", "gn", "genesis"]),
    ("EXO", &["exod", "ex", "exo", "exodus"]),
    ("LEV", &["lev", "le", "lv", "levit", "leviticus"]),
    ("NUM", &["num", "nu", "nm", "numb", "numbers"]),
    ("DEU", &["deut", "dt", "de", "deu", "deuteronomy"]),
    ("JOS", &["josh", "jos", "joshua"]),
    ("JDG", &["judg", "jdg", "jg", "jdgs", "judges"]),
    ("RUT", &["ruth", "ru", "rth", "rut"]),
    ("1SA", &["1sam", "1sa", "1s", "1sm", "1samuel", "1kingdoms", "1kgdms"]),
    ("2SA", &["2sam", "2sa", "2s", "2sm", "2samuel", "2kingdoms", "2kgdms"]),
    ("1KI", &["1kgs", "1ki", "1k", "1kin", "1kings", "3kingdoms", "3kgdms"]),
    ("2KI", &["2kgs", "2ki", "2k", "2kin", "2kings", "4kingdoms", "4kgdms"]),
    ("1CH", &["1chr", "1ch", "1chron", "1chronicles", "1paralipomenon"]),
    ("2CH", &["2chr", "2ch", "2chron", "2chronicles", "2paralipomenon"]),
    ("EZR", &["ezra", "ezr"]),
    ("NEH", &["neh", "ne", "nehemiah", "nehem"]),
    ("EST", &["esth", "es", "est", "esther"]),
    ("JOB", &["job", "jb"]),
    ("PSA", &["ps", "psa", "psalm", "psalms", "pss", "psal", "psl", "psm"]),
    ("PRO", &["prov", "pr", "prv", "pro", "proverbs"]),
    ("ECC", &["eccl", "ecc", "ec", "eccles", "ecclesiastes", "qoh", "qoheleth"]),
    (
        "SNG",
        &[
            "song", "sos", "ss", "sng", "sg", "songofsolomon", "songofsongs", "canticles", "cant",
            "canticleofcanticles", "canticle", "songofsol", "so",
        ],
    ),
    ("ISA", &["isa", "is", "isaiah", "isai"]),
    ("JER", &["jer", "je", "jr", "jeremiah"]),
    ("LAM", &["lam", "la", "lamentations"]),
    ("EZK", &["ezek", "eze", "ezk", "ezekiel"]),
    ("DAN", &["dan", "da", "dn", "daniel"]),
    ("HOS", &["hos", "ho", "hosea"]),
    ("JOL", &["joel", "jl", "joe"]),
    ("AMO", &["amos", "am", "amo"]),
    ("OBA", &["obad", "ob", "oba", "obadiah"]),
    ("JON", &["jonah", "jon", "jnh"]),
    ("MIC", &["mic", "mi", "micah"]),
    ("NAM", &["nah", "na", "nahum"]),
    ("HAB", &["hab", "habakkuk"]),
    ("ZEP", &["zeph", "zep", "zp", "zephaniah"]),
    ("HAG", &["hag", "hg", "haggai"]),
    ("ZEC", &["zech", "zec", "zc", "zechariah"]),
    ("MAL", &["mal", "ml", "malachi"]),
    ("MAT", &["matt", "mt", "mat", "matthew"]),
    ("MRK", &["mark", "mk", "mr", "mrk"]),
    ("LUK", &["luke", "lk", "lu", "luk", "luc"]),
    ("JHN", &["john", "jn", "jhn", "joh"]),
    ("ACT", &["acts", "ac", "act"]),
    ("ROM", &["rom", "ro", "rm", "romans"]),
    ("1CO", &["1cor", "1co", "1corinthians"]),
    ("2CO", &["2cor", "2co", "2corinthians"]),
    ("GAL", &["gal", "ga", "galatians"]),
    ("EPH", &["eph", "ephes", "ephesians"]),
    ("PHP", &["phil", "php", "pp", "philippians", "phi"]),
    ("COL", &["col", "colossians"]),
    ("1TH", &["1thess", "1th", "1thes", "1thessalonians"]),
    ("2TH", &["2thess", "2th", "2thes", "2thessalonians"]),
    ("1TI", &["1tim", "1ti", "1timothy"]),
    ("2TI", &["2tim", "2ti", "2timothy"]),
    ("TIT", &["tit", "titus"]),
    ("PHM", &["philem", "phlm", "phm", "philemon"]),
    ("HEB", &["heb", "hebrews"]),
    ("JAS", &["jas", "jam", "jm", "james"]),
    ("1PE", &["1pet", "1pe", "1pt", "1peter"]),
    ("2PE", &["2pet", "2pe", "2pt", "2peter"]),
    ("1JN", &["1john", "1jn", "1jo", "1joh"]),
    ("2JN", &["2john", "2jn", "2jo", "2joh"]),
    ("3JN", &["3john", "3jn", "3jo", "3joh"]),
    ("JUD", &["jude", "jud"]),
    ("REV", &["rev", "re", "rv", "revelation", "revelations", "apoc", "apocalypse"]),
    // The Apocrypha and the further books of the Septuagint
    ("1ES", &["1esd", "1es", "1esdras", "3ezra"]),
    ("2ES", &["2esd", "2es", "2esdras", "4ezra"]),
    ("TOB", &["tob", "tb", "tobit", "tobias"]),
    ("JDT", &["jdt", "jdth", "judith"]),
    ("ESG", &["esg", "addesth", "esthgr", "estg", "additionstoesther", "estheradditions"]),
    ("WIS", &["wis", "ws", "wisd", "wisdom", "wisdomofsolomon", "wisofsol"]),
    ("SIR", &["sir", "ecclus", "ecclesiasticus", "sirach", "wisdomofsirach", "bensira", "wisdomofjesusbensirach"]),
    ("BAR", &["bar", "baruch"]),
    ("LJE", &["lje", "epjer", "letterofjeremiah", "epistleofjeremiah", "epjeremiah"]),
    (
        "S3Y",
        &[
            "s3y", "prazar", "sgthree", "songofthethree", "songofthreechildren", "songofthethreeyoungmen",
            "songofthreeyoungmen", "prayerofazariah", "azariah", "thesongofthethreeholychildren",
            "songofthethreeholychildren",
        ],
    ),
    ("SUS", &["sus", "susanna"]),
    ("BEL", &["bel", "beldr", "belandthedragon", "belanddragon"]),
    (
        "MAN",
        &["man", "prman", "prayerofmanasseh", "prayerofmanasses", "prmanasseh", "manasseh"],
    ),
    ("1MA", &["1macc", "1ma", "1mac", "1maccabees", "1mc"]),
    ("2MA", &["2macc", "2ma", "2mac", "2maccabees", "2mc"]),
    ("3MA", &["3macc", "3ma", "3mac", "3maccabees", "3mc"]),
    ("4MA", &["4macc", "4ma", "4mac", "4maccabees", "4mc"]),
    ("PS2", &["ps151", "addps", "psalm151"]),
    ("PSS", &["psssol", "pssol", "pssolomon", "psalmsofsolomon", "psalmsol"]),
    ("DAG", &["dangk", "dangr", "adddan", "danielgreek", "danielgr", "additionstodaniel"]),
];

/// Single-chapter books: a bare number after the name is a verse ("Jude 3").
const SINGLE_CHAPTER: &[&str] = &["OBA", "PHM", "2JN", "3JN", "JUD", "LJE", "S3Y", "SUS", "BEL", "MAN", "PS2"];

/// A name reduced to lowercase letters and digits ("1 Cor." and "1cor" are the same).
fn key(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

fn table() -> &'static HashMap<String, &'static str> {
    static TABLE: OnceLock<HashMap<String, &'static str>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut map: HashMap<String, &'static str> = HashMap::new();
        let mut add = |k: String, code: &'static str| match map.insert(k.clone(), code) {
            Some(old) if old != code => panic!("book alias {k:?} means both {old} and {code}"),
            _ => {}
        };
        for b in BOOKS {
            for name in [b.name, b.display, b.abbr] {
                add(key(name), b.code);
            }
        }
        for (code, aliases) in ALIASES {
            let code = by_code(code).expect("alias table names a known book").code;
            for a in *aliases {
                add((*a).to_string(), code);
            }
        }
        // The USFM codes themselves, unless a code is also an ordinary abbreviation
        // ("Pss" is Psalms; the Psalms of Solomon are PSS only in OSIS/USFM text, which
        // `from_osis` reads by exact code first).
        for b in BOOKS {
            map.entry(key(b.code)).or_insert(b.code);
        }
        map
    })
}

/// Where a reference is written: the verse a note is attached to. With a context, a
/// reference that names no book is taken to be in that book (`4:7`), and a bare number
/// at the start of a list, or after a comma, is a verse of that chapter (`13`, `29; 2:9,16`).
/// This is the convention of the Treasury of Scripture Knowledge and Wesley's Notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Context {
    pub book: &'static str,
    /// 0 when there is no chapter (a book introduction): then a bare number is an error.
    pub chapter: u32,
}

/// How book names that are ambiguous in the wild are read, and what a reference without a
/// book means.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    /// `Jud` means Judges, not Jude. (Wesley's Notes and the Treasury of Scripture
    /// Knowledge write Judges as "Jud" and Jude as "Jude"; elsewhere "Jud" is Jude.)
    pub jud_is_judges: bool,
    /// Resolve references that name no book against this verse.
    pub context: Option<Context>,
    /// Starred labels are not part of a reference: the Treasury of Scripture Knowledge
    /// writes `Ps 69:34; *marg:`, `Ps 3:1; *title`, `Joh 1:14; *Gr:` and `Le 27:21; *compared
    /// with:` to say what kind of cross-reference follows or what the text says. A label
    /// runs from its `*` to the next `:` (or to a book name followed by a number, or the end).
    pub starred_labels: bool,
}

/// The USFM code for a book name or abbreviation ("Jn", "1 Cor.", "Song of Solomon",
/// "Pr Man", "I Kings", "JHN"), or `None`.
pub fn book_code(name: &str) -> Option<&'static str> {
    book_code_with(name, Options::default())
}

/// [`book_code`] with options.
pub fn book_code_with(name: &str, options: Options) -> Option<&'static str> {
    let k = numeral_prefix(name);
    lookup(&k, options)
}

fn lookup(k: &str, options: Options) -> Option<&'static str> {
    if options.jud_is_judges && k == "jud" {
        return Some("JDG");
    }
    table().get(k).copied()
}

/// Folds a leading roman numeral or ordinal word into a digit and reduces the name to
/// a key: "I Cor." -> "1cor", "Second Peter" -> "2peter", "1st John" -> "1john".
fn numeral_prefix(name: &str) -> String {
    let t = name.trim_start();
    let first: String = t.chars().take_while(|c| c.is_alphabetic()).collect();
    let rest = &t[first.len()..];
    // A numeral word must be followed by a separator, not run into the name ("Ijohn" is
    // not "1john").
    let sep = rest.starts_with(|c: char| c.is_whitespace() || c == '.');
    let n = match first.to_lowercase().as_str() {
        "i" | "first" if sep => Some('1'),
        "ii" | "second" if sep => Some('2'),
        "iii" | "third" if sep => Some('3'),
        "iv" | "fourth" if sep => Some('4'),
        _ => None,
    };
    match n {
        Some(d) => format!("{d}{}", key(rest)),
        None => {
            // "1st John", "2nd Peter", "3rd John"
            let digits: String = t.chars().take_while(char::is_ascii_digit).collect();
            let after = t[digits.len()..].to_lowercase();
            for suffix in ["st", "nd", "rd", "th"] {
                if !digits.is_empty() && after.starts_with(suffix) && after[suffix.len()..].starts_with(|c: char| c.is_whitespace() || c == '.') {
                    return format!("{digits}{}", key(&after[suffix.len()..]));
                }
            }
            key(t)
        }
    }
}

fn single_chapter(code: &str) -> bool {
    SINGLE_CHAPTER.contains(&code)
}

// ------------------------------------------------------------------------------------
// Human references
// ------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Num(u32),
    Word(String),
    Colon,
    Dash,
    Comma,
    Semi,
    Dot,
    Other(char),
}

#[derive(Debug, Clone)]
struct Token {
    tok: Tok,
    /// Byte offset in the input.
    at: usize,
    /// Whitespace immediately before.
    space: bool,
}

fn lex(text: &str) -> Result<Vec<Token>, Error> {
    let mut out = Vec::new();
    let mut chars = text.char_indices().peekable();
    let mut space = false;
    while let Some(&(at, c)) = chars.peek() {
        if c.is_whitespace() || c == '\u{a0}' {
            chars.next();
            space = true;
            continue;
        }
        let tok = if c.is_ascii_digit() {
            let mut n: u64 = 0;
            while let Some(&(_, d)) = chars.peek() {
                let Some(v) = d.to_digit(10).filter(|_| d.is_ascii_digit()) else { break };
                n = n * 10 + u64::from(v);
                if n > u64::from(u32::MAX - 1) {
                    return Err(Error::Syntax(format!("number too large at {at}")));
                }
                chars.next();
            }
            Tok::Num(n as u32)
        } else if c.is_alphabetic() {
            let mut w = String::new();
            while let Some(&(_, d)) = chars.peek() {
                if d.is_alphabetic() || d == '\'' || d == '’' {
                    w.push(d);
                    chars.next();
                } else {
                    break;
                }
            }
            Tok::Word(w)
        } else {
            chars.next();
            match c {
                ':' => Tok::Colon,
                '-' | '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}' | '\u{2212}' => Tok::Dash,
                ',' => Tok::Comma,
                ';' => Tok::Semi,
                '.' => Tok::Dot,
                other => Tok::Other(other),
            }
        };
        out.push(Token { tok, at, space });
        space = false;
    }
    Ok(out)
}

struct Parser<'a> {
    text: &'a str,
    toks: Vec<Token>,
    pos: usize,
    options: Options,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    None,
    Chapter,
    Verse,
}

struct Ctx {
    book: Option<&'static str>,
    chapter: u32,
    level: Level,
    /// The book is the caller's [`Context`], not one the text named.
    from_context: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sep {
    Start,
    Comma,
    Semi,
    Space,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.tok)
    }

    fn peek_at(&self, ahead: usize) -> Option<&Token> {
        self.toks.get(self.pos + ahead)
    }

    fn rest(&self) -> &'a str {
        match self.toks.get(self.pos) {
            Some(t) => &self.text[t.at..],
            None => "",
        }
    }

    /// A book name at the current position: (code, tokens used, a number was folded in).
    fn book_here(&self) -> Option<(&'static str, usize)> {
        let mut i = self.pos;
        let mut prefix = String::new();
        // A leading numeral: 1, 2, 3, 4, an ordinal ("1st"), a roman numeral or word.
        match self.toks.get(i).map(|t| &t.tok) {
            Some(Tok::Num(n @ 1..=4)) => {
                prefix.push_str(&n.to_string());
                i += 1;
                if let Some(Token { tok: Tok::Word(w), space: false, .. }) = self.toks.get(i) {
                    // "1st John" (but "1Th" is Thessalonians: an ordinal needs a name after it)
                    if matches!(w.to_lowercase().as_str(), "st" | "nd" | "rd" | "th")
                        && matches!(self.toks.get(i + 1).map(|t| &t.tok), Some(Tok::Word(_)))
                    {
                        i += 1;
                    }
                }
            }
            Some(Tok::Word(w)) => {
                let n = match w.to_lowercase().as_str() {
                    "i" | "first" => Some("1"),
                    "ii" | "second" => Some("2"),
                    "iii" | "third" => Some("3"),
                    "iv" | "fourth" => Some("4"),
                    _ => None,
                };
                if let Some(n) = n {
                    // Only a numeral if a name follows.
                    if matches!(self.toks.get(i + 1).map(|t| &t.tok), Some(Tok::Word(_))) {
                        prefix.push_str(n);
                        i += 1;
                    }
                }
            }
            _ => {}
        }
        // Up to six words ("Song of the Three Young Men"), longest first.
        let mut words: Vec<(String, usize)> = Vec::new(); // (word, index after it incl. dots)
        let mut j = i;
        while words.len() < 7 {
            match self.toks.get(j).map(|t| &t.tok) {
                // "Esther (Greek)": parentheses are part of a book's name, not of a reference
                Some(Tok::Other('(')) if !words.is_empty() => j += 1,
                Some(Tok::Word(w)) => {
                    j += 1;
                    // an abbreviation's period ("Rom.", "Pr. Man.")
                    if matches!(self.toks.get(j).map(|t| &t.tok), Some(Tok::Dot)) {
                        j += 1;
                    }
                    words.push((w.to_lowercase(), j));
                }
                _ => break,
            }
        }
        for n in (1..=words.len()).rev() {
            let joined: String = words[..n].iter().map(|(w, _)| w.as_str()).collect();
            let k = format!("{prefix}{joined}");
            if let Some(code) = lookup(&k, self.options) {
                let mut end = words[n - 1].1;
                if matches!(self.toks.get(end).map(|t| &t.tok), Some(Tok::Other(')'))) {
                    end += 1;
                }
                return Some((code, end - self.pos));
            }
        }
        None
    }

    fn expect_num(&mut self, what: &str) -> Result<u32, Error> {
        match self.peek() {
            Some(Tok::Num(n)) => {
                let n = *n;
                self.pos += 1;
                Ok(n)
            }
            _ => Err(Error::Syntax(format!("expected {what} at {:?}", self.rest()))),
        }
    }

    /// A ':' (or a '.' directly between numbers) joining chapter and verse.
    fn eat_verse_sep(&mut self) -> bool {
        match (self.peek(), self.peek_at(1)) {
            (Some(Tok::Colon), Some(Token { tok: Tok::Num(_), .. })) => {
                self.pos += 1;
                true
            }
            (Some(Tok::Dot), Some(Token { tok: Tok::Num(_), space: false, .. })) if !self.toks[self.pos].space => {
                self.pos += 1;
                true
            }
            _ => false,
        }
    }

    fn eat_verse_prefix(&mut self) -> bool {
        if let Some(Tok::Word(w)) = self.peek()
            && matches!(w.to_lowercase().as_str(), "v" | "vv" | "vs" | "vss" | "ver" | "vers" | "verse" | "verses")
        {
            self.pos += 1;
            if matches!(self.peek(), Some(Tok::Dot)) {
                self.pos += 1;
            }
            return true;
        }
        false
    }

    /// "ch.", "chap.", "chapter" (and the plural) before a number.
    fn eat_chapter_prefix(&mut self) -> bool {
        if self.chapter_prefix_ahead() {
            self.pos += 1;
            if matches!(self.peek(), Some(Tok::Dot)) {
                self.pos += 1;
            }
            return true;
        }
        false
    }

    fn chapter_prefix_ahead(&self) -> bool {
        let word = matches!(self.peek(), Some(Tok::Word(w)) if matches!(w.to_lowercase().as_str(), "ch" | "chap" | "chaps" | "chapter" | "chapters"));
        let skip_dot = usize::from(matches!(self.peek_at(1).map(|t| &t.tok), Some(Tok::Dot)));
        word && matches!(self.peek_at(1 + skip_dot).map(|t| &t.tok), Some(Tok::Num(_)))
    }

    fn check_not_ff(&self) -> Result<(), Error> {
        if let Some(Tok::Word(w)) = self.peek()
            && matches!(w.to_lowercase().as_str(), "ff" | "f" | "ffs")
        {
            return Err(Error::Unsupported(format!(
                "{:?}: \"{w}\" (and following verses) is reported, not guessed at",
                self.text
            )));
        }
        Ok(())
    }

    /// One passage after a book (or continuing the previous book).
    fn part(&mut self, ctx: &mut Ctx, sep: Sep) -> Result<Range, Error> {
        let book = ctx.book.expect("a book is set before parts are read");
        let verse_prefix = self.eat_verse_prefix();
        let chapter_prefix = !verse_prefix && self.eat_chapter_prefix();
        let n1 = self.expect_num("a chapter or verse number")?;
        let (start, level);
        if n1 == 0 {
            return Err(Error::Invalid("chapter and verse numbers start at 1".into()));
        }
        if self.eat_verse_sep() {
            let v = self.expect_num("a verse number")?;
            if v == 0 {
                return Err(Error::Invalid("verse numbers start at 1".into()));
            }
            if verse_prefix {
                return Err(Error::Syntax("a chapter:verse after \"v.\"".into()));
            }
            start = (n1, v);
            level = Level::Verse;
            ctx.chapter = n1;
        } else if verse_prefix {
            if single_chapter(book) {
                ctx.chapter = 1;
            } else if ctx.chapter == 0 {
                return Err(Error::Syntax("a verse number with no chapter".into()));
            }
            start = (ctx.chapter, n1);
            level = Level::Verse;
        } else if chapter_prefix {
            ctx.chapter = n1;
            start = (n1, 0);
            level = Level::Chapter;
        } else if single_chapter(book) {
            // "Jude 3" is verse 3 of the only chapter
            ctx.chapter = 1;
            start = (1, n1);
            level = Level::Verse;
        } else if matches!(sep, Sep::Comma | Sep::Start) && ctx.level == Level::Verse {
            start = (ctx.chapter, n1);
            level = Level::Verse;
        } else if ctx.from_context && ctx.level == Level::None {
            // a note with no chapter (a book introduction) cannot say which chapter's verse
            return Err(Error::Syntax("a number with no chapter".into()));
        } else {
            ctx.chapter = n1;
            start = (n1, 0);
            level = Level::Chapter;
        }
        let mut end = if level == Level::Verse { start } else { (n1, END) };
        if matches!(self.peek(), Some(Tok::Dash)) {
            self.pos += 1;
            let n3 = self.expect_num("the end of the range")?;
            if n3 == 0 {
                return Err(Error::Invalid("chapter and verse numbers start at 1".into()));
            }
            if self.eat_verse_sep() {
                let v = self.expect_num("a verse number")?;
                if v == 0 {
                    return Err(Error::Invalid("verse numbers start at 1".into()));
                }
                if level != Level::Verse {
                    return Err(Error::Unsupported(format!("{:?}: a range from a whole chapter to a verse", self.text)));
                }
                end = (n3, v);
                ctx.chapter = n3;
            } else if level == Level::Verse {
                end = (start.0, n3);
            } else {
                end = (n3, END);
                ctx.chapter = n3;
            }
        }
        self.check_not_ff()?;
        ctx.level = level;
        Range::new(book, start, end)
    }

    /// Removes starred labels (see [`Options::starred_labels`]) from the tokens.
    fn strip_labels(&mut self) {
        let mut i = 0;
        while i < self.toks.len() {
            if self.toks[i].tok != Tok::Other('*') {
                i += 1;
                continue;
            }
            let mut j = i + 1;
            while j < self.toks.len() {
                if self.toks[j].tok == Tok::Colon {
                    j += 1;
                    break;
                }
                // A reference follows the label: a book name and then a number
                self.pos = j;
                if let Some((_, used)) = self.book_here()
                    && matches!(self.toks.get(j + used).map(|t| &t.tok), Some(Tok::Num(_)))
                {
                    break;
                }
                j += 1;
            }
            // The text after the label starts where the label's whitespace ended
            let space = self.toks[i].space;
            self.toks.drain(i..j);
            if let Some(next) = self.toks.get_mut(i) {
                next.space |= space;
            }
        }
        self.pos = 0;
    }

    fn run(&mut self) -> Result<Vec<Range>, Error> {
        if self.options.starred_labels {
            self.strip_labels();
        }
        let mut out = Vec::new();
        let mut ctx = match self.options.context {
            Some(c) => Ctx {
                book: Some(by_code(c.book).ok_or_else(|| Error::UnknownBook(c.book.to_string()))?.code),
                chapter: c.chapter,
                level: if c.chapter > 0 { Level::Verse } else { Level::None },
                from_context: true,
            },
            None => Ctx { book: None, chapter: 0, level: Level::None, from_context: false },
        };
        let mut sep = Sep::Start;
        loop {
            // Is this the start of a new book?
            if let Some((code, used)) = self.book_here() {
                let start = self.pos;
                self.pos += used;
                ctx = Ctx { book: Some(code), chapter: 0, level: Level::None, from_context: false };
                // "Psalm 151" is a book of its own in the library
                if code == "PSA" && self.peek() == Some(&Tok::Num(151)) {
                    self.pos += 1;
                    ctx.book = Some("PS2");
                    if matches!(self.peek(), Some(Tok::Colon))
                        && matches!(self.peek_at(1).map(|t| &t.tok), Some(Tok::Num(_)))
                    {
                        self.pos += 1;
                    }
                }
                let book = ctx.book.expect("set above");
                // A bare book name: the whole book, unless numbers follow. After a passage
                // it must be set off by a comma or semicolon ("Gen 1:1 is" is not Isaiah).
                if matches!(self.peek(), Some(Tok::Num(_))) || self.verse_prefix_ahead() || self.chapter_prefix_ahead() {
                    out.push(self.part(&mut ctx, Sep::Start)?);
                } else if !out.is_empty() && sep == Sep::Space {
                    self.pos = start;
                    break;
                } else {
                    out.push(Range::whole_book(book)?);
                }
            } else if ctx.book.is_some()
                && (matches!(self.peek(), Some(Tok::Num(_))) || self.verse_prefix_ahead() || self.chapter_prefix_ahead())
            {
                out.push(self.part(&mut ctx, sep)?);
            } else if out.is_empty() {
                let shown: String = self.rest().chars().take(40).collect();
                return Err(if shown.trim().is_empty() { Error::Empty } else { Error::UnknownBook(shown) });
            } else {
                break;
            }
            if !self.continue_after(&mut sep)? {
                break;
            }
        }
        // Trailing punctuation is harmless; anything else is not part of a reference.
        while matches!(self.peek(), Some(Tok::Dot | Tok::Colon | Tok::Comma | Tok::Semi)) {
            self.pos += 1;
        }
        if self.pos < self.toks.len() {
            return Err(Error::Trailing(self.rest().to_string()));
        }
        Ok(out)
    }

    fn verse_prefix_ahead(&self) -> bool {
        matches!(self.peek(), Some(Tok::Word(w)) if matches!(w.to_lowercase().as_str(), "v" | "vv" | "vs" | "vss" | "ver" | "vers" | "verse" | "verses"))
            && matches!(self.peek_at(1).map(|t| &t.tok), Some(Tok::Num(_) | Tok::Dot))
    }

    /// After a passage: consumes a separator and says whether another passage follows.
    fn continue_after(&mut self, sep: &mut Sep) -> Result<bool, Error> {
        match self.peek() {
            Some(Tok::Semi) => {
                self.pos += 1;
                *sep = Sep::Semi;
                Ok(self.pos < self.toks.len())
            }
            Some(Tok::Comma) => {
                self.pos += 1;
                *sep = Sep::Comma;
                Ok(self.pos < self.toks.len())
            }
            Some(Tok::Num(_)) | Some(Tok::Word(_)) => {
                // whitespace-separated passages ("Ge 15:1 17:1", "Jos 9:23 Jud 1:28")
                *sep = Sep::Space;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

/// Parses human references: "John 3:16", "Rom. 8:28—9:5", "Gen 1:1, 3, 5-7",
/// "Psalm 51:1-3; 52". See the module documentation for what is understood.
pub fn parse(text: &str) -> Result<Vec<Range>, Error> {
    parse_with(text, Options::default())
}

/// [`parse`] with options for ambiguous abbreviations.
pub fn parse_with(text: &str, options: Options) -> Result<Vec<Range>, Error> {
    if text.trim().is_empty() {
        return Err(Error::Empty);
    }
    let mut p = Parser { text, toks: lex(text)?, pos: 0, options };
    p.run()
}

// ------------------------------------------------------------------------------------
// OSIS references
// ------------------------------------------------------------------------------------

/// Parses OSIS references: `John.3.16`, `Gen.1.1-Gen.2.3`, `Ps.23`, `1Cor.13.4-1Cor.13.7`,
/// `Jude`, and space-separated lists of them. A grain suffix (`!a`) is dropped. Book
/// names are OSIS names, the usual abbreviations, or the library's USFM codes.
pub fn from_osis(osis_ref: &str) -> Result<Vec<Range>, Error> {
    let mut out = Vec::new();
    for item in osis_ref.split_whitespace() {
        // grain
        let item = item.split('!').next().unwrap_or("");
        // an optional work prefix ("Bible:", "KJV:") before the first '.'
        let item = match item.split_once(':') {
            Some((work, rest)) if !work.contains('.') && !work.is_empty() => rest,
            _ => item,
        };
        if item.is_empty() {
            return Err(Error::Syntax(format!("empty reference in {osis_ref:?}")));
        }
        let (a, b) = match item.split_once('-') {
            Some((a, b)) => (a, Some(b)),
            None => (item, None),
        };
        let (book, sc, sv) = osis_point(a)?;
        let (start, end) = match b {
            None => match (sc, sv) {
                (None, _) => ((1, 0), (END, END)),
                (Some(c), None) => ((c, 0), (c, END)),
                (Some(c), Some(v)) => ((c, v), (c, v)),
            },
            Some(b) => {
                let (book2, ec, ev) = osis_point(b)?;
                if book != book2 {
                    return Err(Error::Unsupported(format!("{item:?}: a range across books")));
                }
                let start = match (sc, sv) {
                    (None, _) => (1, 0),
                    (Some(c), None) => (c, 0),
                    (Some(c), Some(v)) => (c, v),
                };
                let end = match (ec, ev) {
                    (None, _) => (END, END),
                    (Some(c), None) => (c, END),
                    (Some(c), Some(v)) => (c, v),
                };
                (start, end)
            }
        };
        out.push(Range::new(book, start, end).map_err(|e| match e {
            Error::Invalid(m) => Error::Invalid(format!("{item:?}: {m}")),
            other => other,
        })?);
    }
    if out.is_empty() {
        return Err(Error::Empty);
    }
    Ok(out)
}

/// `Book[.chapter[.verse]]` -> (book, chapter, verse).
fn osis_point(s: &str) -> Result<(&'static str, Option<u32>, Option<u32>), Error> {
    let mut parts = s.split('.');
    let name = parts.next().unwrap_or("");
    // An exact USFM code wins ("PSS" is the Psalms of Solomon here, though "Pss" is Psalms)
    let book = by_code(name)
        .map(|b| b.code)
        .or_else(|| table().get(&key(name)).copied())
        .ok_or_else(|| Error::UnknownBook(name.to_string()))?;
    if name.chars().any(|c| !c.is_alphanumeric()) {
        return Err(Error::Syntax(format!("{s:?}")));
    }
    let num = |p: &str| -> Result<u32, Error> {
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
            return Err(Error::Syntax(format!("{p:?} in {s:?} is not a number")));
        }
        p.parse::<u32>().map_err(|_| Error::Syntax(format!("{p:?} in {s:?} is too large")))
    };
    let chapter = parts.next().map(num).transpose()?;
    let verse = parts.next().map(num).transpose()?;
    if parts.next().is_some() {
        return Err(Error::Syntax(format!("{s:?} has too many parts")));
    }
    Ok((book, chapter, verse))
}
