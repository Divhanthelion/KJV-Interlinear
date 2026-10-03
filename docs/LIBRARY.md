# The library: translations, commentaries, and context

KJV Interlinear grows from one translation with Hebrew and Greek into a study
library: every English translation we may legally ship, the classic commentaries,
the Church Fathers, and cross-references. All of it ships inside the app and works
offline. The reader then composes exactly what the study assistant sees, for
example one verse from Luke, two from Romans, and a chapter of Habakkuk, in three
translations, with notes from three commentaries.

Priorities, in order:
1. **Fidelity.** Every work is checked character by character against its source,
   as the KJV is today (see "Verification").
2. **Breadth.** As many works as the licences allow.
3. **Context engineering.** Precise, inspectable control of what goes to the model.

The interlinear stays, but it is no longer the centre of the app.

## Decisions

- **English only, all bundled, fully offline.** The app still makes no network
  requests of its own; only the opt-in assistant talks to the provider the reader
  chose.
- **Licences.** Public domain, CC0, CC BY, CC BY-SA, CC BY-ND, and CC BY-NC(-ND/-SA)
  are all acceptable. The app never charges money and never alters a text. Every
  work carries its licence and required attribution, shown in the app and in NOTICE.
  Share-alike works stay in their own data files.
- **Sources are pinned.** Every upstream file is recorded in
  `data/library/sources.toml` with its URL, size, SHA-256, retrieval date, and
  licence. The import tool works from those exact files.

## Works

