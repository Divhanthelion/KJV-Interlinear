//! USFM, the markup Bible translations are published in (https://ubsicap.github.io/usfm/).
//!
//! [`parse`] reads one book into chapters of blocks (paragraphs, poetry lines,
//! headings) holding verse markers, styled text, and notes. [`verses`] reduces a book
//! to the plain text of each verse, which is what search, the study assistant, and
//! the fidelity checks use.
//!
//! The parser is strict: a marker it doesn't know is an error, so no text is ever
//! dropped silently.

use std::fmt;

/// One parsed book.
#[derive(Debug, Clone, PartialEq)]
pub struct Book {
    /// USFM book code from `\id`, e.g. "GEN"
    pub code: String,
    /// Identification and table-of-contents fields: ("h", "Genesis"), ("toc1", …)
    pub headers: Vec<(String, String)>,
    /// Titles and the introduction, before chapter 1
    pub intro: Vec<Block>,
    pub chapters: Vec<Chapter>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chapter {
    pub number: u32,
    /// `\cp`: the chapter number as printed, when it differs ("A" in Greek Esther)
    pub published: Option<String>,
    pub blocks: Vec<Block>,
}

/// A paragraph-level unit: a paragraph, a poetry line, a heading.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// The USFM marker that opened it: "p", "q1", "s1", "d", "mt1", …
    pub marker: String,
    pub class: Class,
    pub content: Vec<Inline>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// Scripture text: paragraphs, poetry, lists, tables
    Text,
    /// A Psalm title (`\d` before the first verse of a chapter), part of the text
    Title,
    /// Headings, speaker labels, acrostic letters: not verse text
    Heading,
    /// Book introductions
    Intro,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    /// `\v`: the verse number ("16", "1-2", "3a"); from `\vp`, the number as printed;
    /// from `\va`, an alternate number printed beside it
    Verse { number: String, published: Option<String>, alternate: Option<String> },
    /// Text with the character styles in force, outermost first ("wj", "add", "nd", …)
    Text { text: String, styles: Vec<String> },
    /// A footnote (`f`, `fe`) or cross-reference (`x`), with its caller ("+", "-", "a")
    Note { marker: String, caller: String, parts: Vec<NotePart> },
}

/// A run of note text and the marker that styles it ("fr", "ft", "fq", "xt", …).
#[derive(Debug, Clone, PartialEq)]
pub struct NotePart {
    pub marker: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// 1-based line in the source
    pub line: usize,
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for Error {}

/// Per-translation adjustments for markup that a source uses unusually.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Paragraph markers this source uses for headings rather than text. The KJV
    /// Cambridge Paragraph Bible, for one, marks Psalm 119's letters with `\qc`.
    pub heading_markers: Vec<String>,
}

// ---------------------------------------------------------------- marker table

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Identification (`\id`, `\h`, `\toc1`): one line, kept as a header
    Header,
    /// Paragraph-level markers, by class
    Para(Class),
    /// `\d`: a Psalm title before verse 1, a heading after it
    Descriptive,
    Chapter,
    Verse,
    /// Character style; its text is kept
    Char,
    /// Character marker whose content is not text: `\vp` (kept as the printed verse
    /// number), `\ca`, `\va`, `\fig`, `\rq`, `\cat`
    CharDropped,
    /// Opens a note: `\f`, `\fe`, `\x`, `\ef`, `\ex`
    Note,
    /// Part of a note: `\fr`, `\ft`, `\xt`, …
    NotePart,
    /// Ignored markers with no content: `\pb`, `\cp`'s value is read separately
    Break,
}

