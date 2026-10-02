//! Every book any translation in the library has, in the app's order: the Old
//! Testament, the Apocrypha (in the order the 1611 KJV printed them, then the
//! further books of the Septuagint and Vulgate traditions), and the New Testament.
//!
//! `name` is the key the app already uses everywhere ("First Samuel"), so saved
//! bookmarks and settings keep working; `code` is the USFM code the data files use.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Old,
    /// The deuterocanonical books / Apocrypha
    Apocrypha,
    New,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Book {
    pub code: &'static str,
    /// The app's key: "First Samuel", "Tobit"
    pub name: &'static str,
    /// For display: "1 Samuel"
    pub display: &'static str,
    pub abbr: &'static str,
    pub section: Section,
}

const fn b(code: &'static str, name: &'static str, display: &'static str, abbr: &'static str, section: Section) -> Book {
    Book { code, name, display, abbr, section }
}

use Section::{Apocrypha as A, New as N, Old as O};

pub const BOOKS: &[Book] = &[
    b("GEN", "Genesis", "Genesis", "Gen", O),
    b("EXO", "Exodus", "Exodus", "Exod", O),
    b("LEV", "Leviticus", "Leviticus", "Lev", O),
    b("NUM", "Numbers", "Numbers", "Num", O),
    b("DEU", "Deuteronomy", "Deuteronomy", "Deut", O),
    b("JOS", "Joshua", "Joshua", "Josh", O),
    b("JDG", "Judges", "Judges", "Judg", O),
    b("RUT", "Ruth", "Ruth", "Ruth", O),
    b("1SA", "First Samuel", "1 Samuel", "1 Sam", O),
    b("2SA", "Second Samuel", "2 Samuel", "2 Sam", O),
    b("1KI", "First Kings", "1 Kings", "1 Kgs", O),
    b("2KI", "Second Kings", "2 Kings", "2 Kgs", O),
    b("1CH", "First Chronicles", "1 Chronicles", "1 Chr", O),
    b("2CH", "Second Chronicles", "2 Chronicles", "2 Chr", O),
    b("EZR", "Ezra", "Ezra", "Ezra", O),
    b("NEH", "Nehemiah", "Nehemiah", "Neh", O),
    b("EST", "Esther", "Esther", "Esth", O),
    b("JOB", "Job", "Job", "Job", O),
    b("PSA", "Psalms", "Psalms", "Ps", O),
    b("PRO", "Proverbs", "Proverbs", "Prov", O),
    b("ECC", "Ecclesiastes", "Ecclesiastes", "Eccl", O),
    b("SNG", "Song of Solomon", "Song of Solomon", "Song", O),
    b("ISA", "Isaiah", "Isaiah", "Isa", O),
    b("JER", "Jeremiah", "Jeremiah", "Jer", O),
    b("LAM", "Lamentations", "Lamentations", "Lam", O),
    b("EZK", "Ezekiel", "Ezekiel", "Ezek", O),
    b("DAN", "Daniel", "Daniel", "Dan", O),
    b("HOS", "Hosea", "Hosea", "Hos", O),
    b("JOL", "Joel", "Joel", "Joel", O),
    b("AMO", "Amos", "Amos", "Amos", O),
    b("OBA", "Obadiah", "Obadiah", "Obad", O),
    b("JON", "Jonah", "Jonah", "Jonah", O),
    b("MIC", "Micah", "Micah", "Mic", O),
    b("NAM", "Nahum", "Nahum", "Nah", O),
    b("HAB", "Habakkuk", "Habakkuk", "Hab", O),
    b("ZEP", "Zephaniah", "Zephaniah", "Zeph", O),
    b("HAG", "Haggai", "Haggai", "Hag", O),
    b("ZEC", "Zechariah", "Zechariah", "Zech", O),
    b("MAL", "Malachi", "Malachi", "Mal", O),
    // The Apocrypha as the 1611 KJV printed them
    b("1ES", "First Esdras", "1 Esdras", "1 Esd", A),
    b("2ES", "Second Esdras", "2 Esdras", "2 Esd", A),
    b("TOB", "Tobit", "Tobit", "Tob", A),
    b("JDT", "Judith", "Judith", "Jdt", A),
    b("ESG", "Esther (Greek)", "Esther (Greek)", "Esg", A),
    b("WIS", "Wisdom of Solomon", "Wisdom of Solomon", "Wis", A),
    b("SIR", "Sirach", "Sirach (Ecclesiasticus)", "Sir", A),
    b("BAR", "Baruch", "Baruch", "Bar", A),
    b("LJE", "Letter of Jeremiah", "Letter of Jeremiah", "Ep Jer", A),
    b("S3Y", "Song of the Three", "Song of the Three Young Men", "Sg Three", A),
    b("SUS", "Susanna", "Susanna", "Sus", A),
    b("BEL", "Bel and the Dragon", "Bel and the Dragon", "Bel", A),
    b("MAN", "Prayer of Manasseh", "Prayer of Manasseh", "Pr Man", A),
    b("1MA", "First Maccabees", "1 Maccabees", "1 Macc", A),
    b("2MA", "Second Maccabees", "2 Maccabees", "2 Macc", A),
    // Further books of the Septuagint
    b("3MA", "Third Maccabees", "3 Maccabees", "3 Macc", A),
    b("4MA", "Fourth Maccabees", "4 Maccabees", "4 Macc", A),
    b("PS2", "Psalm 151", "Psalm 151", "Ps 151", A),
    b("PSS", "Psalms of Solomon", "Psalms of Solomon", "Pss Sol", A),
    b("DAG", "Daniel (Greek)", "Daniel (Greek)", "Dan Gk", A),
    b("MAT", "Matthew", "Matthew", "Matt", N),
    b("MRK", "Mark", "Mark", "Mark", N),
    b("LUK", "Luke", "Luke", "Luke", N),
    b("JHN", "John", "John", "John", N),
    b("ACT", "Acts", "Acts", "Acts", N),
    b("ROM", "Romans", "Romans", "Rom", N),
    b("1CO", "First Corinthians", "1 Corinthians", "1 Cor", N),
    b("2CO", "Second Corinthians", "2 Corinthians", "2 Cor", N),
    b("GAL", "Galatians", "Galatians", "Gal", N),
    b("EPH", "Ephesians", "Ephesians", "Eph", N),
    b("PHP", "Philippians", "Philippians", "Phil", N),
    b("COL", "Colossians", "Colossians", "Col", N),
    b("1TH", "First Thessalonians", "1 Thessalonians", "1 Thess", N),
    b("2TH", "Second Thessalonians", "2 Thessalonians", "2 Thess", N),
    b("1TI", "First Timothy", "1 Timothy", "1 Tim", N),
    b("2TI", "Second Timothy", "2 Timothy", "2 Tim", N),
    b("TIT", "Titus", "Titus", "Titus", N),
    b("PHM", "Philemon", "Philemon", "Phlm", N),
    b("HEB", "Hebrews", "Hebrews", "Heb", N),
    b("JAS", "James", "James", "Jas", N),
    b("1PE", "First Peter", "1 Peter", "1 Pet", N),
    b("2PE", "Second Peter", "2 Peter", "2 Pet", N),
    b("1JN", "First John", "1 John", "1 John", N),
    b("2JN", "Second John", "2 John", "2 John", N),
    b("3JN", "Third John", "3 John", "3 John", N),
    b("JUD", "Jude", "Jude", "Jude", N),
    b("REV", "Revelation", "Revelation", "Rev", N),
];

pub fn by_code(code: &str) -> Option<&'static Book> {
    BOOKS.iter().find(|b| b.code == code)
}

pub fn by_name(name: &str) -> Option<&'static Book> {
    BOOKS.iter().find(|b| b.name == name)
}

/// Position in the app's order.
pub fn order(code: &str) -> Option<usize> {
    BOOKS.iter().position(|b| b.code == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixty_six_plus_the_apocrypha() {
        assert_eq!(BOOKS.iter().filter(|b| b.section == Section::Old).count(), 39);
        assert_eq!(BOOKS.iter().filter(|b| b.section == Section::New).count(), 27);
        let codes: std::collections::HashSet<_> = BOOKS.iter().map(|b| b.code).collect();
        let names: std::collections::HashSet<_> = BOOKS.iter().map(|b| b.name).collect();
        assert_eq!((codes.len(), names.len()), (BOOKS.len(), BOOKS.len()));
        assert_eq!(by_code("1SA").unwrap().name, "First Samuel");
        assert!(order("MAL").unwrap() < order("TOB").unwrap() && order("TOB").unwrap() < order("MAT").unwrap());
    }
}
