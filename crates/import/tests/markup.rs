//! The markup converter (`kjv_import::markup`): each element's mapping, the text proof,
//! and the error paths, on small samples written out in full.

use kjv_import::markup::{Dialect, Options, Stats, body_text, convert, prove, source_text, tokenize, Tok};

fn osis_opts() -> Options {
    Options { dialect: Dialect::Osis, jud_is_judges: false, context: None }
}

fn thml_opts() -> Options {
    Options { dialect: Dialect::Thml, jud_is_judges: false, context: None }
}

/// Converts, and checks the proof holds for what came out.
#[track_caller]
fn conv(raw: &str, opts: Options) -> (String, Stats) {
    let (body, stats) = convert(raw, opts).unwrap_or_else(|e| panic!("{raw:?}: {e}"));
    prove(raw, &body, opts.dialect).unwrap_or_else(|e| panic!("{raw:?} -> {body:?}: {e}"));
    (body, stats)
}

#[track_caller]
fn osis(raw: &str) -> String {
    conv(raw, osis_opts()).0
}

#[track_caller]
fn thml(raw: &str) -> String {
    conv(raw, thml_opts()).0
}

#[track_caller]
fn osis_err(raw: &str) -> String {
    convert(raw, osis_opts()).expect_err(raw)
}

#[track_caller]
fn thml_err(raw: &str) -> String {
    convert(raw, thml_opts()).expect_err(raw)
}

// ---------------------------------------------------------------------------------------
// OSIS: blocks
// ---------------------------------------------------------------------------------------

