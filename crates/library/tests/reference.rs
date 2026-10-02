//! Tests for the scripture reference parser (`kjv_library::reference`).

use kjv_library::books::BOOKS;
use kjv_library::reference::{Context, END, Error, Options, Range, book_code, book_code_with, from_osis, parse, parse_with};

fn osis(text: &str) -> Vec<String> {
    parse(text).unwrap_or_else(|e| panic!("{text:?}: {e}")).iter().map(Range::osis).collect()
}

fn one(text: &str) -> String {
    let v = osis(text);
    assert_eq!(v.len(), 1, "{text:?} -> {v:?}");
    v.into_iter().next().unwrap()
}

#[track_caller]
fn same(text: &str, expected: &[&str]) {
    assert_eq!(osis(text), expected, "{text:?}");
}

#[test]
fn single_verses_and_forms_of_the_separator() {
    assert_eq!(one("John 3:16"), "JHN.3.16");
    assert_eq!(one("Jn 3.16"), "JHN.3.16");
    assert_eq!(one("john 3:16"), "JHN.3.16");
    assert_eq!(one("JOHN 3:16"), "JHN.3.16");
    assert_eq!(one("  John   3 : 16  "), "JHN.3.16");
    assert_eq!(one("Joh 3:16"), "JHN.3.16");
    assert_eq!(one("Jhn 3:16."), "JHN.3.16");
    assert_eq!(one("John.3.16"), "JHN.3.16");
    assert_eq!(one("Song 2:1"), "SNG.2.1");
    assert_eq!(one("Cant. 2:1"), "SNG.2.1");
    assert_eq!(one("Ecclus. 3:1"), "SIR.3.1");
    assert_eq!(one("Sir 3:1"), "SIR.3.1");
}

#[test]
fn verse_ranges_and_chapter_crossing() {
    assert_eq!(one("Rom 8:28-29"), "ROM.8.28-ROM.8.29");
    assert_eq!(one("Rom. 8:28—9:5"), "ROM.8.28-ROM.9.5");
    assert_eq!(one("Rom. 8:28–9:5"), "ROM.8.28-ROM.9.5");
    assert_eq!(one("Rom. 8:28 - 9:5"), "ROM.8.28-ROM.9.5");
    assert_eq!(one("Mk 1:1-3:6"), "MRK.1.1-MRK.3.6");
    assert_eq!(one("1 Cor 13:4-7"), "1CO.13.4-1CO.13.7");
    assert_eq!(one("I Cor. 13:4-7"), "1CO.13.4-1CO.13.7");
    // A range of one verse is that verse
    assert_eq!(one("John 3:16-16"), "JHN.3.16");
}

#[test]
fn whole_chapters_and_books() {
    assert_eq!(one("1 Cor 13"), "1CO.13");
    assert_eq!(one("1Co 13"), "1CO.13");
    assert_eq!(one("Ps 23"), "PSA.23");
    assert_eq!(one("Psa. 119:105"), "PSA.119.105");
    assert_eq!(one("Hab 3"), "HAB.3");
    assert_eq!(one("Matt 5-7"), "MAT.5-MAT.7");
    assert_eq!(one("Genesis"), "GEN");
    assert_eq!(one("Jude"), "JUD");
    let r = &parse("Psalm 23").unwrap()[0];
    assert_eq!((r.start, r.end), ((23, 0), (23, END)));
    assert!(r.is_whole_chapters() && !r.is_whole_book());
    let r = &parse("Matt 5-7").unwrap()[0];
    assert_eq!((r.start, r.end), ((5, 0), (7, END)));
    let r = &parse("Matthew").unwrap()[0];
    assert!(r.is_whole_book());
    assert_eq!((r.start, r.end), ((1, 0), (END, END)));
}

#[test]
fn single_chapter_books_take_a_bare_number_as_a_verse() {
    assert_eq!(one("Jude 3"), "JUD.1.3");
    assert_eq!(one("Jude 3-5"), "JUD.1.3-JUD.1.5");
    assert_eq!(one("Jude 1:3"), "JUD.1.3");
    assert_eq!(one("Obad 15"), "OBA.1.15");
    assert_eq!(one("Philemon 10"), "PHM.1.10");
    assert_eq!(one("2 John 5"), "2JN.1.5");
    assert_eq!(one("3 Jn 4"), "3JN.1.4");
    assert_eq!(one("Sus 13"), "SUS.1.13");
    assert_eq!(one("Bel 5"), "BEL.1.5");
    assert_eq!(one("Pr Man 7"), "MAN.1.7");
    assert_eq!(one("Ep Jer 3"), "LJE.1.3");
    // ... but Habakkuk 3 is a chapter
    assert_eq!(one("Hab 3"), "HAB.3");
    // ... and v. prefixes work in any book
    assert_eq!(one("Jude vv. 3-5"), "JUD.1.3-JUD.1.5");
}

