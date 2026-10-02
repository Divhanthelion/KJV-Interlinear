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
| Commentary | Tyndale Open Study Notes | `tyndale_open-studynotes.zip` from tyndaleopenresources.com | English (NLT wording) | CC BY-SA 4.0 |
| Commentary | Wesley's Notes | CrossWire `Wesley` | KJV | Public domain |
| Commentary | Keil & Delitzsch | CrossWire `KD` | KJV | Public domain |
| Commentary | John Gill | SermonIndex SWORD module `sigill` (keeps the Hebrew; HelloAO's copy strips it) | KJV | Public domain |
| Commentary | Jamieson-Fausset-Brown (1871, unabridged) | CrossWire `JFB` | KJV | Public domain |
| Commentary | Church Fathers | NPNF/ANF/Oxford translations (tertullian.org, Writings-Database), keyed by homily headings; filtered HCF index for the rest | verse ranges | Public domain translations only; no machine translations, no modern copyrighted excerpts |
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
importer turns each source format (OSIS, ThML, JSON, XML) into this one markup;
anything it doesn't recognise is an import error, never silently dropped.

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
| `<i>`, `<b>`, `<sup>`, `<sc>` (small caps) | inline styles, nestable |
| `<lang code="he">…</lang>` | text in another language: `he`, `arc`, `grc`, `la`, `syr`, … |
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
  listed in a line rather than shown empty. Long notes start folded. A reference
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

Search must stay as fast as it is today (about 60 ms for the whole KJV, end to end).

- **By default it searches the translation being read**, at today's speed: a straight
  scan of one Bible, with the folded (case- and accent-insensitive) text prepared
  once per translation instead of on every query.
- **The scope is configurable**: the current translation, chosen translations, chosen
  commentaries, or everything. The choice is remembered.
- **Wider scopes use a word index** built when the app is compiled: for each word, the
  verses and notes that contain it. A query intersects the lists for its words and then
  confirms each candidate against the real text, so results are exact. Target: under
  200 ms for everything, on a phone.
- **Results are grouped by source** ("KJV 12 · DRA 9 · Matthew Henry 31"), each group
  expandable.
- **Speed is tested**: a benchmark in CI fails if search exceeds its budget.

## Context engine

The reader builds a **context set**: an ordered list of passages, each a verse,
verse range, chapter, book, or several books, plus the sources to include:
translations, Hebrew and Greek words, Strong's definitions, cross-references, and
commentaries. Sources are chosen once for the whole set and can be overridden for
any passage.

- Passages are typed naturally ("Luke 2:14", "Rom 8:28-29", "Hab 3"), added from the
  reader (this verse, this chapter, a selection), reordered, and removed.
- The token meter shows each passage's share and the total against the model's window.
- **Show exactly what is sent** opens the full text that will go to the model, with a
  copy button. Nothing is hidden.
- Sets can be saved, named, and reused. Each conversation keeps the set it was asked
  with, so reopening it shows exactly what the model saw.
- The text is deterministic and clearly delimited:

```
<passage ref="Luke 2:14">
<text translation="KJV">Glory to God in the highest, and on earth peace, good will toward men.</text>
<text translation="DRA" ref="Luke 2:14">Glory to God in the highest; and on earth peace to men of good will.</text>
<notes source="Matthew Henry" ref="Luke 2:8-20">…</notes>
</passage>
```

  A note covering several attached passages appears once. Strong's definitions are
  listed once for the whole set.

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