/// The kind of `name` (without "+" or "*"), or None if unknown.
fn kind(name: &str) -> Option<Kind> {
    let base = name.trim_end_matches(|c: char| c.is_ascii_digit());
    Some(match base {
        "id" | "ide" | "sts" | "rem" | "h" | "toc" | "toca" | "usfm" | "periph" => Kind::Header,
        "imt" | "imte" | "is" | "ip" | "ipi" | "im" | "imi" | "ipq" | "imq" | "ipr" | "iq" | "ib" | "ili"
        | "iot" | "io" | "iex" | "ie" => Kind::Para(Class::Intro),
        "mt" | "mte" | "ms" | "mr" | "s" | "sr" | "r" | "sp" | "sd" | "cl" | "cd" | "qa" => {
            Kind::Para(Class::Heading)
        }
        "d" => Kind::Descriptive,
        "p" | "m" | "po" | "pr" | "cls" | "pmo" | "pm" | "pmc" | "pmr" | "pi" | "mi" | "nb" | "pc" | "ph" | "b"
        | "q" | "qr" | "qc" | "qm" | "qd" | "lh" | "li" | "lf" | "lim" | "tr" | "th" | "thr" | "tc" | "tcr"
        | "thc" | "tcc" | "lit" => Kind::Para(Class::Text),
        "c" => Kind::Chapter,
        "v" => Kind::Verse,
        "add" | "bk" | "dc" | "k" | "nd" | "ord" | "pn" | "png" | "addpn" | "qt" | "sig" | "sls" | "tl" | "wj"
        | "em" | "bd" | "it" | "bdit" | "no" | "sc" | "sup" | "rb" | "pro" | "w" | "wg" | "wh" | "wa" | "jmp"
        | "qs" | "qac" | "ior" | "iqt" | "litl" | "lik" | "liv" | "ndx" => Kind::Char,
        "vp" | "ca" | "va" | "fig" | "rq" | "cat" => Kind::CharDropped,
        "f" | "fe" | "x" | "ef" | "ex" => Kind::Note,
        "fr" | "ft" | "fq" | "fqa" | "fk" | "fl" | "fw" | "fp" | "fv" | "fdc" | "fm" | "xo" | "xk" | "xq" | "xt"
        | "xta" | "xop" | "xot" | "xnt" | "xdc" => Kind::NotePart,
        "pb" | "cp" => Kind::Break,
        _ => return None,
    })
}

/// Character markers whose content may carry `|attributes` after the text.
fn has_attributes(name: &str) -> bool {
    matches!(name, "w" | "rb" | "fig" | "jmp" | "xt" | "wg" | "wh" | "wa")
}

// ---------------------------------------------------------------- tokens

#[derive(Debug, Clone, Copy, PartialEq)]
enum Tok<'a> {
    /// `\name`, `\+name`, `\name*`, `\+name*`
    Marker { name: &'a str, closing: bool },
    /// `\*`, ending a milestone such as `\qt-s |who="Pilate"\*`
    MilestoneEnd,
    Text(&'a str),
}

struct Tokens<'a> {
    src: &'a str,
    pos: usize,
    line: usize,
}

impl<'a> Tokens<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, pos: 0, line: 1 }
    }

    fn next(&mut self) -> Option<(Tok<'a>, usize)> {
        let bytes = self.src.as_bytes();
        if self.pos >= bytes.len() {
            return None;
        }
        let line = self.line;
        if bytes[self.pos] != b'\\' {
            let start = self.pos;
            while self.pos < bytes.len() && bytes[self.pos] != b'\\' {
                if bytes[self.pos] == b'\n' {
                    self.line += 1;
                }
                self.pos += 1;
            }
            return Some((Tok::Text(&self.src[start..self.pos]), line));
        }
        // A marker
        self.pos += 1;
        if bytes.get(self.pos) == Some(&b'*') {
            self.pos += 1;
            return Some((Tok::MilestoneEnd, line));
        }
        if bytes.get(self.pos) == Some(&b'+') {
            self.pos += 1;
        }
        let start = self.pos;
        while self.pos < bytes.len() && (bytes[self.pos].is_ascii_alphanumeric() || bytes[self.pos] == b'-') {
            self.pos += 1;
        }
        let name = &self.src[start..self.pos];
        if bytes.get(self.pos) == Some(&b'*') {
            self.pos += 1;
            return Some((Tok::Marker { name, closing: true }, line));
        }
        // An opening marker owns one following whitespace character
        match bytes.get(self.pos) {
            Some(b' ' | b'\t') => self.pos += 1,
            Some(b'\r') => {
                self.pos += 1;
                if bytes.get(self.pos) == Some(&b'\n') {
                    self.pos += 1;
                }
                self.line += 1;
            }
            Some(b'\n') => {
                self.pos += 1;
                self.line += 1;
            }
            _ => {}
        }
        Some((Tok::Marker { name, closing: false }, line))
    }
}

// ---------------------------------------------------------------- parser