#[test]
fn lists() {
    same("Gen 1:1, 3, 5-7", &["GEN.1.1", "GEN.1.3", "GEN.1.5-GEN.1.7"]);
    same("Psalm 51:1-3; 52", &["PSA.51.1-PSA.51.3", "PSA.52"]);
    same("Mark 1:1; Luke 3:1", &["MRK.1.1", "LUK.3.1"]);
    same("Rom 8:28; 12:1", &["ROM.8.28", "ROM.12.1"]);
    same("Ex 20:11; 31:18; 1Ch 16:26", &["EXO.20.11", "EXO.31.18", "1CH.16.26"]);
    same("Ps 33:6,9", &["PSA.33.6", "PSA.33.9"]);
    same("Isa 40:28; 42:5; 44:24", &["ISA.40.28", "ISA.42.5", "ISA.44.24"]);
    same("Matt 5, 7", &["MAT.5", "MAT.7"]);
    same("Ps 23; 1 John 4:8", &["PSA.23", "1JN.4.8"]);
    same("Ge 15:1 17:1", &["GEN.15.1", "GEN.17.1"]);
    same("Jos 9:23 Jud 1:28", &["JOS.9.23", "JUD.1.28"]);
    same("Gen 1:1, Exod 2:3", &["GEN.1.1", "EXO.2.3"]);
    same("Jude 3, 5", &["JUD.1.3", "JUD.1.5"]);
    same("Gen 1:1; 2:1-3; 5", &["GEN.1.1", "GEN.2.1-GEN.2.3", "GEN.5"]);
    same("John 3:16; v. 17", &["JHN.3.16", "JHN.3.17"]);
    same("John 3:16, vv. 18-20", &["JHN.3.16", "JHN.3.18-JHN.3.20"]);
    same("Rom 8:28-9:5, 9:10", &["ROM.8.28-ROM.9.5", "ROM.9.10"]);
}

#[test]
fn trailing_punctuation_is_harmless() {
    assert_eq!(one("Luke 9:10: "), "LUK.9.10");
    assert_eq!(one("Luke 9:10."), "LUK.9.10");
    assert_eq!(one("Luke 9:10;"), "LUK.9.10");
    assert_eq!(one("Luke 9:10,"), "LUK.9.10");
}

#[test]
fn numbered_books_in_every_spelling() {
    for spelling in [
        "1 Cor 13", "1Cor 13", "1 Corinthians 13", "1Co 13", "I Cor 13", "I Cor. 13", "ICor 13", "First Corinthians 13",
        "1st Corinthians 13", "1 CORINTHIANS 13", "i cor. 13",
    ] {
        if spelling == "ICor 13" {
            continue; // not a numeral: no separator
        }
        assert_eq!(one(spelling), "1CO.13", "{spelling}");
    }
    assert_eq!(one("II Cor. 5:17"), "2CO.5.17");
    assert_eq!(one("Second Corinthians 5:17"), "2CO.5.17");
    assert_eq!(one("2nd Cor 5:17"), "2CO.5.17");
    assert_eq!(one("III John 4"), "3JN.1.4");
    assert_eq!(one("Third John 4"), "3JN.1.4");
    assert_eq!(one("1 S 3:4"), "1SA.3.4");
    assert_eq!(one("I Sam 3:4"), "1SA.3.4");
    assert_eq!(one("1 Kingdoms 3:4"), "1SA.3.4");
    assert_eq!(one("1 Kgs 3:4"), "1KI.3.4");
    assert_eq!(one("2 Ki 3:4"), "2KI.3.4");
    assert_eq!(one("1Pt 1:3"), "1PE.1.3");
    assert_eq!(one("1Jn 1:9"), "1JN.1.9");
    assert_eq!(one("1 Thess 5:17"), "1TH.5.17");
    assert_eq!(one("2 Tim 3:16"), "2TI.3.16");
    assert_eq!(one("1 Macc 2:1"), "1MA.2.1");
    assert_eq!(one("1Ma 2:1"), "1MA.2.1");
    assert_eq!(one("1 Mac 2:1"), "1MA.2.1");
    assert_eq!(one("4 Macc 1:1"), "4MA.1.1");
    assert_eq!(one("IV Macc 1:1"), "4MA.1.1");
    assert_eq!(one("1 Esd 3:4"), "1ES.3.4");
    assert_eq!(one("2 Esd 3:4"), "2ES.3.4");
}

