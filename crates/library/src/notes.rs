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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_and_what_they_cover() {
        let n = parse("{\"from\":\"0:0\",\"body\":\"<p>Intro</p>\"}\n{\"from\":\"3:14\",\"to\":\"3:16\",\"body\":\"<p>x</p>\"}\n").unwrap();
        assert_eq!(n.len(), 2);
        assert!(n[0].covers(0, 0) && !n[0].covers(1, 1));
        assert!(n[1].covers(3, 15) && n[1].covers(3, 16) && !n[1].covers(3, 17) && !n[1].covers(3, 13));
        assert!(parse("{\"from\":\"3:16\",\"to\":\"3:14\",\"body\":\"\"}").is_err());
    }
}