/// Parse one book.
pub fn parse(src: &str, options: &Options) -> Result<Book, Error> {
    let src = src.strip_prefix('\u{feff}').unwrap_or(src);
    let mut p = Parser {
        options,
        book: Book { code: String::new(), headers: Vec::new(), intro: Vec::new(), chapters: Vec::new() },
        block: None,
        styles: Vec::new(),
        carried: Vec::new(),
        note: None,
        dropped: None,
        attributes: false,
        milestone: false,
        pending: None,
        line: 0,
        verse_seen: false,
        header_name: String::new(),
    };
    let mut tokens = Tokens::new(src);
    while let Some((tok, line)) = tokens.next() {
        p.line = line;
        p.token(tok)?;
    }
    p.end_block();
    if p.book.code.is_empty() {
        return Err(Error { line: 1, message: "no \\id line".into() });
    }
    // A `\d` before verse 1 is a Psalm title, unless the chapter has more `\d` lines
    // between its verses: then they are all acrostic letters (Psalm 119's ALEPH, BETH…)
    for ch in &mut p.book.chapters {
        let acrostic = ch.blocks.iter().any(|b| b.marker == "d" && b.class == Class::Heading);
        if acrostic {
            for b in ch.blocks.iter_mut().filter(|b| b.class == Class::Title) {
                b.class = Class::Heading;
            }
        }
    }
    Ok(p.book)
}

/// What the next text token's leading number belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    Chapter,
    Verse,
    /// `\cp`: the printed chapter label
    PublishedChapter,
    /// A header line (`\id GEN …`, `\h Genesis`)
    Header,
    /// A note's caller ("+", "-", or a letter)
    Caller,
}

struct OpenNote {
    marker: String,
    caller: String,
    parts: Vec<NotePart>,
    /// The note part currently open ("ft" until another starts)
    part: String,
}

struct Parser<'o> {
    options: &'o Options,
    book: Book,
    block: Option<Block>,
    /// Open character styles, outermost first
    styles: Vec<String>,
    /// Styles open when a heading interrupted them, resumed by the next text paragraph
    carried: Vec<String>,
    note: Option<OpenNote>,
    /// Inside a dropped character marker (`\vp`, `\fig`, …): its name and collected text
    dropped: Option<(String, String)>,
    /// Inside a marker's `|attributes`, skipped until it closes
    attributes: bool,
    /// Inside a milestone's attributes, skipped until `\*`
    milestone: bool,
    pending: Option<(Pending, String)>,
    line: usize,
    /// A verse has started in the current chapter (decides what `\d` means)
    verse_seen: bool,
    /// The header marker whose line is being read
    header_name: String,
}