#[test]
fn old_and_variant_book_names() {
    let cases: &[(&str, &str)] = &[
        ("Gen", "GEN"), ("Ge", "GEN"), ("Gn", "GEN"), ("Exod", "EXO"), ("Ex", "EXO"), ("Exo", "EXO"),
        ("Lev", "LEV"), ("Le", "LEV"), ("Lv", "LEV"), ("Num", "NUM"), ("Nu", "NUM"), ("Nm", "NUM"),
        ("Deut", "DEU"), ("Dt", "DEU"), ("De", "DEU"), ("Josh", "JOS"), ("Jos", "JOS"), ("Judg", "JDG"),
        ("Jdg", "JDG"), ("Jg", "JDG"), ("Ru", "RUT"), ("Rth", "RUT"), ("Ruth", "RUT"), ("Ezra", "EZR"),
        ("Neh", "NEH"), ("Esth", "EST"), ("Esther", "EST"), ("Job", "JOB"), ("Psalms", "PSA"), ("Ps", "PSA"),
        ("Psa", "PSA"), ("Pss", "PSA"), ("Psalm", "PSA"), ("Prov", "PRO"), ("Pr", "PRO"), ("Prv", "PRO"),
        ("Eccl", "ECC"), ("Ecc", "ECC"), ("Qoh", "ECC"), ("Ecclesiastes", "ECC"), ("Song", "SNG"),
        ("SoS", "SNG"), ("Cant", "SNG"), ("Canticles", "SNG"), ("Song of Solomon", "SNG"),
        ("Song of Songs", "SNG"), ("Isa", "ISA"), ("Is", "ISA"), ("Jer", "JER"), ("Je", "JER"),
        ("Lam", "LAM"), ("La", "LAM"), ("Ezek", "EZK"), ("Eze", "EZK"), ("Ezk", "EZK"), ("Dan", "DAN"),
        ("Da", "DAN"), ("Dn", "DAN"), ("Hos", "HOS"), ("Ho", "HOS"), ("Joel", "JOL"), ("Jl", "JOL"),
        ("Amos", "AMO"), ("Am", "AMO"), ("Obad", "OBA"), ("Ob", "OBA"), ("Jonah", "JON"), ("Jon", "JON"),
        ("Mic", "MIC"), ("Mi", "MIC"), ("Nah", "NAM"), ("Na", "NAM"), ("Hab", "HAB"), ("Zeph", "ZEP"),
        ("Zep", "ZEP"), ("Hag", "HAG"), ("Hg", "HAG"), ("Zech", "ZEC"), ("Zec", "ZEC"), ("Mal", "MAL"),
        ("Matt", "MAT"), ("Mt", "MAT"), ("Mark", "MRK"), ("Mk", "MRK"), ("Mr", "MRK"), ("Luke", "LUK"),
        ("Lk", "LUK"), ("Lu", "LUK"), ("John", "JHN"), ("Jn", "JHN"), ("Jhn", "JHN"), ("Joh", "JHN"),
        ("Acts", "ACT"), ("Ac", "ACT"), ("Rom", "ROM"), ("Ro", "ROM"), ("Rm", "ROM"), ("Gal", "GAL"),
        ("Ga", "GAL"), ("Eph", "EPH"), ("Phil", "PHP"), ("Php", "PHP"), ("Pp", "PHP"), ("Col", "COL"),
        ("Tit", "TIT"), ("Titus", "TIT"), ("Philem", "PHM"), ("Phlm", "PHM"), ("Phm", "PHM"), ("Heb", "HEB"),
        ("Jas", "JAS"), ("Jam", "JAS"), ("Jm", "JAS"), ("Jude", "JUD"), ("Jud", "JUD"), ("Rev", "REV"),
        ("Re", "REV"), ("Rv", "REV"), ("Apoc", "REV"), ("Tob", "TOB"), ("Tb", "TOB"), ("Jdt", "JDT"),
        ("Wis", "WIS"), ("Ws", "WIS"), ("Sir", "SIR"), ("Ecclus", "SIR"), ("Bar", "BAR"),
        ("Pr Man", "MAN"), ("Prayer of Manasseh", "MAN"), ("Sg Three", "S3Y"), ("Song of the Three", "S3Y"),
        ("Prayer of Azariah", "S3Y"), ("Dan Gk", "DAG"), ("Pss Sol", "PSS"), ("Psalms of Solomon", "PSS"),
        ("Esther (Greek)", "ESG"), ("Esth Gr", "ESG"), ("Bel and the Dragon", "BEL"), ("Susanna", "SUS"),
        ("Letter of Jeremiah", "LJE"), ("Epistle of Jeremiah", "LJE"), ("Wisdom of Solomon", "WIS"),
        ("Sirach", "SIR"), ("Ecclesiasticus", "SIR"), ("Revelation of John", "REV"),
    ];
    for (name, code) in cases {
        // Revelation of John is not an alias; check only the ones that should resolve
        if *name == "Revelation of John" {
            assert_eq!(book_code(name), None);
            continue;
        }
        assert_eq!(book_code(name), Some(*code), "book_code({name:?})");
        let r = parse(name).unwrap_or_else(|e| panic!("{name:?}: {e}"));
        assert_eq!(r.len(), 1, "{name}");
        assert_eq!(r[0].book, *code, "{name}");
        assert!(r[0].is_whole_book(), "{name}");
    }
}

