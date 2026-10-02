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
| Commentary | Haydock (1859) | volunteer USFM transcription (`cmahte/ENG-B-Haydock1883-pd-PSFM`) | Douay-Rheims | 1859 text public domain; *transcription's terms unconfirmed* |
| Commentary | Tyndale Open Study Notes | `tyndale_open-studynotes.zip` from tyndaleopenresources.com | English (NLT wording) | CC BY-SA 4.0 |
| Commentary | Wesley's Notes | CrossWire `Wesley` | KJV | Public domain |
| Commentary | Keil & Delitzsch | CrossWire `KD` | KJV | Public domain |
| Commentary | John Gill | SermonIndex SWORD module `sigill` (keeps the Hebrew; HelloAO's copy strips it) | KJV | Public domain |
| Commentary | Jamieson-Fausset-Brown (1871, unabridged) | CrossWire `JFB` | KJV | Public domain |
| Commentary | Church Fathers | NPNF/ANF/Oxford translations (tertullian.org, Writings-Database), keyed by homily headings; filtered HCF index for the rest | verse ranges | Public domain translations only; no machine translations, no modern copyrighted excerpts |
| Cross-references | Treasury of Scripture Knowledge | CrossWire `TSK` | KJV | Public domain |
| Cross-references | OpenBible.info | openbible.info | KJV | CC BY |

## Data in the repository

```
data/library/
  sources.toml                every upstream file: URL, size, SHA-256, date, licence
  bibles/<id>/meta.toml       name, abbreviation, year, description, licence, attribution, versification
  bibles/<id>/<BOOK>.usfm     the source USFM, cleaned (see below), one file per book
  commentaries/<id>/meta.toml
  commentaries/<id>/<BOOK>.jsonl   one note per line: {"from":"3:16","to":"3:18","body":"…"}
  crossrefs/<id>.tsv
  versification/              mappings from each numbering scheme to the KJV's
```

**Bibles are stored as cleaned USFM.** USFM is the standard the sources use, and it
keeps paragraphs, poetry lines, section headings, Psalm titles, footnotes,
supplied words (`\add`), the divine name (`\nd`), and the words of Jesus (`\wj`).
Cleaning removes only things that are not the translation's text: eBible's
automatic Strong's tags (`\w word|strong="…"\w*` becomes `word`), figures, and
publishing metadata. The cleaning rule is simple enough to audit, and the plain
text of every verse is checked against eBible's separately produced VPL edition.

**Commentary notes** use a small, closed markup in `body`: paragraphs, italics,
bold, Hebrew and Greek spans (with language), and Scripture references
(`<ref to="JHN 3:16">John 3:16</ref>`). The importer turns each source format
(OSIS, ThML, JSON, XML) into this one markup; anything it doesn't recognise is an
import error, never silently dropped.

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
a text shows up in review as a readable diff.

## In the app

- **Library archive.** `build.rs` packs `data/library/` into one archive embedded in
  the app: a table of contents plus one zstd frame per work and book. Nothing is
  decompressed until it is read, and recently used books stay in a small cache, so
  memory use stays low on phones however many works ship.
- **Catalogue.** Every work with its name, kind, description, coverage, licence,
  and attribution. An About page lists them all.
- **References.** The app speaks KJV numbering. Each work declares its own numbering,
  and the versification tables map between them. Parallel reading, commentary lookup,
  and context building all go through that one mapping.
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
- **Versification.** Every mapped reference exists in both schemes. Known hard cases,
  such as Psalm titles, Malachi 4 and Joel 3 in Hebrew numbering, and the Vulgate
  Psalms, have explicit tests.
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
