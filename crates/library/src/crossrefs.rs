//! Cross-references: the catalogue, and the references from a verse.
//!
//! Two kinds, both keyed to the KJV's numbering. A list (OpenBible.info) is one file
//! per book, `chapter:verse \t to \t votes`, each verse's references most helpful
//! first. The Treasury is a commentary already; its notes are read as lines of words
//! (keywords, remarks, dates) and the places they name, in order (see [`lines`]).

use serde::{Deserialize, Serialize};

/// One collection, as listed in the archive's `crossrefs.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrossrefInfo {
    pub id: String,
    pub name: String,
    /// For buttons: "Treasury", "OpenBible"
    pub short: String,
    pub licence: String,
    pub credit: String,
    pub about: String,
    /// The commentary whose notes are these references, if any (the Treasury)
    #[serde(default)]
    pub commentary: Option<String>,
    /// Book codes it has references from, in the app's order
    pub books: Vec<String>,
}

/// One reference in a list: from KJV verse `from` to `to` ("ROM.5.8",
/// "2CO.5.19-2CO.5.21"), with the readers' votes for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Xref {
    pub from: (u32, u32),
    pub to: String,
    pub votes: i32,
}

/// Parse a list's book (`chapter:verse \t to \t votes`; `#` starts a comment line).
pub fn parse(tsv: &str) -> Result<Vec<Xref>, String> {
    let mut out = Vec::new();
    for (n, line) in tsv.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let bad = || format!("cross-references line {}: {:?}", n + 1, line);
        let mut fields = line.split('\t');
        let (Some(from), Some(to), Some(votes), None) = (fields.next(), fields.next(), fields.next(), fields.next()) else {
            return Err(bad());
        };
        let (c, v) = from.split_once(':').ok_or_else(bad)?;
        out.push(Xref {
            from: (c.parse().map_err(|_| bad())?, v.parse().map_err(|_| bad())?),
            to: to.to_string(),
            votes: votes.parse().map_err(|_| bad())?,
        });
    }
    Ok(out)
}

/// A place a line points to, with its votes where the collection has them.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Target {
    /// "LUK.2.14", "2CO.5.19-2CO.5.21" (KJV numbering)
    pub to: String,
    pub votes: Option<i32>,
}

/// A line of references: its words (a keyword such as "God.", a remark, a date; empty
/// for a plain list) and the places it names.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct Line {
    pub text: String,
    pub refs: Vec<Target>,
}

/// A note in the library's markup (docs/LIBRARY.md) read as lines of references. A
/// line ends at `<br/>` and at every block; a reference's places are kept and its
/// printed form ("Lu 2:14; Ro 5:8") left out, since the places are shown instead. A
/// reference that couldn't be read (no `to`) keeps its printed form as words. Places
/// alone on a line belong to the line before (the Treasury's keyword, then its
/// references, sometimes over several lines).
pub fn lines(body: &str) -> Vec<Line> {
    let mut out: Vec<Line> = Vec::new();
    for line in raw_lines(body) {
        match out.last_mut() {
            Some(last) if line.text.is_empty() => last.refs.extend(line.refs),
            _ => out.push(line),
        }
    }
    out
}