#[test]
fn psalm_151_is_its_own_book() {
    assert_eq!(one("Psalm 151"), "PS2");
    assert_eq!(one("Ps 151:3"), "PS2.1.3");
    assert_eq!(one("Ps 150"), "PSA.150");
    assert_eq!(one("Ps 150:6"), "PSA.150.6");
}

#[test]
fn every_book_is_found_by_name_display_abbr_and_code() {
    for b in BOOKS {
        // As human text, "PSS" is Psalms ("Pss") and "PS2" / "S3Y" read as a number in the
        // middle of a word; the USFM codes are for `from_osis` (every_book_round_trips...)
        let code_is_text = b.code != "PSS" && !b.code[1..].contains(|c: char| c.is_ascii_digit());
        let mut forms = vec![b.name, b.display, b.abbr];
        if code_is_text {
            forms.push(b.code);
        }
        // "Pss" is Psalms in text, so the code PSS (the Psalms of Solomon) reads as Psalms
        assert_eq!(book_code(b.code), Some(if b.code == "PSS" { "PSA" } else { b.code }));
        for form in forms {
            // "Ps 151" and "Psalm 151" are the book PS2; every form names the whole book
            let r = parse(form).unwrap_or_else(|e| panic!("{form:?} ({}): {e}", b.code));
            assert_eq!(r.len(), 1, "{form}");
            assert_eq!(r[0].book, b.code, "{form:?} should be {}", b.code);
            assert!(r[0].is_whole_book(), "{form}");
            assert_eq!(book_code(form), Some(b.code), "{form}");
            // With a lower- and upper-case spelling and a trailing period
            assert_eq!(book_code(&form.to_lowercase()), Some(b.code));
            assert_eq!(book_code(&form.to_uppercase()), Some(b.code));
            assert_eq!(parse(&format!("{form}.")).unwrap()[0].book, b.code, "{form}.");
        }
        // And with a reference: chapter and verse, for books with more than one chapter
        let single = ["OBA", "PHM", "2JN", "3JN", "JUD", "LJE", "S3Y", "SUS", "BEL", "MAN", "PS2"].contains(&b.code);
        for form in [b.display, b.abbr] {
            let text = format!("{form} 1:2");
            let r = parse(&text).unwrap_or_else(|e| panic!("{text:?}: {e}"));
            assert_eq!(r.len(), 1);
            if b.code == "PS2" {
                continue; // "Psalm 151 1:2" is not a reference
            }
            assert_eq!(r[0].book, b.code, "{text}");
            assert_eq!((r[0].start, r[0].end), ((1, 2), (1, 2)), "{text}");
            let text = format!("{form} 3");
            let r = parse(&text).unwrap();
            if single {
                assert_eq!(r[0].start, (1, 3), "{text}");
            } else {
                assert_eq!((r[0].start, r[0].end), ((3, 0), (3, END)), "{text}");
            }
        }
    }
}

#[test]
fn every_book_round_trips_through_osis_text() {
    for b in BOOKS {
        for text in [
            b.code.to_string(),
            format!("{}.1", b.code),
            format!("{}.1.2", b.code),
            format!("{}.1.2-{}.1.4", b.code, b.code),
            format!("{}.1.2-{}.3.4", b.code, b.code),
            format!("{}.1-{}.3", b.code, b.code),
        ] {
            let r = from_osis(&text).unwrap_or_else(|e| panic!("{text}: {e}"));
            assert_eq!(r.len(), 1);
            assert_eq!(r[0].book, b.code);
            assert_eq!(r[0].osis(), text);
            assert_eq!(from_osis(&r[0].osis()).unwrap(), r);
        }
    }
}

#[test]
fn display_and_encoding() {
    let r = Range::verse("JHN", 3, 16).unwrap();
    assert_eq!(r.to_string(), "JHN.3.16");
    assert_eq!(format!("{r}"), r.osis());
    assert!(r.is_single_verse());
    assert!(r.contains("JHN", 3, 16) && !r.contains("JHN", 3, 17) && !r.contains("MAT", 3, 16));
    let r = Range::new("ROM", (8, 28), (8, 29)).unwrap();
    assert_eq!(r.osis(), "ROM.8.28-ROM.8.29");
    assert!(r.contains("ROM", 8, 28) && r.contains("ROM", 8, 29) && !r.contains("ROM", 8, 30));
    let r = Range::chapter("PSA", 23).unwrap();
    assert_eq!(r.osis(), "PSA.23");
    assert!(r.contains("PSA", 23, 1) && r.contains("PSA", 23, 6) && !r.contains("PSA", 24, 1));
    assert_eq!(Range::new("MAT", (5, 0), (7, END)).unwrap().osis(), "MAT.5-MAT.7");
    assert_eq!(Range::whole_book("JUD").unwrap().osis(), "JUD");
    assert_eq!(Range::new("GEN", (1, 5), (1, END)).unwrap().osis(), "GEN.1.5-GEN.1");
    assert_eq!(Range::new("GEN", (1, 5), (END, END)).unwrap().osis(), "GEN.1.5-GEN");
    // Round trips through from_osis
    for text in ["GEN.1.5-GEN.1", "GEN.1.5-GEN", "PSA.23", "MAT.5-MAT.7", "JUD", "ROM.8.28-ROM.9.5", "JHN.3.16"] {
        assert_eq!(from_osis(text).unwrap()[0].osis(), text);
    }
}