impl Parser<'_> {
    fn err(&self, message: impl Into<String>) -> Error {
        Error { line: self.line, message: message.into() }
    }

    fn token(&mut self, tok: Tok<'_>) -> Result<(), Error> {
        match tok {
            Tok::MilestoneEnd => {
                if !self.milestone {
                    return Err(self.err("\\* without a milestone"));
                }
                self.milestone = false;
                Ok(())
            }
            Tok::Text(text) => self.text(text),
            Tok::Marker { name, closing } => {
                if self.milestone {
                    return Err(self.err(format!("\\{} inside a milestone", name)));
                }
                if name.ends_with("-s") || name.ends_with("-e") {
                    // Milestones (\qt-s … \*, \ts-e \*) mark spans without content
                    self.milestone = true;
                    return Ok(());
                }
                if closing { self.close(name) } else { self.open(name) }
            }
        }
    }

    fn text(&mut self, text: &str) -> Result<(), Error> {
        if self.milestone || self.attributes {
            return Ok(());
        }
        let mut text = text;
        if let Some((pending, _)) = self.pending.take() {
            match pending {
                Pending::Header => {
                    let (name, value) = (self.pending_name(), text.trim_end());
                    if name == "id" {
                        self.book.code = value.split_whitespace().next().unwrap_or("").to_string();
                    }
                    self.book.headers.push((name, value.to_string()));
                    return Ok(());
                }
                Pending::Chapter | Pending::Verse | Pending::PublishedChapter | Pending::Caller => {
                    let trimmed = text.trim_start();
                    let end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
                    let (word, rest) = trimmed.split_at(end);
                    if word.is_empty() {
                        return Err(self.err("marker without its number"));
                    }
                    self.number(pending, word)?;
                    // The number owns one following whitespace character
                    text = rest.strip_prefix([' ', '\t', '\n']).or_else(|| rest.strip_prefix("\r\n")).unwrap_or(rest);
                }
            }
        }
        // Whitespace outside any paragraph (after "\c 1  ") is layout, not content
        if text.is_empty() || (self.block.is_none() && self.note.is_none() && text.trim().is_empty()) {
            return Ok(());
        }
        // Attributes follow "|" inside \w and its kin
        let (text, attrs) = match self.styles.last() {
            Some(s) if has_attributes(s) && self.dropped.is_none() => match text.find('|') {
                Some(i) => (&text[..i], true),
                None => (text, false),
            },
            _ => match &self.dropped {
                Some((d, _)) if has_attributes(d) => match text.find('|') {
                    Some(i) => (&text[..i], true),
                    None => (text, false),
                },
                _ => (text, false),
            },
        };
        if let Some((_, collected)) = &mut self.dropped {
            collected.push_str(text);
        } else if let Some(note) = &mut self.note {
            let marker = note.part.clone();
            match note.parts.last_mut() {
                Some(last) if last.marker == marker => last.text.push_str(text),
                _ => note.parts.push(NotePart { marker, text: text.to_string() }),
            }
        } else if !text.is_empty() {
            let styles = self.styles.clone();
            let block = self.block_mut();
            match block.content.last_mut() {
                Some(Inline::Text { text: last, styles: s }) if *s == styles => last.push_str(text),
                _ => block.content.push(Inline::Text { text: text.to_string(), styles }),
            }
        }
        if attrs {
            self.attributes = true;
        }
        Ok(())
    }

    /// The marker name stored with a pending header.
    fn pending_name(&self) -> String {
        self.header_name.clone()
    }

    fn number(&mut self, pending: Pending, word: &str) -> Result<(), Error> {
        match pending {
            Pending::Chapter => {
                self.end_block();
                let number: u32 = word.parse().map_err(|_| self.err(format!("bad chapter number {:?}", word)))?;
                self.book.chapters.push(Chapter { number, published: None, blocks: Vec::new() });
                self.verse_seen = false;
            }
            Pending::PublishedChapter => {
                let line = self.line;
                let ch = self.book.chapters.last_mut().ok_or(Error { line, message: "\\cp before \\c".into() })?;
                ch.published = Some(word.to_string());
            }
            Pending::Verse => {
                if self.book.chapters.is_empty() {
                    return Err(self.err("\\v before \\c"));
                }
                self.verse_seen = true;
                // A verse outside any paragraph starts an implicit one
                if self.block.as_ref().is_none_or(|b| b.class != Class::Text && b.class != Class::Title) {
                    self.start_block("p", Class::Text);
                }
                if self.block.as_ref().is_some_and(|b| b.class == Class::Title) {
                    // Verse 1 follows a Psalm title within the same paragraph
                    self.start_block("p", Class::Text);
                }
                self.block_mut().content.push(Inline::Verse { number: word.to_string(), published: None, alternate: None });
            }
            Pending::Caller => {
                self.note.as_mut().expect("note open").caller = word.to_string();
            }
            Pending::Header => unreachable!(),
        }
        Ok(())
    }

    fn open(&mut self, name: &str) -> Result<(), Error> {
        if self.attributes {
            return Err(self.err(format!("\\{} inside attributes", name)));
        }
        let k = kind(name).ok_or_else(|| self.err(format!("unknown marker \\{}", name)))?;
        if let Some((open, _)) = &self.dropped
            && !matches!(k, Kind::Char)
        {
            return Err(self.err(format!("\\{} inside \\{}", name, open)));
        }
        match k {
            Kind::Header => {
                self.end_block();
                self.header_name = name.to_string();
                self.pending = Some((Pending::Header, String::new()));
            }
            Kind::Para(class) => {
                self.close_note_implicitly(name)?;
                let class = if class == Class::Text && self.options.heading_markers.iter().any(|m| m == name) {
                    Class::Heading
                } else {
                    class
                };
                self.carry_styles(class);
                self.start_block(name, class);
            }
            Kind::Descriptive => {
                self.close_note_implicitly(name)?;
                let class = if self.verse_seen || self.book.chapters.is_empty() { Class::Heading } else { Class::Title };
                self.carry_styles(class);
                self.start_block(name, class);
            }
            Kind::Chapter => {
                self.close_note_implicitly(name)?;
                if let Some(open) = self.styles.iter().chain(&self.carried).next() {
                    return Err(self.err(format!("\\{} still open at the end of the chapter", open)));
                }
                self.pending = Some((Pending::Chapter, String::new()));
            }
            Kind::Verse => {
                if self.note.is_some() {
                    return Err(self.err("\\v inside a note"));
                }
                self.pending = Some((Pending::Verse, String::new()));
            }
            Kind::Char => {
                if self.dropped.is_some() {
                    // e.g. \+w inside \rq: content stays dropped
                    return Ok(());
                }
                self.styles.push(name.to_string());
            }
            Kind::CharDropped => {
                self.dropped = Some((name.to_string(), String::new()));
            }
            Kind::Note => {
                if self.note.is_some() {
                    return Err(self.err(format!("\\{} inside a note", name)));
                }
                let part = if name.starts_with('x') || name == "ex" { "xt" } else { "ft" };
                self.note = Some(OpenNote { marker: name.to_string(), caller: String::new(), parts: Vec::new(), part: part.into() });
                self.pending = Some((Pending::Caller, String::new()));
            }
            Kind::NotePart => match self.note.as_mut() {
                Some(note) => note.part = name.to_string(),
                // Outside a note (\xt in an introduction) it styles text like any other
                None => self.styles.push(name.to_string()),
            },
            Kind::Break => {
                if name == "cp" {
                    self.pending = Some((Pending::PublishedChapter, String::new()));
                }
            }
        }
        Ok(())
    }

    fn close(&mut self, name: &str) -> Result<(), Error> {
        let k = kind(name).ok_or_else(|| self.err(format!("unknown marker \\{}*", name)))?;
        if self.attributes {
            self.attributes = false;
        }
        match k {
            Kind::Char => {
                if self.dropped.is_some() {
                    return Ok(());
                }
                if let Some(i) = self.styles.iter().rposition(|s| s == name) {
                    // Closing an outer style closes the ones inside it too
                    self.styles.truncate(i);
                } else if let Some(i) = self.carried.iter().rposition(|s| s == name) {
                    self.carried.truncate(i);
                } else {
                    return Err(self.err(format!("\\{}* without \\{}", name, name)));
                }
            }
            Kind::CharDropped => {
                let (open, text) = self.dropped.take().ok_or_else(|| self.err(format!("\\{}* without \\{}", name, name)))?;
                if open != name {
                    return Err(self.err(format!("\\{}* closes \\{}", name, open)));
                }
                if name == "vp" || name == "va" {
                    self.set_verse_label(name, &text);
                }
            }
            Kind::Note => {
                let note = self.note.take().ok_or_else(|| self.err(format!("\\{}* without a note", name)))?;
                if note.marker != name {
                    return Err(self.err(format!("\\{}* closes \\{}", name, note.marker)));
                }
                // The styles around the note (\wj … \f …\f* … \wj*) carry on after it
                self.block_mut().content.push(Inline::Note { marker: note.marker, caller: note.caller, parts: note.parts });
            }
            Kind::NotePart => match self.note.as_mut() {
                // An explicitly closed part returns to the note's main text
                Some(note) => note.part = if note.marker.starts_with('x') { "xt".into() } else { "ft".into() },
                None => match self.styles.iter().rposition(|s| s == name) {
                    Some(i) => self.styles.truncate(i),
                    None => return Err(self.err(format!("\\{}* without \\{}", name, name))),
                },
            },
            _ => return Err(self.err(format!("\\{}* is not a closing marker", name))),
        }
        Ok(())
    }

    /// A paragraph marker ends any open character styles; inside a note it's an error.
    fn close_note_implicitly(&mut self, name: &str) -> Result<(), Error> {
        if self.note.is_some() {
            return Err(self.err(format!("\\{} inside a note", name)));
        }
        if let Some((open, _)) = &self.dropped {
            return Err(self.err(format!("\\{} inside \\{}", name, open)));
        }
        Ok(())
    }

    /// Character styles run on across paragraphs until closed (sources close `\wj`
    /// after a paragraph break), but not into headings: a heading sets them aside
    /// and the next text paragraph takes them up again.
    fn carry_styles(&mut self, class: Class) {
        if class == Class::Text || class == Class::Title {
            if self.styles.is_empty() {
                self.styles = std::mem::take(&mut self.carried);
            }
        } else if !self.styles.is_empty() {
            self.carried = std::mem::take(&mut self.styles);
        }
    }

    /// `\vp`: the printed number of the verse just started. Outside a verse (in an
    /// appendix quoting a passage, say) it's printed text, kept with style "vp".
    fn set_verse_label(&mut self, marker: &str, label: &str) {
        let block = self.block_mut();
        match block.content.last_mut() {
            Some(Inline::Verse { published, .. }) if marker == "vp" => *published = Some(label.trim().to_string()),
            Some(Inline::Verse { alternate, .. }) if marker == "va" => *alternate = Some(label.trim().to_string()),
            _ => block.content.push(Inline::Text { text: label.to_string(), styles: vec![marker.to_string()] }),
        }
    }

    fn start_block(&mut self, marker: &str, class: Class) {
        self.end_block();
        self.block = Some(Block { marker: marker.to_string(), class, content: Vec::new() });
    }

    fn end_block(&mut self) {
        if let Some(block) = self.block.take() {
            match self.book.chapters.last_mut() {
                Some(ch) => ch.blocks.push(block),
                None => self.book.intro.push(block),
            }
        }
    }

    fn block_mut(&mut self) -> &mut Block {
        if self.block.is_none() {
            self.block = Some(Block { marker: "p".into(), class: Class::Text, content: Vec::new() });
        }
        self.block.as_mut().unwrap()
    }
}

