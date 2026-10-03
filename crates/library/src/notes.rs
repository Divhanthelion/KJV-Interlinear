//! Commentaries: their catalogue, and the notes on a verse.
//!
//! Notes are keyed to the KJV's numbering (every commentary in the library is), as
//! verse ranges `from`..=`to`; verse 0 is a chapter's introduction, chapter 0 the
//! book's. Bodies use the library's note markup (docs/LIBRARY.md).

use serde::{Deserialize, Serialize};

/// One commentary, as listed in the archive's `commentaries.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommentaryInfo {
    pub id: String,
    pub name: String,
    /// For buttons: "Matthew Henry", "Tyndale"
    #[serde(default)]
    pub short: Option<String>,
    pub author: String,
    pub year: String,
    pub tradition: String,
    pub coverage: String,
    pub licence: String,
    pub credit: String,
    pub about: String,
    /// Book codes it has notes on, in the app's order
    pub books: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Note {
    /// (chapter, verse); (c, 0) a chapter introduction, (0, 0) the book's
    pub from: (u32, u32),
    pub to: (u32, u32),
    pub body: String,
}

impl Note {
    /// Whether the note covers verse `v` of chapter `c` (verse 0: the chapter itself).
    pub fn covers(&self, c: u32, v: u32) -> bool {
        self.from <= (c, v) && (c, v) <= self.to
    }
}

#[derive(Deserialize)]
struct Line {
    from: String,
    #[serde(default)]
    to: Option<String>,
    body: String,
}

fn place(s: &str) -> Result<(u32, u32), String> {
    let (c, v) = s.split_once(':').ok_or_else(|| format!("bad note reference {:?}", s))?;
    Ok((c.parse().map_err(|_| format!("bad chapter {:?}", s))?, v.parse().map_err(|_| format!("bad verse {:?}", s))?))
}

/// Parse a commentary book (JSON Lines).
pub fn parse(jsonl: &str) -> Result<Vec<Note>, String> {
    jsonl
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let line: Line = serde_json::from_str(l).map_err(|e| format!("note: {}", e))?;
            let from = place(&line.from)?;
            let to = match &line.to {
                Some(t) => place(t)?,
                None => from,
            };
            if to < from {
                return Err(format!("note {}–{} runs backwards", line.from, line.to.unwrap_or_default()));
            }
            Ok(Note { from, to, body: line.body })
        })
        .collect()
}

/// A note's body (in the library's note markup, docs/LIBRARY.md) as plain text: each block
/// on its own line (headings marked "###", list items "-", table cells "|"), inline
/// styles dropped except small capitals, written as capitals ("LORD"), footnotes in
/// brackets where they stand, references as printed.
pub fn text(body: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut block = String::new();
    let mut prefix = String::new();
    let mut small_caps = 0usize;
    let mut footnote = 0usize;
    let mut cells = 0usize;
    let flush = |block: &mut String, prefix: &str, out: &mut Vec<String>| {
        let lines: Vec<String> =
            block.split('\n').map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|l| !l.is_empty()).collect();
        if !lines.is_empty() {
            out.push(format!("{}{}", prefix, lines.join("\n")));
        }
        block.clear();
    };
    let mut rest = body;
    while !rest.is_empty() {
        if let Some(tag_on) = rest.strip_prefix('<')
            && let Some(end) = tag_on.find('>')
        {
            let tag = &tag_on[..end];
            rest = &tag_on[end + 1..];
            let closing = tag.starts_with('/');
            let inner = tag.trim_start_matches('/').trim_end_matches('/');
            let name = inner.split_whitespace().next().unwrap_or("");
            let level = inner
                .split_once("level=\"")
                .and_then(|(_, v)| v.split('"').next())
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1)
                .clamp(1, 3);
            match (name, closing) {
                ("p" | "h" | "l" | "li" | "tr", false) => {
                    flush(&mut block, &prefix, &mut out);
                    cells = 0;
                    let indent = "  ".repeat(level - 1);
                    prefix = match name {
                        "h" => "### ".to_string(),
                        "l" => indent,
                        "li" => format!("{}- ", indent),
                        _ => String::new(),
                    };
                }
                ("p" | "h" | "l" | "li" | "tr", true) => {
                    flush(&mut block, &prefix, &mut out);
                    prefix.clear();
                }
                ("td", false) => {
                    if cells > 0 {
                        block.push_str(" | ");
                    }
                    cells += 1;
                }
                ("sc", false) => small_caps += 1,
                ("sc", true) => small_caps = small_caps.saturating_sub(1),
                ("fn", false) => {
                    footnote += 1;
                    block.push_str(" [");
                }
                ("fn", true) => {
                    footnote = footnote.saturating_sub(1);
                    block.push(']');
                }
                ("br", _) => block.push(if footnote > 0 { ' ' } else { '\n' }),
                _ => {}
            }
            continue;
        }
        let first = rest.chars().next().map_or(1, char::len_utf8);
        let end = rest[first..].find('<').map_or(rest.len(), |i| i + first);
        let raw = &rest[..end];
        rest = &rest[end..];
        let text = raw.replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&");
        if small_caps > 0 {
            block.push_str(&text.to_uppercase());
        } else {
            block.push_str(&text);
        }
    }
    flush(&mut block, &prefix, &mut out);
    // A footnote that starts or ends with a line break: no space inside its brackets
    out.join("\n").replace("[ ", "[").replace(" ]", "]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_as_plain_text() {
        assert_eq!(
            text("<p>The <sc>Lord</sc> is <i>my</i> shepherd.</p><p>Second &amp; last.</p>"),
            "The LORD is my shepherd.\nSecond & last."
        );
        assert_eq!(
            text("<h>The Case</h><p>See <ref to=\"JHN.3.16\">John iii. 16</ref>.<fn>Gr. <lang code=\"grc\">ἀγάπη</lang>.</fn></p>"),
            "### The Case\nSee John iii. 16. [Gr. ἀγάπη.]"
        );
        assert_eq!(
            text("<l>Line one,</l><l level=\"2\">indented.</l><li>item</li><li level=\"2\">sub</li>"),
            "Line one,\n  indented.\n- item\n  - sub"
        );
        assert_eq!(text("<tr><td>a</td><td>b</td></tr><p>x<br/>y</p><p>z<fn><br/>note</fn></p>"), "a | b\nx\ny\nz [note]");
    }

    #[test]
    fn notes_and_what_they_cover() {
        let n = parse("{\"from\":\"0:0\",\"body\":\"<p>Intro</p>\"}\n{\"from\":\"3:14\",\"to\":\"3:16\",\"body\":\"<p>x</p>\"}\n").unwrap();
        assert_eq!(n.len(), 2);
        assert!(n[0].covers(0, 0) && !n[0].covers(1, 1));
        assert!(n[1].covers(3, 15) && n[1].covers(3, 16) && !n[1].covers(3, 17) && !n[1].covers(3, 13));
        assert!(parse("{\"from\":\"3:16\",\"to\":\"3:14\",\"body\":\"\"}").is_err());
    }
}