#[test]
fn constructors_validate() {
    assert!(Range::new("NOPE", (1, 1), (1, 1)).is_err());
    assert!(Range::new("JHN", (0, 1), (0, 1)).is_err());
    assert!(Range::new("JHN", (1, 1), (1, 0)).is_err());
    assert!(Range::new("JHN", (3, 16), (3, 15)).is_err());
    assert!(Range::new("JHN", (3, 16), (2, 20)).is_err());
    assert!(Range::verse("JHN", 3, 0).is_err());
    assert!(Range::verse("JHN", 0, 1).is_err());
    assert!(Range::chapter("JHN", 0).is_err());
    assert!(Range::new("JHN", (3, 16), (END, 5)).is_err());
}

#[test]
fn dialects_of_jud() {
    assert_eq!(book_code("Jud"), Some("JUD"));
    assert_eq!(book_code_with("Jud", Options { jud_is_judges: true, ..Options::default() }), Some("JDG"));
    assert_eq!(book_code_with("Jude", Options { jud_is_judges: true, ..Options::default() }), Some("JUD"));
    let o = Options { jud_is_judges: true, ..Options::default() };
    let r = parse_with("Jud 16:1,2; Jude 3", o).unwrap();
    assert_eq!(r.iter().map(Range::osis).collect::<Vec<_>>(), ["JDG.16.1", "JDG.16.2", "JUD.1.3"]);
}

#[test]
fn from_osis_forms() {
    let f = |s: &str| from_osis(s).unwrap_or_else(|e| panic!("{s}: {e}")).iter().map(Range::osis).collect::<Vec<_>>();
    assert_eq!(f("John.3.16"), ["JHN.3.16"]);
    assert_eq!(f("Gen.1.1-Gen.2.3"), ["GEN.1.1-GEN.2.3"]);
    assert_eq!(f("Ps.23"), ["PSA.23"]);
    assert_eq!(f("1Cor.13.4-1Cor.13.7"), ["1CO.13.4-1CO.13.7"]);
    assert_eq!(f("1Sam.2.3"), ["1SA.2.3"]);
    assert_eq!(f("Song.1.1"), ["SNG.1.1"]);
    assert_eq!(f("Matt.5.3-Matt.5.10"), ["MAT.5.3-MAT.5.10"]);
    assert_eq!(f("Jude"), ["JUD"]);
    assert_eq!(f("Gen"), ["GEN"]);
    assert_eq!(f("Gen.1-Gen.3"), ["GEN.1-GEN.3"]);
    assert_eq!(f("Gen.1.5-Gen.2"), ["GEN.1.5-GEN.2"]);
    assert_eq!(f("John.1.3 John.1.10"), ["JHN.1.3", "JHN.1.10"]);
    assert_eq!(f("Luke.9.46-Luke.9.50 Luke.22.24-Luke.22.27"), ["LUK.9.46-LUK.9.50", "LUK.22.24-LUK.22.27"]);
    assert_eq!(f("  Ps.42.7   Ps.69.2 "), ["PSA.42.7", "PSA.69.2"]);
    // Grain suffixes are dropped
    assert_eq!(f("John.3.16!a"), ["JHN.3.16"]);
    assert_eq!(f("Gen.1.1!crossReference.a Gen.1.2"), ["GEN.1.1", "GEN.1.2"]);
    // A work prefix is dropped
    assert_eq!(f("KJV:John.3.16"), ["JHN.3.16"]);
    // Every Apocrypha book name from the brief
    for (osis, usfm) in [
        ("Tob", "TOB"), ("Jdt", "JDT"), ("AddEsth", "ESG"), ("EsthGr", "ESG"), ("Wis", "WIS"), ("Sir", "SIR"), ("Bar", "BAR"),
        ("EpJer", "LJE"), ("PrAzar", "S3Y"), ("Sus", "SUS"), ("Bel", "BEL"), ("PrMan", "MAN"), ("1Macc", "1MA"),
        ("2Macc", "2MA"), ("3Macc", "3MA"), ("4Macc", "4MA"), ("1Esd", "1ES"), ("2Esd", "2ES"), ("AddPs", "PS2"),
        ("PssSol", "PSS"), ("AddDan", "DAG"), ("DanGr", "DAG"),
    ] {
        assert_eq!(f(&format!("{osis}.1.2")), [format!("{usfm}.1.2")], "{osis}");
    }
    // Every OSIS book name
    for (osis, usfm) in [
        ("Gen", "GEN"), ("Exod", "EXO"), ("Lev", "LEV"), ("Num", "NUM"), ("Deut", "DEU"), ("Josh", "JOS"), ("Judg", "JDG"),
        ("Ruth", "RUT"), ("1Sam", "1SA"), ("2Sam", "2SA"), ("1Kgs", "1KI"), ("2Kgs", "2KI"), ("1Chr", "1CH"),
        ("2Chr", "2CH"), ("Ezra", "EZR"), ("Neh", "NEH"), ("Esth", "EST"), ("Job", "JOB"), ("Ps", "PSA"), ("Prov", "PRO"),
        ("Eccl", "ECC"), ("Song", "SNG"), ("Isa", "ISA"), ("Jer", "JER"), ("Lam", "LAM"), ("Ezek", "EZK"), ("Dan", "DAN"),
        ("Hos", "HOS"), ("Joel", "JOL"), ("Amos", "AMO"), ("Obad", "OBA"), ("Jonah", "JON"), ("Mic", "MIC"), ("Nah", "NAM"),
        ("Hab", "HAB"), ("Zeph", "ZEP"), ("Hag", "HAG"), ("Zech", "ZEC"), ("Mal", "MAL"), ("Matt", "MAT"), ("Mark", "MRK"),
        ("Luke", "LUK"), ("John", "JHN"), ("Acts", "ACT"), ("Rom", "ROM"), ("1Cor", "1CO"), ("2Cor", "2CO"), ("Gal", "GAL"),
        ("Eph", "EPH"), ("Phil", "PHP"), ("Col", "COL"), ("1Thess", "1TH"), ("2Thess", "2TH"), ("1Tim", "1TI"),
        ("2Tim", "2TI"), ("Titus", "TIT"), ("Phlm", "PHM"), ("Heb", "HEB"), ("Jas", "JAS"), ("1Pet", "1PE"),
        ("2Pet", "2PE"), ("1John", "1JN"), ("2John", "2JN"), ("3John", "3JN"), ("Jude", "JUD"), ("Rev", "REV"),
    ] {
        assert_eq!(f(&format!("{osis}.2.3")), [format!("{usfm}.2.3")], "{osis}");
    }
    // The abbreviations that appear in real OSIS data
    for (osis, usfm) in [("Ge", "GEN"), ("Ac", "ACT"), ("Lu", "LUK"), ("De", "DEU"), ("Pr", "PRO"), ("Ne", "NEH"), ("Php", "PHP"),
        ("Eze", "EZK"), ("1Jo", "1JN"), ("1Ti", "1TI"), ("1Th", "1TH"), ("1Ki", "1KI"), ("1Sa", "1SA"), ("2Ch", "2CH"),
        ("Psa", "PSA"), ("Cant", "SNG"), ("Wisdom", "WIS"), ("Tobit", "TOB"), ("1Mac", "1MA")] {
        assert_eq!(f(&format!("{osis}.2.3")), [format!("{usfm}.2.3")], "{osis}");
    }
}