// ---------------------------------------------------------------- plain verse text

/// The plain text of one verse, or of a Psalm title.
#[derive(Debug, Clone, PartialEq)]
pub struct VerseText {
    pub chapter: u32,
    /// "16", "1-2", "3a"; "0" for a Psalm title
    pub number: String,
    /// A Psalm title (`\d` before the first verse), not a numbered verse
    pub title: bool,
    pub text: String,
    /// Things printed in or right before the verse that aren't its text: headings,
    /// speaker labels, acrostic letters, alternate verse numbers. Some plain-text
    /// editions run these into the verse; the fidelity checks allow for that.
    pub asides: Vec<String>,
}

/// Every verse's plain text, in order: text paragraphs only, notes and printed
/// labels removed, whitespace collapsed. A Psalm title comes first in its chapter.
pub fn verses(book: &Book) -> Vec<VerseText> {
    let mut out: Vec<VerseText> = Vec::new();
    for ch in &book.chapters {
        let mut current: Option<VerseText> = None;
        let mut title: Option<VerseText> = None;
        let mut asides: Vec<String> = Vec::new();
        for block in &ch.blocks {
            match block.class {
                Class::Heading | Class::Intro => {
                    let t = plain(&block.content);
                    if !t.is_empty() {
                        if let Some(v) = &mut current {
                            v.asides.push(t.clone());
                        }
                        asides.push(t);
                    }
                    continue;
                }
                Class::Title => {
                    let t = title.get_or_insert_with(|| VerseText {
                        chapter: ch.number,
                        number: "0".into(),
                        title: true,
                        text: String::new(),
                        asides: Vec::new(),
                    });
                    t.text.push(' ');
                    t.text.push_str(&plain(&block.content));
                    continue;
                }
                Class::Text => {}
            }
            for inline in &block.content {
                match inline {
                    Inline::Verse { number, alternate, .. } => {
                        out.extend(title.take());
                        out.extend(current.take());
                        let mut v = VerseText {
                            chapter: ch.number,
                            number: number.clone(),
                            title: false,
                            text: String::new(),
                            asides: std::mem::take(&mut asides),
                        };
                        v.asides.extend(alternate.clone());
                        current = Some(v);
                    }
                    Inline::Text { text, styles } => {
                        if let Some(v) = &mut current {
                            if is_label(styles) {
                                v.asides.push(text.trim().to_string());
                            } else {
                                v.text.push_str(text);
                            }
                        }
                    }
                    Inline::Note { .. } => {}
                }
            }
            // A paragraph or line break separates words
            if let Some(v) = &mut current {
                v.text.push(' ');
            }
        }
        out.extend(title.take());
        out.extend(current.take());
    }
    for v in &mut out {
        v.text = v.text.split_whitespace().collect::<Vec<_>>().join(" ");
    }
    out
}

