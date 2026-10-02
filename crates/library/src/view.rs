//! A chapter of any translation, ready to draw: each verse with the headings before
//! it, its text in styled pieces, the line and paragraph breaks inside it (poetry),
//! and its notes.

use serde::Serialize;

use crate::usfm::{self, Class, Inline};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChapterView {
    pub chapter: u32,
    /// `\cp`: the chapter number as this translation prints it, if different
    pub published: Option<String>,
    /// The Psalm title, when the translation sets it apart from verse 1
    pub title: Option<VerseView>,
    pub verses: Vec<VerseView>,
    /// Headings after the last verse (rare)
    pub after: Vec<Heading>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VerseView {
    /// "16", "1-2", "3a"; "0" for a Psalm title
    pub number: String,
    /// How the translation prints the number, when it differs (`\vp`)
    pub published: Option<String>,
    /// An alternate number printed beside it (`\va`)
    pub alternate: Option<String>,
    /// Section headings, speaker labels, and the like, shown before the verse
    pub before: Vec<Heading>,
    /// The kind of paragraph or poetry line the verse starts ("p", "q1", "m", …), if it starts one
    pub starts: Option<String>,
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Heading {
    /// "s1", "ms1", "sp", "d", "qa", "r", …
    pub marker: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Part {
    /// Text and its character styles ("wj", "add", "nd", "it", …), outermost first
    Text { text: String, styles: Vec<String> },
    /// A new paragraph or poetry line begins inside the verse
    Break { kind: String },
    /// A footnote or cross-reference
    Note { marker: String, caller: String, parts: Vec<NotePart> },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NotePart {
    pub marker: String,
    pub text: String,
}

/// Chapter `number` of `book`, or None if the book has no such chapter.
pub fn chapter(book: &usfm::Book, number: u32) -> Option<ChapterView> {
    let ch = book.chapters.iter().find(|c| c.number == number)?;
    let mut view = ChapterView { chapter: number, published: ch.published.clone(), title: None, verses: Vec::new(), after: Vec::new() };
    let mut pending: Vec<Heading> = Vec::new();
    for block in &ch.blocks {
        match block.class {
            Class::Heading | Class::Intro => {
                let text = usfm::plain(&block.content);
                if !text.is_empty() {
                    pending.push(Heading { marker: block.marker.clone(), text });
                }
                continue;
            }
            Class::Title => {
                let title = view.title.get_or_insert_with(|| VerseView {
                    number: "0".into(),
                    published: None,
                    alternate: None,
                    before: std::mem::take(&mut pending),
                    starts: Some(block.marker.clone()),
                    parts: Vec::new(),
                });
                if !title.parts.is_empty() {
                    title.parts.push(Part::Break { kind: block.marker.clone() });
                }
                push_content(&mut title.parts, &block.content);
                continue;
            }
            Class::Text => {}
        }
        // A text block: verses may start in it, or it continues the current verse
        let mut first = true;
        for inline in &block.content {
            match inline {
                Inline::Verse { number, published, alternate } => {
                    view.verses.push(VerseView {
                        number: number.clone(),
                        published: published.clone(),
                        alternate: alternate.clone(),
                        before: std::mem::take(&mut pending),
                        starts: first.then(|| block.marker.clone()),
                        parts: Vec::new(),
                    });
                }
                other => {
                    let Some(v) = view.verses.last_mut() else { continue };
                    if first && !v.parts.is_empty() {
                        v.parts.push(Part::Break { kind: block.marker.clone() });
                    }
                    push_content(&mut v.parts, std::slice::from_ref(other));
                }
            }
            first = false;
        }
        if block.content.is_empty()
            && let Some(v) = view.verses.last_mut()
        {
            // A blank line (\b) between stanzas
            v.parts.push(Part::Break { kind: block.marker.clone() });
        }
    }
    view.after = pending;
    for v in view.verses.iter_mut().chain(view.title.iter_mut()) {
        tidy(&mut v.parts);
    }
    Some(view)
}

fn push_content(parts: &mut Vec<Part>, content: &[Inline]) {
    for inline in content {
        match inline {
            Inline::Text { text, styles } => {
                let styles: Vec<String> = styles.iter().filter(|s| *s != "w").cloned().collect();
                match parts.last_mut() {
                    Some(Part::Text { text: t, styles: s }) if *s == styles => t.push_str(text),
                    _ => parts.push(Part::Text { text: text.clone(), styles }),
                }
            }
            Inline::Note { marker, caller, parts: np } => parts.push(Part::Note {
                marker: marker.clone(),
                caller: caller.clone(),
                parts: np.iter().map(|p| NotePart { marker: p.marker.clone(), text: collapse(&p.text) }).collect(),
            }),
            Inline::Verse { .. } => {}
        }
    }
}

/// Whitespace collapsed inside text, trimmed at the verse's ends and around breaks.
fn tidy(parts: &mut Vec<Part>) {
    for p in parts.iter_mut() {
        if let Part::Text { text, .. } = p {
            *text = collapse(text);
        }
    }
    // No breaks at either end of a verse
    while matches!(parts.last(), Some(Part::Break { .. })) {
        parts.pop();
    }
    while matches!(parts.first(), Some(Part::Break { .. })) {
        parts.remove(0);
    }
    let n = parts.len();
    for i in 0..n {
        let before_break = i + 1 < n && matches!(parts[i + 1], Part::Break { .. });
        let after_break = i > 0 && matches!(parts[i - 1], Part::Break { .. });
        if let Part::Text { text, .. } = &mut parts[i] {
            if i == 0 || after_break {
                *text = text.trim_start().to_string();
            }
            if i + 1 == n || before_break {
                *text = text.trim_end().to_string();
            }
        }
    }
    parts.retain(|p| !matches!(p, Part::Text { text, .. } if text.is_empty()));
}

/// Whitespace runs as single spaces. Spaces at the ends are kept: they separate this
/// piece's words from its neighbours' ("LORD" + " is my shepherd").
fn collapse(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            space = true;
        } else {
            if space {
                out.push(' ');
            }
            space = false;
            out.push(c);
        }
    }
    if space {
        out.push(' ');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usfm::{Options, parse};

    #[test]
    fn verses_headings_poetry_and_notes() {
        let src = "\\id PSA\n\\c 23\n\\d A Psalm of David.\n\\q1\n\\v 1 The \\nd LORD\\nd* is my shepherd;\n\\q2 I shall not want.\\f + \\fr 23:1 \\ft Or, lack\\f*\n\\s1 Comfort\n\\q1\n\\v 2 He makes me lie down\n";
        let book = parse(src, &Options::default()).unwrap();
        let v = chapter(&book, 23).unwrap();
        let title = v.title.as_ref().unwrap();
        assert_eq!(title.parts, [Part::Text { text: "A Psalm of David.".into(), styles: vec![] }]);
        assert_eq!(v.verses.len(), 2);
        let v1 = &v.verses[0];
        assert_eq!(v1.starts.as_deref(), Some("q1"));
        assert_eq!(
            v1.parts,
            [
                Part::Text { text: "The ".into(), styles: vec![] },
                Part::Text { text: "LORD".into(), styles: vec!["nd".into()] },
                Part::Text { text: " is my shepherd;".into(), styles: vec![] },
                Part::Break { kind: "q2".into() },
                Part::Text { text: "I shall not want.".into(), styles: vec![] },
                Part::Note {
                    marker: "f".into(),
                    caller: "+".into(),
                    parts: vec![NotePart { marker: "fr".into(), text: "23:1 ".into() }, NotePart { marker: "ft".into(), text: "Or, lack".into() }],
                },
            ]
        );
        assert_eq!(v.verses[1].before, [Heading { marker: "s1".into(), text: "Comfort".into() }]);
        assert!(chapter(&book, 24).is_none());
    }
}