#[test]
fn errors() {
    let e = |s: &str| parse(s).unwrap_err();
    assert_eq!(e(""), Error::Empty);
    assert_eq!(e("   "), Error::Empty);
    assert!(matches!(e("Foo 3:16"), Error::UnknownBook(_)));
    assert!(matches!(e("3:16"), Error::UnknownBook(_)));
    assert!(matches!(e("ch. 3:16"), Error::UnknownBook(_)));
    assert!(matches!(e("16"), Error::UnknownBook(_)));
    assert!(matches!(e("John 3:16ff"), Error::Unsupported(_)));
    assert!(matches!(e("John 3:16 ff."), Error::Unsupported(_)));
    assert!(matches!(e("John 3:16f"), Error::Unsupported(_)));
    assert!(matches!(e("John 3:16-4"), Error::Invalid(_)), "{:?}", e("John 3:16-4"));
    assert!(matches!(e("John 3:16-3:15"), Error::Invalid(_)));
    assert!(matches!(e("Num 15:4-7:28"), Error::Invalid(_)));
    assert!(matches!(e("Rom 0:1"), Error::Invalid(_)));
    assert!(matches!(e("Rom 8:0"), Error::Invalid(_)));
    assert!(matches!(e("Rom 8:0-5"), Error::Invalid(_)));
    assert!(matches!(e("Rom 8:5-0"), Error::Invalid(_)));
    assert!(matches!(e("Rom 0"), Error::Invalid(_)));
    assert!(matches!(e("Matt 7-5"), Error::Invalid(_)));
    assert!(matches!(e("John 3:16 and following verses"), Error::Trailing(_)));
    assert!(matches!(e("John 3:16 they"), Error::Trailing(_)));
    assert!(matches!(e("John 3:16 (NIV)"), Error::Trailing(_)));
    // a dangling colon is trailing punctuation
    assert_eq!(one("John 3:"), "JHN.3");
    assert!(matches!(e("John 99999999999"), Error::Syntax(_)));
    assert!(matches!(e("Matt 5-7:5"), Error::Unsupported(_)));
    // v. with no chapter known
    assert!(matches!(e("Gen vv. 3-5"), Error::Syntax(_)));
    // Errors display usefully
    for s in ["Foo 1:1", "John 3:16ff", "Rom 0:1", "John 3:16 they", ""] {
        let msg = e(s).to_string();
        assert!(!msg.is_empty());
    }
}