/// Text content of inlines, notes and printed labels removed, whitespace collapsed.
pub fn plain(content: &[Inline]) -> String {
    let mut s = String::new();
    for i in content {
        if let Inline::Text { text, styles } = i
            && !is_label(styles)
        {
            s.push_str(text);
        }
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Printed verse numbers that no verse marker carries (`\vp` on the extra Septuagint
/// passages numbered "36)", `\va` alternate numbers) are labels, not text.
fn is_label(styles: &[String]) -> bool {
    styles.iter().any(|s| s == "vp" || s == "va")
}

/// Book codes for things that aren't books of the Bible: front and back matter,
/// glossaries, indexes, and extra material ("XXA"–"XXG").
pub fn is_peripheral(code: &str) -> bool {
    matches!(code, "FRT" | "INT" | "BAK" | "OTH" | "CNC" | "GLO" | "TDX" | "NDX") || code.starts_with("XX")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(src: &str) -> Book {
        parse(src, &Options::default()).unwrap()
    }

    #[test]
    fn words_styles_and_attributes() {
        let b = book("\\id GEN test\n\\c 1\n\\p\n\\v 1 \\w In|strong=\"H7225\"\\w* the \\nd Lord\\nd*’s \\add house\\add*.\n");
        assert_eq!(b.code, "GEN");
        let v = verses(&b);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].text, "In the Lord’s house.");
        let Inline::Text { styles, .. } = &b.chapters[0].blocks[0].content[3] else { panic!() };
        assert_eq!(styles, &["nd"]);
    }

    #[test]
    fn notes_are_not_verse_text() {
        let b = book("\\id PSA\n\\c 1\n\\q1\n\\v 1 Blessed\\f + \\fr 1:1 \\ft Or, \\fq happy\\f* is the man\n");
        let v = verses(&b);
        assert_eq!(v[0].text, "Blessed is the man");
        let Inline::Note { caller, parts, .. } = &b.chapters[0].blocks[0].content[2] else { panic!("{:?}", b.chapters[0].blocks[0].content) };
        assert_eq!(caller, "+");
        assert_eq!(parts.iter().map(|p| (p.marker.as_str(), p.text.as_str())).collect::<Vec<_>>(), [("fr", "1:1 "), ("ft", "Or, "), ("fq", "happy")]);
    }

    #[test]
    fn psalm_titles_and_mid_chapter_headings() {
        // A title before verse 1
        let b = book("\\id PSA\n\\c 23\n\\d A Psalm of David.\n\\q1\n\\v 1 The LORD is my shepherd;\n");
        let v = verses(&b);
        assert_eq!((v[0].title, v[0].number.as_str(), v[0].text.as_str()), (true, "0", "A Psalm of David."));
        // Acrostic letters, the first before verse 1: all headings, none a title
        let b = book("\\id PSA\n\\c 119\n\\d ALEPH\n\\q1\n\\v 1 Blessed are the undefiled.\n\\d BETH\n\\q1\n\\v 9 Wherewithal\n");
        let v = verses(&b);
        assert_eq!(v.len(), 2);
        assert_eq!((v[0].number.as_str(), v[0].text.as_str()), ("1", "Blessed are the undefiled."));
        assert_eq!(v[0].asides, ["ALEPH", "BETH"]);
        assert_eq!(v[1].text, "Wherewithal");
    }

    #[test]
    fn verses_span_poetry_lines_and_a_marker_owns_one_space() {
        let b = book("\\id PSA\n\\c 68\n\\q1\n\\v 32 Sing\n\\q2 to the Lord—\\qs Selah—\\qs*\n\\q1\n\\v 33 to him\n");
        let v = verses(&b);
        assert_eq!(v[0].text, "Sing to the Lord—Selah—");
    }

    #[test]
    fn published_numbers_and_unknown_markers() {
        let b = book("\\id EST\n\\c 1\n\\cp A\n\\p\n\\v 1 \\vp 1a\\vp* In the second year\n");
        assert_eq!(b.chapters[0].published.as_deref(), Some("A"));
        let Inline::Verse { published, .. } = &b.chapters[0].blocks[0].content[0] else { panic!() };
        assert_eq!(published.as_deref(), Some("1a"));
        assert_eq!(verses(&b)[0].text, "In the second year");
        let e = parse("\\id GEN\n\\c 1\n\\zzz oops\n", &Options::default()).unwrap_err();
        assert!(e.message.contains("unknown marker \\zzz"), "{}", e);
        assert_eq!(e.line, 3);
    }
}