#[test]
fn paragraph_milestones_make_paragraphs() {
    assert_eq!(
        osis(r#"<div sID="g1" type="x-p"/>Hello <hi type="italic">world</hi>.<div eID="g1" type="x-p"/>"#),
        "<p>Hello <i>world</i>.</p>"
    );
    assert_eq!(
        osis(r#"<div sID="g1" type="x-p"/>One<div eID="g1" type="x-p"/><div sID="g2" type="x-p"/>Two<div eID="g2" type="x-p"/>"#),
        "<p>One</p><p>Two</p>"
    );
}

#[test]
fn a_paragraph_container_makes_a_paragraph() {
    assert_eq!(osis(r#"<div type="x-p">One</div><div type="x-p">Two</div>"#), "<p>One</p><p>Two</p>");
    assert_eq!(osis(r#"<milestone type="x-p"/>One<milestone type="x-p"/>Two"#), "<p>One</p><p>Two</p>");
}

#[test]
fn text_outside_any_paragraph_gets_a_paragraph() {
    assert_eq!(osis("Plain text"), "<p>Plain text</p>");
    assert_eq!(osis(r#"Before<div type="x-p">Inside</div>After"#), "<p>Before</p><p>Inside</p><p>After</p>");
}

#[test]
fn whitespace_between_blocks_is_not_text() {
    assert_eq!(osis("  \n "), "");
    assert_eq!(osis(r#" <div type="x-p">A</div> <div type="x-p">B</div> "#), "<p>A</p><p>B</p>");
}

#[test]
fn titles_become_headings() {
    assert_eq!(osis(r#"<title type="x-s3">The Case of Abraham</title><div type="x-p">Text</div>"#), "<h>The Case of Abraham</h><p>Text</p>");
    assert_eq!(osis(r#"<title>Untyped</title>"#), "<h>Untyped</h>");
    for t in ["x-s", "x-s2", "x-s3", "x-s4", "x-ms", "main", "x-IS"] {
        assert_eq!(osis(&format!(r#"<title type="{t}">H</title>"#)), "<h>H</h>", "{t}");
    }
    assert!(osis_err(r#"<title type="sub">H</title>"#).contains("unknown title type"));
}

#[test]
fn a_title_in_the_middle_of_a_paragraph_splits_it() {
    assert_eq!(osis(r#"<div type="x-p">Before<title>Mid</title>After</div>"#), "<p>Before</p><h>Mid</h><p>After</p>");
}

#[test]
fn lines_of_verse() {
    assert_eq!(osis(r#"<l level="1">Line one</l><l level="1">Line two</l>"#), "<l>Line one</l><l>Line two</l>");
    assert_eq!(
        osis(r#"<l sID="a" level="1"/>Line one<l eID="a"/><l sID="b" level="2"/>Line <hi type="italic">two</hi><l eID="b"/>"#),
        "<l>Line one</l><l>Line <i>two</i></l>"
    );
    assert_eq!(osis(r#"<lg sID="x"/><l level="1">A</l><lg eID="x"/>"#), "<l>A</l>");
    assert_eq!(osis(r#"<lg><l level="1">A</l><l level="1">B</l></lg>"#), "<l>A</l><l>B</l>");
}

#[test]
fn lists_and_items() {
    assert_eq!(
        osis(r#"<list><item type="x-indent-1">First</item><item type="x-indent-1">Second <hi type="bold">b</hi></item></list>"#),
        "<li>First</li><li>Second <b>b</b></li>"
    );
}

#[test]
fn tables_become_rows_of_cells() {
    let (body, stats) = conv(
        r#"<table><row><cell role="label">Name</cell><cell>Value</cell></row><row><cell>a</cell><cell><hi type="italic">b</hi></cell></row></table>"#,
        osis_opts(),
    );
    assert_eq!(body, "<tr><td>Name</td><td>Value</td></tr><tr><td>a</td><td><i>b</i></td></tr>");
    assert_eq!(stats.dropped_data.get("cell role=label"), Some(&1));
    // an empty row has nothing to keep
    assert_eq!(osis("<table><row><cell> </cell></row></table>"), "");
    assert!(osis_err(r#"<table><row><cell role="odd">x</cell></row></table>"#).contains("unknown cell role"));
    assert!(osis_err("<table><row>text<cell>x</cell></row></table>").contains("text directly inside <row>"));
    assert!(osis_err("<table><row><td>x</td></row></table>").contains("inside <row>"));
}

// ---------------------------------------------------------------------------------------
// OSIS: inline
// ---------------------------------------------------------------------------------------

#[test]
fn inline_styles() {
    assert_eq!(osis(r#"a <hi type="italic">i</hi> <hi type="underline">u</hi>"#), "<p>a <i>i</i> <i>u</i></p>");
    assert_eq!(osis(r#"<hi type="bold">b</hi>"#), "<p><b>b</b></p>");
    assert_eq!(osis(r#"x<hi type="super">2</hi>"#), "<p>x<sup>2</sup></p>");
    assert_eq!(osis(r#"<hi type="small-caps">Lord</hi>"#), "<p><sc>Lord</sc></p>");
    assert_eq!(osis(r#"<hi type="bold">a <hi type="italic">b</hi></hi>"#), "<p><b>a <i>b</i></b></p>");
    assert!(osis_err(r#"<hi type="glow">x</hi>"#).contains("unknown style"));
    assert!(osis_err(r#"<hi>x</hi>"#).contains("unknown style"));
}

#[test]
fn language_runs() {
    assert_eq!(osis(r#"the word <foreign xml:lang="he">דבר</foreign>"#), "<p>the word <lang code=\"he\">דבר</lang></p>");
    assert_eq!(osis(r#"<foreign xml:lang="grc">λόγος</foreign>"#), "<p><lang code=\"grc\">λόγος</lang></p>");
    assert!(osis_err("<foreign>x</foreign>").contains("no xml:lang"));
    assert!(osis_err(r#"<foreign xml:lang="he&quot;x">x</foreign>"#).contains("odd language code"));
}

#[test]
fn translators_additions_are_italic() {
    assert_eq!(osis(r#"God <transChange type="added">is</transChange> love"#), "<p>God <i>is</i> love</p>");
}

#[test]
fn quotations_keep_only_their_text() {
    assert_eq!(osis(r#"He said <q who="Jesus" marker="">Come</q>"#), "<p>He said Come</p>");
    assert!(osis_err(r#"<q sID="q1"/>x"#).contains("quotation milestone"));
    // marks that only an attribute holds would be lost
    assert!(osis_err("He said <q marker=\"\u{201c}\">Come</q>").contains("quotation marks that are not in its text"));
}

#[test]
fn line_breaks() {
    assert_eq!(osis(r#"one<lb/>two"#), "<p>one<br/>two</p>");
    // none at the edges and no space before one; two in a row are two (a blank line)
    assert_eq!(osis(r#"<lb/>one <lb/> <lb/>two<lb/>"#), "<p>one<br/><br/>two</p>");
}

#[test]
fn notes_become_footnotes_in_place() {
    let (body, stats) = conv(r#"<div type="x-p">Text<note placement="foot"> The note. </note> goes on.</div>"#, osis_opts());
    assert_eq!(body, "<p>Text <fn>The note.</fn> goes on.</p>");
    assert_eq!(stats.footnotes, 1);
    // A paragraph break inside a note becomes a line break
    let (body, stats) = conv(
        r#"<div type="x-p">A<note type="x-footnote"><div sID="n" type="x-p"/>First<div eID="n" type="x-p"/><div sID="m" type="x-p"/>Second<div eID="m" type="x-p"/></note>B</div>"#,
        osis_opts(),
    );
    assert_eq!(body, "<p>A<fn>First<br/>Second</fn>B</p>");
    assert_eq!(stats.footnotes, 1);
    // counted by kind, not by the milestones' ids
    assert_eq!(stats.ignored.get("<div type=x-p> inside a footnote"), Some(&4), "{:?}", stats.ignored);
    assert!(osis_err(r#"<note placement="foot"><list/></note>"#).contains("only inline content"));
}

#[test]
fn a_title_inside_a_note_is_a_line_break_around_its_text() {
    assert_eq!(osis(r#"A<note>x<title>T</title>y</note>"#), "<p>A<fn>x<br/>T<br/>y</fn></p>");
}

// ---------------------------------------------------------------------------------------
// OSIS: references
// ---------------------------------------------------------------------------------------

#[test]
fn references_carry_their_osis_range() {
    let (body, stats) = conv(r#"See <reference osisRef="Rom.8.28-Rom.8.29">Rom 8:28-29</reference>."#, osis_opts());
    assert_eq!(body, r#"<p>See <ref to="ROM.8.28-ROM.8.29">Rom 8:28-29</ref>.</p>"#);
    assert_eq!(stats.refs_resolved, 1);
    assert!(stats.unparsed.is_empty() && stats.refs_corrected.is_empty() && stats.refs_withheld.is_empty());
    // a list in one element
    assert_eq!(
        osis(r#"<reference osisRef="John.1.1 John.1.14">John 1:1, 14</reference>"#),
        r#"<p><ref to="JHN.1.1 JHN.1.14">John 1:1, 14</ref></p>"#
    );
    // a work prefix and a grain are not part of the range
    assert_eq!(osis(r#"<reference osisRef="KJV:John.3.16!a">John 3:16</reference>"#), r#"<p><ref to="JHN.3.16">John 3:16</ref></p>"#);
    // a reference with no osisRef is a plain ref, counted
    let (body, stats) = conv("<reference>John 3:16</reference>", osis_opts());
    assert_eq!(body, "<p><ref>John 3:16</ref></p>");
    assert_eq!(stats.unparsed, ["John 3:16"]);
    // an unusable osisRef likewise, and the text is kept
    let (body, stats) = conv(r#"<reference osisRef="Matt.13.3-Matt.9.18">Mt 13:3-9, 18</reference>"#, osis_opts());
    assert_eq!(body, "<p><ref>Mt 13:3-9, 18</ref></p>");
    assert_eq!(stats.unparsed, ["Matt.13.3-Matt.9.18 (Mt 13:3-9, 18)"]);
    assert_eq!(stats.refs_resolved, 0);
}

#[test]
fn an_osis_ref_the_text_contradicts_is_corrected_or_withheld() {
    // KD turns the chapters "12-14" into verse 14 of chapter 12
    let (body, stats) = conv(r#"<reference osisRef="Gen.12.14">Gen 12-14</reference>"#, osis_opts());
    assert_eq!(body, r#"<p><ref to="GEN.12-GEN.14">Gen 12-14</ref></p>"#);
    assert_eq!(stats.refs_corrected, ["Gen.12.14 (Gen 12-14)"]);
    assert_eq!(stats.refs_resolved, 1);
    // JFB turns "6:1-7; 9" into the range 6:1 to 7:9
    let (body, stats) = conv(r#"<reference osisRef="Matt.6.1-Matt.7.9">Mt 6:1-7; 9</reference>"#, osis_opts());
    assert_eq!(body, "<p><ref>Mt 6:1-7; 9</ref></p>");
    assert_eq!(stats.refs_withheld, ["Matt.6.1-Matt.7.9 (Mt 6:1-7; 9)"]);
    assert_eq!(stats.refs_resolved, 0);
    assert!(stats.unparsed.is_empty());
    // but the same verses written differently are no conflict
    for (osis_ref, shown, to) in [
        ("Gen.1.1-Gen.1.31", "Gen 1", "GEN.1.1-GEN.1.31"),
        ("Obad.1.3-Obad.1.4", "Obad. 3, 4", "OBA.1.3-OBA.1.4"),
        ("Ps.23", "Psalm 23", "PSA.23"),
    ] {
        let (body, stats) = conv(&format!(r#"<reference osisRef="{osis_ref}">{shown}</reference>"#), osis_opts());
        assert_eq!(body, format!(r#"<p><ref to="{to}">{shown}</ref></p>"#), "{shown}");
        assert!(stats.refs_corrected.is_empty() && stats.refs_withheld.is_empty(), "{shown}");
    }
    // a chapter shown, a narrower passage in it linked: the source's narrower reading stays
    assert_eq!(osis(r#"<reference osisRef="John.1.1-John.1.18">John 1.</reference>"#), r#"<p><ref to="JHN.1.1-JHN.1.18">John 1.</ref></p>"#);
}

#[test]
fn text_that_cannot_be_read_does_not_overrule_the_osis_ref() {
    // a roman numeral chapter (MHC), a bare book name, no number, a different abbreviation
    for (osis_ref, shown, to) in [
        ("Isa.1.13", "Isa. i. 13", "ISA.1.13"),
        ("Ps.50.20", "Ps. l. 20", "PSA.50.20"),
        ("Song.1.1", "Canticles", "SNG.1.1"),
        ("Isa.55.1", "lv. 1", "ISA.55.1"),
        ("John.3.16", "ver. 16", "JHN.3.16"),
        ("John.20.22", "Jon 20:22", "JHN.20.22"),
    ] {
        assert_eq!(osis(&format!(r#"<reference osisRef="{osis_ref}">{shown}</reference>"#)), format!(r#"<p><ref to="{to}">{shown}</ref></p>"#), "{shown}");
    }
}

#[test]
fn jud_means_judges_only_where_the_module_says_so() {
    // The text is read as Judges 9:36 where the module says "Jud" is Judges, so it
    // contradicts the source's 9:35; read as Jude, a different book, it cannot overrule it
    let raw = r#"<reference osisRef="Judg.9.35">Jud 9:36</reference>"#;
    let (body, stats) = conv(raw, Options { jud_is_judges: true, ..osis_opts() });
    assert_eq!(body, "<p><ref>Jud 9:36</ref></p>");
    assert_eq!(stats.refs_withheld, ["Judg.9.35 (Jud 9:36)"]);
    let (body, stats) = conv(raw, osis_opts());
    assert_eq!(body, r#"<p><ref to="JDG.9.35">Jud 9:36</ref></p>"#);
    assert!(stats.refs_withheld.is_empty());
}

// ---------------------------------------------------------------------------------------
// OSIS: structure that is dropped and counted
// ---------------------------------------------------------------------------------------

#[test]
fn structure_without_text_is_dropped_and_counted() {
    let raw = r#"<div canonical="true" osisID="Gen" sID="gen1" type="book"/><milestone n="Genesis" type="x-usfm-toc1"/><milestone type="x-usfm-toc2"/><chapter n="1" osisID="Gen.1" sID="Gen.1"/><div type="x-milestone" subType="x-preverse" sID="pv1"/><div type="x-milestone" subType="x-preverse" eID="pv1"/>"#;
    let (body, stats) = conv(raw, osis_opts());
    assert_eq!(body, "");
    assert_eq!(stats.ignored.get("chapter milestone"), Some(&1));
    assert_eq!(stats.ignored.get("div type=book"), Some(&1));
    assert_eq!(stats.ignored.get("milestone x-usfm-toc1"), Some(&1));
    assert_eq!(stats.ignored.get("div type=x-milestone"), Some(&2));
    assert_eq!(stats.dropped_data.get("milestone x-usfm-toc1 n= (book title in an attribute)"), Some(&1));
}

#[test]
fn a_container_div_keeps_its_content() {
    let (body, stats) = conv(r#"<div type="introduction"><div type="x-p">Intro</div></div>"#, osis_opts());
    assert_eq!(body, "<p>Intro</p>");
    assert_eq!(stats.ignored.values().sum::<u64>(), 1);
    assert_eq!(osis(r#"<div type="section">Text</div>"#), "<p>Text</p>");
}

// ---------------------------------------------------------------------------------------
// OSIS: errors (nothing unknown is ever dropped)
// ---------------------------------------------------------------------------------------

#[test]
fn unknown_markup_is_an_error() {
    assert!(osis_err("<blink>x</blink>").contains("unknown element <blink>"));
    assert!(osis_err("<div>x</div>").contains("no type"));
    assert!(osis_err(r#"<div type="weird">x</div>"#).contains("unknown <div"));
    assert!(osis_err(r#"<milestone type="weird"/>"#).contains("unknown milestone"));
    assert!(osis_err(r#"<hi type="bold" class="x">x</hi>"#).contains("unexpected attribute \"class\""));
    assert!(osis_err(r#"<chapter n="1">text</chapter>"#).contains("has content"));
    assert!(osis_err("<br/>").contains("unknown element <br>"), "br is ThML, not OSIS");
    assert!(osis_err(r#"<scripRef passage="Ge 1:1">1</scripRef>"#).contains("unknown element <scripRef"));
    assert!(osis_err("<p>x</p>").contains("unknown element <p>"));
}

#[test]
fn blocks_cannot_hide_inside_inline_containers() {
    assert!(osis_err(r#"<title><div type="x-p">x</div></title>"#).contains("only inline content"));
    assert!(osis_err(r#"<item><title>x</title></item>"#).contains("only inline content"));
    assert!(osis_err(r#"<l level="1"><l level="1">x</l></l>"#).contains("only inline content"));
}

#[test]
fn malformed_markup_is_an_error() {
    assert!(osis_err("<hi type=\"bold\">never closed").contains("never closed"));
    assert!(osis_err("</hi>").contains("closes nothing"));
    assert!(osis_err(r#"<hi type="bold"><b>x</hi></b>"#).contains("closes"));
    assert!(osis_err("a < b").contains("not a well-formed tag"));
    assert!(osis_err("a & b").contains("unknown entity or bare &"));
    assert!(osis_err("a &nbsp; b").contains("unknown entity"));
    assert!(osis_err("a &#0; b").contains("unknown entity"));
    assert!(osis_err("a &#xD800; b").contains("unknown entity"));
    assert!(osis_err("<hi type=bold>x</hi>").contains("not a well-formed tag"));
    assert!(osis_err("<hi type=\"a<b\">x</hi>").contains("not a well-formed tag"));
}

// ---------------------------------------------------------------------------------------
// Text, entities, escaping
// ---------------------------------------------------------------------------------------

#[test]
fn entities_are_decoded_and_only_three_characters_are_escaped() {
    assert_eq!(osis("Tom &amp; Jerry &lt;3 &gt; &quot;x&quot; &apos;y&apos;"), "<p>Tom &amp; Jerry &lt;3 &gt; \"x\" 'y'</p>");
    assert_eq!(osis("em&#8212;dash &#x2014; &#x41;"), "<p>em\u{2014}dash \u{2014} A</p>");
    // Characters other than & < > are never escaped, and the rest of Unicode is untouched
    assert_eq!(osis("“curly” ‘quotes’ — דבר λόγος 𐤀"), "<p>“curly” ‘quotes’ — דבר λόγος 𐤀</p>");
    // a literal tab or newline between words is a space
    assert_eq!(osis("a\tb\nc\r\nd"), "<p>a b c d</p>");
}

#[test]
fn non_ascii_space_characters_are_text_and_kept() {
    assert_eq!(osis("a\u{a0}b\u{2003}c"), "<p>a\u{a0}b\u{2003}c</p>");
    assert_eq!(osis("\u{a0}x"), "<p>\u{a0}x</p>");
}

#[test]
fn control_characters_and_replacement_characters_are_kept() {
    assert_eq!(osis("a\u{89}b\u{fffd}c\u{7f}"), "<p>a\u{89}b\u{fffd}c\u{7f}</p>");
}

#[test]
fn whitespace_is_collapsed_and_trimmed_inside_blocks() {
    assert_eq!(osis(r#"<div type="x-p">  a   <hi type="italic"> b </hi>  c  </div>"#), "<p>a <i>b</i> c</p>");
    // the space at the inside edge of an element is moved outside it, however deep
    assert_eq!(osis(r#"w<hi type="bold"> <hi type="italic"> x </hi> </hi>y"#), "<p>w <b><i>x</i></b> y</p>");
    assert_eq!(osis(r#"<hi type="bold"> x</hi><hi type="italic">y </hi>z"#), "<p><b>x</b><i>y</i> z</p>");
    // but never a space that was not there
    assert_eq!(osis(r#"a<hi type="italic">b</hi>c"#), "<p>a<i>b</i>c</p>");
    assert_eq!(osis(r#"<div type="x-p"><hi type="italic"> </hi>x</div>"#), "<p>x</p>");
}

#[test]
fn empty_elements_are_removed() {
    assert_eq!(osis(r#"<div type="x-p">a<hi type="italic"></hi>b</div>"#), "<p>ab</p>");
    assert_eq!(osis(r#"<div type="x-p"><hi type="bold"/></div>"#), "");
}

// ---------------------------------------------------------------------------------------
// ThML
// ---------------------------------------------------------------------------------------

#[test]
fn thml_paragraphs_and_breaks() {
    assert_eq!(thml("<p>One</p><p>Two<br />three</p>"), "<p>One</p><p>Two<br/>three</p>");
    assert_eq!(thml("<br />\nbeginning.<br /><i>mean</i>"), "<p>beginning.<br/><i>mean</i></p>");
    assert!(thml_err("<br>x</br>").contains("has content"));
}

#[test]
fn thml_inline_styles() {
    assert_eq!(thml("<i>a</i> <b>b</b> x<sup>2</sup>"), "<p><i>a</i> <b>b</b> x<sup>2</sup></p>");
}

#[test]
fn thml_references() {
    let (body, stats) = conv(r#"<br /><scripRef passage="Ge 1:1">1</scripRef> God creates;<br /><scripRef>Pr 8:22-24; 16:4; Mr 13:19</scripRef>"#, thml_opts());
    assert_eq!(
        body,
        r#"<p><ref to="GEN.1.1">1</ref> God creates;<br/><ref to="PRO.8.22-PRO.8.24 PRO.16.4 MRK.13.19">Pr 8:22-24; 16:4; Mr 13:19</ref></p>"#
    );
    assert_eq!(stats.refs_resolved, 2);
    assert!(thml_err(r#"<scripRef id="x">1</scripRef>"#).contains("unexpected attribute"));
}

#[test]
fn thml_references_that_cannot_be_read_keep_their_text() {
    let (body, stats) = conv("<scripRef>Rev 7:2 the</scripRef> text", thml_opts());
    assert_eq!(body, "<p><ref>Rev 7:2 the</ref> text</p>");
    assert_eq!(stats.unparsed, ["Rev 7:2 the"]);
    let (body, stats) = conv(r#"<scripRef passage="Ge 1:1">verse one</scripRef>"#, thml_opts());
    assert_eq!(body, r#"<p><ref to="GEN.1.1">verse one</ref></p>"#);
    assert!(stats.unparsed.is_empty());
}

#[test]
fn relative_references_use_the_notes_own_book_and_chapter() {
    let under = Options { context: Some(("GEN", 1)), ..thml_opts() };
    let (body, stats) = conv("<scripRef>22; 8:17; Ex 20:11; 31:18</scripRef>", under);
    assert_eq!(body, r#"<p><ref to="GEN.1.22 GEN.8.17 EXO.20.11 EXO.31.18">22; 8:17; Ex 20:11; 31:18</ref></p>"#);
    assert_eq!(stats.refs_resolved, 1);
    // without the context the same text has no book
    let (body, stats) = conv("<scripRef>22; 8:17</scripRef>", thml_opts());
    assert_eq!(body, "<p><ref>22; 8:17</ref></p>");
    assert_eq!(stats.unparsed, ["22; 8:17"]);
}

#[test]
fn the_treasurys_starred_labels_stay_in_the_text_but_not_in_the_link() {
    let (body, stats) = conv("<scripRef>Ps 69:34; *marg:</scripRef>", thml_opts());
    assert_eq!(body, r#"<p><ref to="PSA.69.34">Ps 69:34; *marg:</ref></p>"#);
    assert_eq!(stats.refs_resolved, 1);
    assert_eq!(thml("<scripRef>Ps 30:1; *title Joh 10:22</scripRef>"), r#"<p><ref to="PSA.30.1 JHN.10.22">Ps 30:1; *title Joh 10:22</ref></p>"#);
}

#[test]
fn jud_is_judges_in_thml_when_the_module_says_so() {
    let judges = Options { jud_is_judges: true, ..thml_opts() };
    assert_eq!(conv("<scripRef>Jud 6:24; Jude 3</scripRef>", judges).0, r#"<p><ref to="JDG.6.24 JUD.1.3">Jud 6:24; Jude 3</ref></p>"#);
    assert_eq!(thml("<scripRef>Jud 7</scripRef>"), r#"<p><ref to="JUD.1.7">Jud 7</ref></p>"#);
    assert_eq!(conv("<scripRef>Jud 7:3</scripRef>", judges).0, r#"<p><ref to="JDG.7.3">Jud 7:3</ref></p>"#);
}

#[test]
fn strongs_syncs_are_dropped_and_counted() {
    let (body, stats) = conv(r#"<p>the<sync type="Strongs" value="H0430" /> word</p>"#, thml_opts());
    assert_eq!(body, "<p>the word</p>");
    assert_eq!(stats.dropped_data.get("sync type=Strongs value="), Some(&1));
    assert!(thml_err(r#"<sync type="Other" value="x" />"#).contains("unknown <sync"));
    assert!(thml_err(r#"<sync type="Strongs" value="x">y</sync>"#).contains("unknown <sync"));
}

#[test]
fn thml_that_is_not_well_formed_keeps_its_stray_characters_as_text() {
    let (body, stats) = conv("<p>he came, &c. and a < b and c > d &amp; e</p>", thml_opts());
    assert_eq!(body, "<p>he came, &amp;c. and a &lt; b and c &gt; d &amp; e</p>");
    assert_eq!(stats.bare_ampersands, 1);
    assert_eq!(stats.literal_markup.len(), 1);
    assert!(stats.literal_markup[0].starts_with("<  in: "), "{:?}", stats.literal_markup);
    // unknown elements are still errors
    assert!(thml_err("<font>x</font>").contains("unknown element <font>"));
}

// ---------------------------------------------------------------------------------------
// The proof
// ---------------------------------------------------------------------------------------

#[test]
fn the_proof_accepts_what_the_converter_makes() {
    let raw = r#"<div type="x-p">A  b<hi type="italic">c</hi>—d<lb/>e &amp; f</div>"#;
    let (body, _) = convert(raw, osis_opts()).unwrap();
    let p = prove(raw, &body, Dialect::Osis).unwrap();
    assert_eq!(p.chars, "Abc—de&f".chars().count() as u64);
    // line breaks and block boundaries are the only gaps it may add
    assert_eq!(p.structural_gaps, 1, "the <lb/> splits d and e");
}

#[test]
fn the_proof_counts_characters_and_gaps() {
    let p = prove("<p>ab cd</p>", "<p>ab cd</p>", Dialect::Thml).unwrap();
    assert_eq!((p.chars, p.structural_gaps), (4, 0));
    // "ab" and "cd" run together in the source; the two blocks of the body separate them
    let p = prove("abcd", "<p>ab</p><p>cd</p>", Dialect::Osis).unwrap();
    assert_eq!((p.chars, p.structural_gaps), (4, 1));
}

#[test]
fn the_proof_rejects_every_kind_of_difference() {
    let raw = "<div type=\"x-p\">one two three</div>";
    let ok = "<p>one two three</p>";
    prove(raw, ok, Dialect::Osis).unwrap();
    for (what, body, want) in [
        ("a changed letter", "<p>one tvo three</p>", "a character differs"),
        ("a lost word", "<p>one three</p>", "a character differs"),
        ("a lost tail", "<p>one two</p>", "the converted text ends early"),
        ("an added word", "<p>one two three four</p>", "extra content"),
        ("an added letter", "<p>one twoo three</p>", "the source separates words the conversion joins"),
        ("joined words", "<p>onetwo three</p>", "the source separates words the conversion joins"),
        ("split words", "<p>on e two three</p>", "the conversion separates characters the source joins"),
        ("reordered words", "<p>two one three</p>", "a character differs"),
    ] {
        let e = prove(raw, body, Dialect::Osis).expect_err(what);
        assert!(e.contains(want), "{what}: {e}");
        assert!(e.contains("source:") && e.contains("converted:"), "{what}: the error shows both sides: {e}");
    }
}

#[test]
fn the_proof_rejects_a_case_difference_and_a_missing_entity() {
    assert!(prove("Lord", "<p>lord</p>", Dialect::Osis).is_err());
    assert!(prove("a &amp; b", "<p>a b</p>", Dialect::Osis).is_err());
    assert!(prove("a &amp; b", "<p>a &amp; b</p>", Dialect::Osis).is_ok());
    assert!(prove("a &lt; b", "<p>a &lt; b</p>", Dialect::Osis).is_ok());
    // the control and replacement characters are text like any other
    assert!(prove("a\u{89}b", "<p>ab</p>", Dialect::Osis).is_err());
    assert!(prove("a\u{fffd}b", "<p>a\u{fffd}b</p>", Dialect::Osis).is_ok());
    assert!(prove("a\u{a0}b", "<p>a b</p>", Dialect::Osis).is_err(), "a no-break space is not whitespace to collapse");
}

#[test]
fn the_proof_rejects_a_body_outside_the_closed_markup() {
    let raw = "x";
    for (body, want) in [
        ("<p>x</p><div>y</div>", "unknown <div> in body"),
        ("<p><blink>x</blink></p>", "unknown <blink> in body"),
        ("<p>x</p", "bad tag in body"),
        ("x", "text outside a block"),
        ("<i>x</i>", "inline <i> outside a block"),
        ("<p><p>x</p></p>", "inside"),
        ("<p>x</i>", "does not match"),
        ("<p>x", "never closed"),
        ("<p>a & b</p>", "unescaped &"),
        ("<p>a > b</p>", "unescaped >"),
        ("<p>a &quot; b</p>", "unescaped &"),
        ("<p class=\"x\">x</p>", "bad attributes"),
        ("<p><ref href=\"x\">x</ref></p>", "bad attributes"),
        ("<p><lang>x</lang></p>", "bad attributes"),
        ("<br/>", "outside a block"),
        ("<p><br class=\"x\"/>x</p>", "unknown <br/>"),
        ("<tr>x</tr>", "text outside a block"),
        ("<tr><i>x</i></tr>", "outside a block"),
        ("<td>x</td>", "inside"),
        ("<p><tr><td>x</td></tr></p>", "inside"),
    ] {
        let e = prove(raw, body, Dialect::Osis).expect_err(body);
        assert!(e.contains(want), "{body:?}: {e}");
    }
    // and the allowed shapes are accepted
    for body in [
        "<p>x</p>",
        "<h>x</h>",
        "<l>x</l>",
        "<li>x</li>",
        "<tr><td>x</td></tr>",
        "<p><i>x</i></p>",
        "<p><b><i>x</i></b></p>",
        "<p><sup>x</sup></p>",
        "<p><sc>x</sc></p>",
        "<p><fn>x</fn></p>",
        "<p><lang code=\"he\">x</lang></p>",
        "<p><ref to=\"JHN.3.16\">x</ref></p>",
        "<p><ref>x</ref></p>",
        "<p>x<br/></p>",
    ] {
        prove(raw, body, Dialect::Osis).unwrap_or_else(|e| panic!("{body}: {e}"));
    }
}

#[test]
fn the_proof_reads_the_source_itself() {
    // a source that cannot be read is an error, whatever the body says
    assert!(prove("<hi", "<p>x</p>", Dialect::Osis).is_err());
    assert!(prove("a & b", "<p>a &amp; b</p>", Dialect::Osis).is_err());
    assert!(prove("a & b", "<p>a &amp; b</p>", Dialect::Thml).is_ok());
}

#[test]
fn text_content_helpers_agree() {
    let raw = r#"<div type="x-p">Tom &amp; <hi type="italic">Jerry</hi>   (1&#8212;2)</div><div type="x-p">Next</div>"#;
    let (body, _) = convert(raw, osis_opts()).unwrap();
    // the source has nothing between the two paragraphs; the converted body has a block boundary
    assert_eq!(source_text(raw, Dialect::Osis).unwrap(), "Tom & Jerry (1\u{2014}2)Next");
    assert_eq!(body_text(&body).unwrap(), "Tom & Jerry (1\u{2014}2) Next");
    assert!(body_text("<p>x").is_err());
    assert!(source_text("<hi", Dialect::Osis).is_err());
}

#[test]
fn the_tokenizer_splits_tags_text_and_entities() {
    let mut stats = Stats::default();
    let toks = tokenize(r#"a &amp; <hi type="italic">b</hi><lb/>"#, Dialect::Osis, &mut stats).unwrap();
    assert_eq!(
        toks,
        vec![
            Tok::Text("a & ".into()),
            Tok::Open("hi".into(), vec![("type".into(), "italic".into())]),
            Tok::Text("b".into()),
            Tok::Close("hi".into()),
            Tok::Empty("lb".into(), vec![]),
        ]
    );
    // attribute values may use either quote and contain entities
    let toks = tokenize(r#"<x a='1 &amp; 2' b="&lt;"/>"#, Dialect::Osis, &mut stats).unwrap();
    assert_eq!(toks, vec![Tok::Empty("x".into(), vec![("a".into(), "1 & 2".into()), ("b".into(), "<".into())])]);
}

// ---------------------------------------------------------------------------------------
// References to verses the KJV does not have
// ---------------------------------------------------------------------------------------

#[test]
fn a_reference_to_a_chapter_or_verse_the_kjv_lacks_gets_no_link_and_keeps_its_text() {
    // Micah has seven chapters; Psalm 137 has nine verses; Philemon has one chapter
    let (body, stats) = conv(r#"<reference osisRef="Mic.35">Mic. 3:5</reference>"#, osis_opts());
    assert_eq!(body, "<p><ref>Mic. 3:5</ref></p>");
    assert_eq!(stats.refs_impossible, ["Mic.35 (Mic. 3:5)"]);
    assert_eq!((stats.refs_resolved, stats.unparsed.len(), stats.refs_withheld.len()), (0, 0, 0));
    let (body, stats) = conv(r#"<reference osisRef="Ps.137.11">Psa 137:11</reference>"#, osis_opts());
    assert_eq!(body, "<p><ref>Psa 137:11</ref></p>");
    assert_eq!(stats.refs_impossible.len(), 1);
    let (body, stats) = conv("<scripRef>Ge 1:99</scripRef> and <scripRef>Phm 2:10</scripRef>", thml_opts());
    assert_eq!(body, "<p><ref>Ge 1:99</ref> and <ref>Phm 2:10</ref></p>");
    assert_eq!(stats.refs_impossible, ["Ge 1:99", "Phm 2:10"]);
    // one bad passage in a list withholds the list
    let (_, stats) = conv("<scripRef>Ge 1:1; 99:1</scripRef>", thml_opts());
    assert_eq!(stats.refs_impossible.len(), 1);
    // the last chapter and the last verse are fine; so are books the KJV does not have
    for (raw, to) in [
        ("Rev 22:21", "REV.22.21"),
        ("Ps 119:176", "PSA.119.176"),
        ("Jn 3:36", "JHN.3.36"),
        ("Ps 150", "PSA.150"),
        ("Tob 14:2", "TOB.14.2"),
        ("Ps 151", "PS2"),
        ("Gen", "GEN"),
    ] {
        let (body, stats) = conv(&format!("<scripRef>{raw}</scripRef>"), thml_opts());
        assert_eq!(body, format!(r#"<p><ref to="{to}">{raw}</ref></p>"#), "{raw}");
        assert!(stats.refs_impossible.is_empty(), "{raw}");
    }
}