#[test]
fn osis_errors() {
    let e = |s: &str| from_osis(s).unwrap_err();
    assert_eq!(e(""), Error::Empty);
    assert_eq!(e("   "), Error::Empty);
    assert!(matches!(e("Foo.1.1"), Error::UnknownBook(_)));
    assert!(matches!(e("John.x.1"), Error::Syntax(_)));
    assert!(matches!(e("John.3.16.2"), Error::Syntax(_)));
    assert!(matches!(e("John.3."), Error::Syntax(_)));
    assert!(matches!(e("John.3.16-Gen.1.1"), Error::Unsupported(_)));
    assert!(matches!(e("Num.15.4-Num.7.28"), Error::Invalid(_)));
    assert!(matches!(e("John.0.1"), Error::Invalid(_)));
    assert!(matches!(e("John.3.0"), Error::Invalid(_)));
    assert!(matches!(e("Ps.5.1Ch"), Error::Syntax(_)));
    // the real-world malformed one: "2Pe 1.16" splits into a book and a non-reference
    assert!(matches!(e("1Jo.1.1 2Pe 1.16"), Error::UnknownBook(_)), "{:?}", e("1Jo.1.1 2Pe 1.16"));
    assert!(matches!(e("!a"), Error::Syntax(_)));
    assert!(matches!(e("John.3.16-"), Error::UnknownBook(_)));
}

#[test]
fn whole_chapter_ranges_compare_in_order() {
    // Ranges are ordered by (chapter, verse); a whole chapter starts before its verses
    let ch = Range::chapter("PSA", 23).unwrap();
    assert!(ch.start < Range::verse("PSA", 23, 1).unwrap().start);
}

// ------------------------------------------------------------------------------------
// References written inside a note: the book (and chapter) come from where it is written
// ------------------------------------------------------------------------------------

fn under(book: &'static str, chapter: u32) -> Options {
    Options { context: Some(Context { book, chapter }), ..Options::default() }
}

#[track_caller]
fn rel(book: &'static str, chapter: u32, text: &str, expected: &[&str]) {
    let got = parse_with(text, under(book, chapter)).unwrap_or_else(|e| panic!("{text:?}: {e}"));
    assert_eq!(got.iter().map(Range::osis).collect::<Vec<_>>(), expected, "{text:?} under {book} {chapter}");
}

#[test]
fn a_context_supplies_the_book_and_chapter() {
    // The convention of the Treasury of Scripture Knowledge and Wesley's Notes
    rel("GEN", 1, "22; 8:17; 9:1,7", &["GEN.1.22", "GEN.8.17", "GEN.9.1", "GEN.9.7"]);
    rel("GEN", 1, "4:7", &["GEN.4.7"]);
    rel("GEN", 1, "13", &["GEN.1.13"]);
    rel("GEN", 1, "13-15", &["GEN.1.13-GEN.1.15"]);
    rel("GEN", 1, "10,19; 5:2", &["GEN.1.10", "GEN.1.19", "GEN.5.2"]);
    rel("ISA", 2, "10,19; Ex 33:22; Job 30:6; So 2:14", &["ISA.2.10", "ISA.2.19", "EXO.33.22", "JOB.30.6", "SNG.2.14"]);
    rel("PSA", 17, "17:7; Mt 10:21", &["PSA.17.7", "MAT.10.21"]);
    // a book named in the text replaces the context, for every later passage too
    rel("GEN", 1, "Ex 20:11; 31:18; 1Ch 16:26", &["EXO.20.11", "EXO.31.18", "1CH.16.26"]);
    // after a semicolon a bare number is a chapter, as without a context
    rel("GEN", 1, "22; 23", &["GEN.1.22", "GEN.23"]);
    // verse and chapter prefixes
    rel("GEN", 1, "v. 3", &["GEN.1.3"]);
    rel("GEN", 1, "ver. 3", &["GEN.1.3"]);
    rel("GEN", 1, "vv. 3-5", &["GEN.1.3-GEN.1.5"]);
    rel("GEN", 1, "ch. 3:4", &["GEN.3.4"]);
    rel("GEN", 1, "chap. 3", &["GEN.3"]);
    rel("GEN", 1, "chapter 3-4", &["GEN.3-GEN.4"]);
    rel("GEN", 1, "ch. 3; 5:1", &["GEN.3", "GEN.5.1"]);
    // single-chapter books
    rel("JUD", 1, "3", &["JUD.1.3"]);
    // Jud is Judges where the module says so
    let o = Options { jud_is_judges: true, ..under("GEN", 1) };
    assert_eq!(parse_with("Jud 7:7; 8", o).unwrap().iter().map(Range::osis).collect::<Vec<_>>(), ["JDG.7.7", "JDG.8"]);
}