| Kind | Work | Source | Keyed to | Licence |
|---|---|---|---|---|
| Bibles | ~35 English translations (list in `data/library/bibles/`) | eBible.org USFM, cross-checked against eBible VPL | each translation's own numbering | per translation |
| Commentary | Matthew Henry, Complete | CrossWire `MHC` | KJV | Public domain |
| Commentary | Catena Aurea (Aquinas) | CrossWire `Catena` | KJV | Public domain |
| Commentary | Tyndale Open Study Notes: study notes and book introductions (`tyndale`); profiles of people and theme articles (`tyndalearticles`) | `tyndale_open-studynotes.zip` from tyndaleopenresources.com (its XML) | NLT numbering, placed on the KJV's (3 John 1:15 and Revelation 12:18 are the KJV's 1:14 and 13:1) | CC BY-SA 4.0 |
| Commentary | Wesley's Notes | CrossWire `Wesley` | KJV | Public domain |
| Commentary | Keil & Delitzsch | CrossWire `KD` | KJV | Public domain |
| Commentary | John Gill | SermonIndex SWORD module `sigill` (keeps the Hebrew; HelloAO's copy strips it) | KJV | Public domain |
| Commentary | Jamieson-Fausset-Brown (1871, unabridged) | CrossWire `JFB` | KJV | Public domain |
| Commentary | Church Fathers: John Chrysostom's homilies on Matthew, John, Acts, and Paul's epistles with Hebrews (`chrysostom`); Augustine's expositions of the Psalms, tractates on John and 1 John, the Sermon on the Mount, and sermons on New Testament lessons (`augustine`) | The Nicene and Post-Nicene Fathers, First Series (vols. 6–8, 10–14), in the Christian Classics Ethereal Library's ThML (`data/library/fathers.toml` lists each series); each homily placed on the passage it expounds | KJV (the editions' Psalm numbers are the English) | Public domain translations only; no machine translations, no modern copyrighted excerpts |
| Cross-references | Treasury of Scripture Knowledge | CrossWire `TSK` (the commentary above, read as references) | KJV | Public domain |
| Cross-references | OpenBible.info | `cross-references.zip` from openbible.info (ESV numbering; 3 John 1:15 is the KJV's 1:14) | KJV | CC BY 4.0 |

## Data in the repository

```
data/library/
  sources.toml                every upstream file: URL, size, SHA-256, date, licence
  bibles.toml                 the translations: name, abbreviation, year, description, licence, credit
  bibles/<id>/index.toml      generated: its books, chapter numbers, verse counts, source and SHA-256
  bibles/<id>/<BOOK>.usfm     the source USFM, cleaned (see below), one file per book
  commentaries.toml           the commentaries: name, author, year, tradition, licence, credit
  commentaries/<id>/index.toml     generated: counts, orphans placed, ranges trimmed, references
  commentaries/<id>/<BOOK>.jsonl   one note per line: {"from":"3:16","to":"3:18","body":"…"}
                                   ("0" verse = chapter introduction, "0:0" = book introduction)
  fathers.toml                the Church Fathers' series: which division of which volume, and
                              how each homily is placed on its passage (see crates/import/src/fathers.rs)
  crossrefs.toml              the cross-reference collections: name, licence, credit; the
                              Treasury names its commentary, the others a pinned source
  crossrefs/<id>/index.toml   generated: counts, references left out or renumbered, source and SHA-256
  crossrefs/<id>/<BOOK>.tsv   chapter:verse \t to \t votes, each verse's most helpful first
  alignment/<id>.tsv          generated: the verses whose KJV counterpart isn't the same-numbered verse
```

**Bibles are stored as cleaned USFM.** USFM is the standard the sources use, and it
keeps paragraphs, poetry lines, section headings, Psalm titles, footnotes,
supplied words (`\add`), the divine name (`\nd`), and the words of Jesus (`\wj`).
Cleaning removes only things that are not the translation's text: eBible's
automatic Strong's tags (`\w word|strong="…"\w*` becomes `word`), figures, and
publishing metadata. The cleaning rule is simple enough to audit, and the plain
text of every verse is checked against eBible's separately produced VPL edition.

**Commentary notes** use a small, closed markup in `body`, described below. The
importer turns each source format (OSIS, ThML, the Tyndale notes' XML) into this one
markup; anything it doesn't recognise is an import error, never silently dropped. The
The Fathers come from the Christian Classics Ethereal Library's editions of the Nicene
and Post-Nicene Fathers (public-domain translations, read only where the edition names
the passage a homily expounds). A homily is placed by the passage its title names, or
the edition's key where the title names none; the keys disagree with the titles 26
times (Homily LXIX on Matthew, on the wedding feast of 22:1-14, is keyed 21:1-14; six
Psalms are keyed to others) and the titles are right each time, so each is settled in
`fathers.toml` and an unsettled one fails the build. Chrysostom's titles give where a
homily begins, so it runs to where the next begins. The editions' references are taken
except where they misread a Psalm number above a hundred ("Ps. cii. 27" keyed Psalm
2:27), read then from the printed text. Editors' prefaces and essays are left out.

The Tyndale notes' links to their own items (a theme article, a book's introduction,
another study note) open the passage that item is about; a link whose target is cut
short ("Gen.1.3-2") is read from its shown text ("1:3–2:3") only when that starts
where the link does. Every repair and every link left without a `to` is counted in
the commentary's `index.toml`.

### Note markup

A note body is a sequence of blocks. Text is plain Unicode with `&`, `<`, `>`
escaped as `&amp;`, `&lt;`, `&gt;`, and nothing else escaped.

| Element | Meaning |
|---|---|
| `<p>…</p>` | paragraph |
| `<h>…</h>` | a heading inside a note ("The Case of Abraham") |
| `<l>…</l>` | a line of verse (poetry quoted in a note); consecutive lines form a stanza |
| `<li>…</li>` | a list item |
| `<tr><td>…</td>…</tr>` | a table row (rare: TSK, KD) |
| `<l level="2">`, `<li level="2">` | a line or list item indented to the second (or third) level; the first level has no `level` |
| `<i>`, `<b>`, `<sup>`, `<sub>`, `<sc>` (small caps) | inline styles, nestable |
| `<lang code="he">…</lang>` | text in another language: `he`, `arc`, `grc`, `la`, `syr`, …; a transliteration in Latin letters adds the script, `he-Latn` |
| `<ref to="JHN.3.16">John 3:16</ref>` | a Scripture reference; `to` is OSIS-style, `JHN.3.16-JHN.3.18` for ranges, several ranges separated by spaces, book codes as in `books.rs`. `to` is left out when the reference can't be read with certainty; the text is always kept |
| `<fn>…</fn>` | a footnote inside a note, kept where it stands |
| `<br/>` | a line break inside a block |

Rules: blocks don't nest; inline elements nest only inside blocks; whitespace
inside a block is collapsed by readers. The text content of a converted note (all
text, entities decoded, markup removed) must equal the source's text content
exactly, character for character, apart from whitespace collapsing; the importer
checks this for every note.

Books are identified by USFM codes (`GEN`, `1SA`, `TOB`, `1MA`) in data files and
by the app's existing names in the interface. The deuterocanonical books, the
Apocrypha of the 1611 KJV, appear where a translation has them.

## Building: the import tool

`crates/import` (`kjv-import`) downloads each pinned source into `.cache/sources/`
(git-ignored), verifies its SHA-256, converts it, and writes `data/library/`.

```sh
cargo run -p kjv-import -- fetch            # download any missing source, check hashes
cargo run -p kjv-import -- build [ids…]     # convert sources into data/library/
cargo run -p kjv-import -- check            # re-convert and compare with data/library/ (CI)
```

Converted data is committed, so builds never need the network, and every change to
a text shows up in review as a readable diff. `build crossrefs` converts the
cross-reference collections; OpenBible.info's references with negative votes (more
readers found them unhelpful than helpful) are left out, and every remaining reference
must name verses the KJV has.

## In the app

- **Library archive.** `build.rs` packs `data/library/` into one archive embedded in
  the app: a table of contents plus one zstd frame per work and book. Nothing is
  decompressed until it is read, and recently used books stay in a small cache, so
  memory use stays low on phones however many works ship.
- **Catalogue.** Every work with its name, kind, description, coverage, licence,
  and attribution. An About page lists them all.
- **References.** The app speaks KJV numbering. Each translation's verses are aligned
  with the KJV's (see "Verse alignment"), and parallel reading, switching translations,
  commentary lookup, and context building all go through that one mapping.
- **Commentary panel.** The chosen commentaries' notes on the selected verse, or the
  chapter's introductions when none is selected, in whatever translation is being
  read: the verse is mapped to the KJV first, and the panel says so when the
  numbering differs (the Douay-Rheims' Psalm 22:4 is the KJV's 23:4). A chapter's
  introductions are those of each KJV chapter holding at least a quarter of its
  verses (the Douay-Rheims' Psalm 9 is the KJV's 9 and 10). Commentaries are chosen
  with chips (remembered); those with nothing on the verse, or on the book, are
  listed in a line rather than shown empty. A book's introductions come with its
  first chapter's. Long notes start folded, named by their title where they open
  with one ("Adam and Eve · Genesis 2:7-4:2"). A reference
  opens its passage in the translation being read; where one reference lists
  several places (the Treasury's "Lu 2:14; Ro 5:8; 1Jo 4:9,10,19", Wesley's "Numb
  1:22 26:14") and its text has a part for each, in order, each part opens its own.
- **Cross-references panel.** The chosen collections' references from the selected
  verse, each place shown with its words in the translation being read, numbered as
  that translation numbers it and opening there. The Treasury is shown line by line as
  it prints them (a keyword such as "God.", a remark, a date, then its places), with
  nothing left out; OpenBible.info's are one list, most helpful first, twenty at first
  and the rest on request. A range shows its first three verses. Where the translation
  hasn't a place (the New Testament in Brenton's Septuagint, a verse the BSB leaves
  out), the KJV's words are shown and marked as the KJV's.

## Verse alignment

Translations number verses differently: the Douay-Rheims follows the Vulgate's
Psalms, the Septuagint orders Jeremiah differently, Jewish editions count Psalm titles
as verses, some translations join, split, swap, or leave out verses, and the KJV
prints Susanna and the additions to Esther in its Apocrypha where Catholic Bibles
print them in Daniel and Esther. Published mapping tables cover some of this, with
known errors.

Every text in the library is English, so the library aligns verses by what they say
(`crates/library/src/align.rs`): the distinctive words two verses share (names,
numbers, rarer words), in two passes.

1. An in-order sequence alignment of each book against the KJV's, allowing one-to-one
   matches, joins of two verses, swapped pairs, and verses with no counterpart, with a
   small preference for verses carrying the same number. Then blocks: what no run of
   strong, consecutive matches anchors is lined up again, in order, against the KJV
   verses still free (the related books' too), as often as that finds runs of at least
   three anchored matches. Each pass recovers text printed in another order: the
   Septuagint's Jeremiah (the oracles against the nations after 25:13, the KJV's 26-45
   as 33-51), its Exodus 35-40, Nehemiah printed as the Septuagint's Ezra 11-23 (2
   Esdras), Greek Esther against the KJV's Esther and its Additions, the Song of the
   Three Children. A block matched just where the first alignment had it changes
   nothing; every first-alignment match no block touched is judged as before, so a
   book with no block found aligns as it always has.
2. A second pass keeps only matches the text (or strong matches on both sides)
   supports, splits joins whose halves don't both match, moves leftovers to a clearly
   better match anywhere in the book or its related books (a single verse printed
   elsewhere, relocated additions), and pairs remaining leftovers by number only where
   the text agrees a little, a neighbour is paired the same way, or the verse is empty
   here. A bridged verse ("24-30") stands for its whole range.

Mapping a verse from the KJV to a translation that has it twice (Brenton prints
Nehemiah both on its own and as Ezra 11-23) opens it in the same book first.

`kjv-import align` writes the result per translation to `data/library/alignment/<id>.tsv`:
only the verses whose KJV counterpart isn't the same-numbered verse, each with how it
was matched (content, framed, moved, number, unmatched) and its similarity, so every
row can be reviewed. Without being told any mapping, it reproduces the Vulgate's Psalm
numbering, Psalm titles counted as verses, the Romans doxology where the WEB prints it,
swapped verses, and the Septuagint's order; `crates/library/tests/alignment.rs` pins
these hard cases.
- **Reader.** Choose a translation, or read several in parallel. A notes panel shows
  the chosen commentaries and cross-references for the selected verse.

## Search

Search finds what it always has (case, curly quotes, dashes, and "æ" set aside,
anywhere in a verse, across words), now in any source
(`crates/core/src/search.rs`, `ui/js/search.js`):

- **By default it searches the translation being read.** The other choices are
  chosen translations and commentaries, or everything. The choice is remembered.
- **Each source is searched on its own**, a few at once, and its results shown as they
  arrive, grouped by source ("KJV 2 · WEB 11 · Matthew Henry 14 …"): the translation
  being read first, then the other translations, then the commentaries. With several
  sources, each shows its first 20 results, and all of them on request. A commentary
  result shows the words around the match and opens the note in the Commentary
  panel; a verse in another translation opens in that translation with the match
  marked.
- **Each book is folded once** (`crates/library/src/search.rs`) and kept for searching
  again, up to 256 MB of folded text on a computer and 64 MB on a phone; a search
  scans the folded text, so results are exact.
- **A word index**, made with the archive, lists for every word the books it occurs in
  (200,000 words, 2,989 books: 8 MB, 2.9 MB compressed in the app). A book is read only if, for
  each run of letters and digits in the query, it holds a word containing it; so a
  rare word reads only the books it is in, and no book with a match is ever ruled out
  (`crates/core/tests/search.rs` checks 1,100 searches and over 600 random pieces of
  real text against reading every book).

Speed is the goal, not a promise for every scope. Measured on a desktop (release
build, one source after another):

| Search | First time | Again |
|---|---|---|
| The KJV, "love" (the old search: about 60 ms) | 84 ms | 5 ms |
| The WEB, "love" | 26 ms | 4 ms |
| Matthew Henry, "love" | 290 ms | 135 ms |
| Everything, "melchizedek" (127 of 2,989 books read) | 1.1 s | 0.25 s |
| Everything, "love" (nearly every book) | 2.2 s | 2.3 s |

A common word across everything reads nearly all 325 MB of text, more than is kept,
so it costs about the same each time; results arrive source by source meanwhile.
`crates/core/tests/search.rs` fails if searching the translation being read again
takes 100 ms or more.

## Context engine

What the study assistant reads with each question is the reader's to choose
(`crates/core/src/context.rs` writes it; `ui/js/context.js` is the editor):

- **Passages**, in order: typed naturally ("Luke 2:14; Rom 5:1-2; Micah 6" adds three;
  "Gen 1:1, 3, 5-7" is one), or following the reader (this verse, this chapter, this
  book, wherever they are), or the whole Bible or a Testament. A passage is kept in
  the numbering of the translation it was chosen in, so "Psalm 22" chosen while
  reading the Douay-Rheims is its Psalm 22 (the KJV's 23), labelled "Psalm 22 (DRA
  numbering)".
- **Sources**: any translations (by default the one being read), any commentaries,
  OpenBible.info's or the Treasury's cross-references (5, 10, 25, or every place per
  verse, or per word in the Treasury; with or without their words, in the passage's
  first translation), and the KJV's Hebrew and Greek words with Strong's numbers
  and, if wanted, the full lexicon entries. Chosen once for all the passages; any
  passage can have its own.
- **Sizes**: the meter shows the total against the model's window, and the editor
  shows each passage's share and each source's within it. Sizing stops counting at
  about 2.6M tokens, more than any model reads, so asking for everything stays quick.
- **What's sent** shows the instructions and text exactly as the model gets them,
  with Copy, to use with any assistant.
- **Saved contexts** are named and used again. Each question keeps the context it was
  asked with (passages fixed where they were); reopening a conversation sends its
  follow-up questions with that context until a new conversation starts.

Every other translation's text is found verse by verse through the verse alignment;
commentaries and cross-references through the KJV's verses. A translation that leaves
a verse out says so ("Not in this translation: Matthew 17:21 in the KJV"). The KJV's
66 books come from the app's KJV, so they read exactly as the reader shows them.
Commentaries give their notes on any of the passage's verses, and their chapter and
book introductions with whole chapters (a book's with its first). A note reached from
two passages is given once, and the second time names where it was given.

The text is plain lines in a few XML-style elements, described to the model in the
instructions (which also name each translation and commentary, with its author, date,
and tradition):

```
<context>
<passage ref="Psalm 22 (DRA numbering)" numbering="DRA">
<bible translation="Douay-Rheims Bible" abbr="DRA" year="1899 (Challoner revision)" ref="Psalm 22">
## Psalm 22
1 A psalm for David. The Lord ruleth me: and I shall want nothing.
…
</bible>
<bible translation="King James Version" abbr="KJV" year="1611 (1769 text)" ref="Psalm 23">
## Psalm 23
(title) A Psalm of David.
   Hebrew: מִזְמ֥וֹר H4210 a psalm | לְדָוִ֑ד H1732 of David
1 The LORD is my shepherd; I shall not want.
…
</bible>
<commentary name="Matthew Henry's Complete Commentary" author="Matthew Henry" year="1706–1721">
<note on="Psalm 23 (introduction)">
Many of David's psalms are full of complaints, but this is full of comforts, and the expressions of delight in God's great goodness and dependence upon him. …
…
</note>
</commentary>
<crossrefs name="OpenBible.info Cross References" numbering="KJV">
Psalm 23:1
- Philippians 4:19: And may my God supply all your want, according to his riches in glory in Christ Jesus.
…
</crossrefs>
</passage>
<definitions>
## H4210 מִזְמוֹר · miz.mor · H:N-M · melody
…
</definitions>
</context>
```

Notes are given as plain text: each block on its own line, headings marked "###",
small capitals written as capitals ("LORD"), and footnotes in brackets where they stand.

## Verification

The same standard as the KJV today (`crates/core/tests/text_fidelity.rs`):

- **Bibles.** The plain text of every verse matches eBible's VPL edition character for
  character. A lock file records each chapter's SHA-256 and every character's count.
  Typography checks and on-screen sweeps cover every translation.
- **The KJV gets a second witness.** It is compared with the Cambridge Paragraph Bible,
  and every difference is listed and adjudicated in a committed file.
- **Commentaries.** Every note in the source appears in the data, under the same verse,
  with the same text once markup is removed. Counts are checked per book. No unknown
  markup may remain. Spot checks pin known notes to known verses.
- **Cross-references.** Every place in both collections names verses the KJV has
  (`crates/library/tests/crossrefs.rs`); reading the Treasury's notes as lines keeps
  every place and every word; a sweep asks for the references of thousands of verses
  in every translation and checks every place has a label and words.
- **Alignment.** Every row names verses that exist on both sides; the hard cases
  (the Vulgate Psalms, the Romans doxology, Susanna, Esther's additions, Hebrew
  numbering, omitted verses) have explicit tests; tables are reproducible byte for
  byte.
- **Reproducibility.** CI runs `kjv-import check`: re-converting the pinned sources
  must reproduce `data/library/` exactly.

## Milestones

Each milestone is a pull request into `feature/library` and must pass every check
before the next begins.

1. **Foundations.** USFM parser, library archive and lazy loading, catalogue,
   versification, import tool, and the first three translations end to end.
2. **All translations.** Every licensed English translation, fidelity locks,
   translation picker, parallel reading, search in any translation.
3. **Commentaries.** All ten sources, the notes panel, About and licences.
4. **Cross-references.** TSK and OpenBible in the notes panel.
5. **Context engine.** Context sets, the builder interface, preview, saved sets,
   conversations that remember their context.
6. **Finish.** Performance on phones, app size, accessibility, documentation, and
   NOTICE / privacy / store listing updates.