fn raw_lines(body: &str) -> Vec<Line> {
    const BLOCKS: &[&str] = &["p", "h", "l", "li", "tr", "td", "br"];
    let mut out: Vec<Line> = Vec::new();
    let mut line = Line::default();
    let mut text = String::new();
    // Inside a reference with places: its printed form is skipped
    let mut skipping = false;
    let mut rest = body;
    let finish = |line: &mut Line, text: &mut String, out: &mut Vec<Line>| {
        let words = text.split_whitespace().collect::<Vec<_>>().join(" ");
        text.clear();
        if !words.is_empty() || !line.refs.is_empty() {
            line.text = words;
            out.push(std::mem::take(line));
        }
    };
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('<') {
            let Some(end) = after.find('>') else { break };
            let tag = &after[..end];
            rest = &after[end + 1..];
            let closing = tag.starts_with('/');
            let name: String = tag.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            if name == "ref" {
                if closing {
                    skipping = false;
                } else if let Some(to) = attribute(tag, "to") {
                    line.refs.extend(to.split_whitespace().map(|t| Target { to: t.to_string(), votes: None }));
                    skipping = true;
                }
            } else if BLOCKS.contains(&name.as_str()) {
                finish(&mut line, &mut text, &mut out);
            }
        } else {
            let end = rest.find('<').unwrap_or(rest.len());
            if !skipping {
                text.push_str(&decode(&rest[..end]));
            }
            rest = &rest[end..];
        }
    }
    finish(&mut line, &mut text, &mut out);
    out
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let start = tag.find(&format!("{}=\"", name))? + name.len() + 2;
    let end = tag[start..].find('"')? + start;
    Some(decode(&tag[start..end]))
}

/// The markup escapes only `&`, `<`, `>` (and `"` inside attributes).
fn decode(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to(line: &Line) -> Vec<&str> {
        line.refs.iter().map(|t| t.to.as_str()).collect()
    }

    #[test]
    fn a_list() {
        let x = parse("# from\tto\tvotes\n3:16\tROM.5.8\t984\n3:16\t2CO.5.19-2CO.5.21\t213\n").unwrap();
        assert_eq!(x.len(), 2);
        assert_eq!(x[1], Xref { from: (3, 16), to: "2CO.5.19-2CO.5.21".into(), votes: 213 });
        assert!(parse("3:16\tROM.5.8").is_err());
        assert!(parse("3-16\tROM.5.8\t1").is_err());
    }

    #[test]
    fn the_treasury_read_as_lines() {
        // Keywords, each followed by its references
        let l = lines("<p>God.<br/><ref to=\"LUK.2.14 ROM.5.8\">Lu 2:14; Ro 5:8</ref><br/>gave.<br/><ref to=\"JHN.1.14\">1:14</ref></p>");
        assert_eq!(l.len(), 2);
        assert_eq!((l[0].text.as_str(), to(&l[0])), ("God.", vec!["LUK.2.14", "ROM.5.8"]));
        assert_eq!((l[1].text.as_str(), to(&l[1])), ("gave.", vec!["JHN.1.14"]));
        // A remark with no places stays a line of its own; places continue the line before
        let l = lines("<p>Heb. journeyed.<br/>the land.Milk and honey.<br/><ref to=\"EXO.3.8\">Ex 3:8</ref><br/><ref to=\"NUM.13.27\">Nu 13:27</ref></p>");
        assert_eq!(l.iter().map(|x| (x.text.as_str(), x.refs.len())).collect::<Vec<_>>(), [("Heb. journeyed.", 0), ("the land.Milk and honey.", 2)]);
        // Places with nothing before them make a line of their own
        let l = lines("<p><ref to=\"PSA.8.2\">Ps 8:2</ref><br/><ref to=\"MAT.21.16\">Mt 21:16</ref></p>");
        assert_eq!((l.len(), l[0].text.as_str(), to(&l[0])), (1, "", vec!["PSA.8.2", "MAT.21.16"]));
        // A chapter outline: a place and what is there
        let l = lines("<p><ref to=\"LEV.22.6\">6</ref> How they shall be cleansed.</p>");
        assert_eq!((l[0].text.as_str(), to(&l[0])), ("How they shall be cleansed.", vec!["LEV.22.6"]));
        // Words are decoded and spaced; a reference that couldn't be read keeps its words
        let l = lines("<p>A &amp; B  <i>c</i>.<br/><ref>Ps 151</ref></p>");
        assert_eq!(l[0].text, "A & B c.");
        assert_eq!((l[1].text.as_str(), l[1].refs.len()), ("Ps 151", 0));
        assert!(lines("<p> </p>").is_empty());
    }
}