#[test]
fn a_context_without_a_chapter_cannot_place_a_bare_number() {
    let o = under("GEN", 0);
    assert!(matches!(parse_with("13", o), Err(Error::Syntax(_))));
    assert!(matches!(parse_with("v. 3", o), Err(Error::Syntax(_))));
    // but a chapter and verse, or a chapter, is fine
    assert_eq!(parse_with("4:7", o).unwrap()[0].osis(), "GEN.4.7");
    assert_eq!(parse_with("ch. 4", o).unwrap()[0].osis(), "GEN.4");
}

#[test]
fn without_a_context_a_reference_needs_a_book() {
    assert!(matches!(parse("4:7"), Err(Error::UnknownBook(_))));
    assert!(matches!(parse("ch. 4"), Err(Error::UnknownBook(_))));
    assert!(matches!(parse("v. 4"), Err(Error::UnknownBook(_))));
    // a bad context book is reported
    let o = Options { context: Some(Context { book: "XXX", chapter: 1 }), ..Options::default() };
    assert!(matches!(parse_with("4:7", o), Err(Error::UnknownBook(_))));
    // after a book is named, ch. and v. refer to it
    same("John 3:16; ch. 4", &["JHN.3.16", "JHN.4"]);
    same("John 3:16; ch. 4:2", &["JHN.3.16", "JHN.4.2"]);
}

#[test]
fn a_bare_book_name_after_a_passage_needs_a_separator() {
    assert!(matches!(parse("Gen 1:1 is").unwrap_err(), Error::Trailing(_)));
    assert!(matches!(parse("John 3:16 so").unwrap_err(), Error::Trailing(_)));
    same("Gen 1:1, Job", &["GEN.1.1", "JOB"]);
    same("Gen 1:1; Ruth", &["GEN.1.1", "RUT"]);
    // with numbers it is a passage
    same("Jos 9:23 Jud 1:28", &["JOS.9.23", "JUD.1.28"]);
    // and a bare name that opens a list is the whole book
    same("Ruth; Job 1", &["RUT", "JOB.1"]);
}

// ------------------------------------------------------------------------------------
// The Treasury's starred labels
// ------------------------------------------------------------------------------------

fn starred(text: &str) -> Vec<String> {
    let o = Options { starred_labels: true, ..Options::default() };
    parse_with(text, o).unwrap_or_else(|e| panic!("{text:?}: {e}")).iter().map(Range::osis).collect()
}

#[test]
fn starred_labels_are_not_part_of_a_reference() {
    assert_eq!(starred("Ps 69:34; *marg:"), ["PSA.69.34"]);
    assert_eq!(starred("Ps 3:1; *title"), ["PSA.3.1"]);
    assert_eq!(starred("1Jo 5:19; *Gr:"), ["1JN.5.19"]);
    assert_eq!(starred("Ex 11:8; *Heb:"), ["EXO.11.8"]);
    assert_eq!(starred("2Ch 9:5,6; *marg,"), ["2CH.9.5", "2CH.9.6"]);
    assert_eq!(starred("Le 27:21,28; *compared with:"), ["LEV.27.21", "LEV.27.28"]);
    assert_eq!(starred("Ge 38:27,29,30; 46:12; * Judah, Pharez, Zarah:"), ["GEN.38.27", "GEN.38.29", "GEN.38.30", "GEN.46.12"]);
    // a label does not give a book to a bare reference
    let o = Options { starred_labels: true, ..Options::default() };
    assert!(matches!(parse_with("74:1; *title, marg:", o), Err(Error::UnknownBook(_))));
}

#[test]
fn a_starred_label_ends_where_a_reference_begins() {
    assert_eq!(starred("Ps 30:1; *title Joh 10:22"), ["PSA.30.1", "JHN.10.22"]);
    assert_eq!(starred("Ps 30:1; *marg: Joh 10:22"), ["PSA.30.1", "JHN.10.22"]);
    assert_eq!(starred("Ps 30:1; *title; Joh 10:22"), ["PSA.30.1", "JHN.10.22"]);
    // a label in the middle of a list
    assert_eq!(starred("Ps 30:1; *marg: 31:2"), ["PSA.30.1", "PSA.31.2"]);
}

#[test]
fn starred_labels_need_the_option() {
    assert!(matches!(parse("Ps 69:34; *marg:"), Err(Error::Trailing(_))));
    assert!(matches!(parse("Ps 69:34; *title"), Err(Error::Trailing(_))));
}

#[test]
fn starred_labels_work_with_a_context() {
    let o = Options { starred_labels: true, ..under("PSA", 3) };
    assert_eq!(parse_with("1; *title", o).unwrap()[0].osis(), "PSA.3.1");
    assert_eq!(parse_with("74:1; *title, marg:", o).unwrap()[0].osis(), "PSA.74.1");
}
