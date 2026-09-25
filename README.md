# KJV-Interlinear

A KJV Bible study app with Hebrew and Greek interlinear support, built in Rust with [egui](https://github.com/emilk/egui).

KJV-Interlinear pairs every verse of the King James Version with the Hebrew and Greek behind it, with Strong's numbers, lexicon lookups, search, bookmarks, reading history, and dark and light themes, all in a native desktop app.

![Interlinear view of Genesis 1: each Hebrew word with its transliteration, Strong's number, and English gloss](assets/screenshots/interlinear-genesis-1.jpg)

## Features

- **KJV text**: all 66 books in the 1769 standard text, including the Psalm titles
- **Hebrew & Greek interlinear**: every verse and Psalm title, word by word, with transliteration, Strong's number, morphology, and gloss
- **Four views**: KJV, Parallel (KJV beside the original), Interlinear, and Original only
- **Strong's search**: find every verse with a Strong's number (`H430`, `h430`, and `430` all work)
- **Lexicon**: click any Strong's number for its Hebrew (TBESH) or Greek (TBESG) dictionary entry
- **Red letter**: words of Christ in the Gospels, Acts, and Revelation, colored at the phrase level
- **Search**: live full-text search across all books, one book, or one testament; results highlight in the text, and `Caesar's` finds `Cæsar’s`
- **Bookmarks & history**: jump back to saved verses and recently read chapters
- **Themes and settings**: dark and light modes, four font sizes, separate Hebrew and Greek size offsets; everything is saved between sessions

## Following the text the KJV translated

The interlinear shows the original-language text the KJV translators worked from, not a modern critical text:

- **Greek** follows the Textus Receptus (Scrivener 1894). Words found only in modern editions are left out, TR readings replace differing words, and TR-only verses such as Acts 8:37 and 1 John 5:7 are included.
- **Hebrew** follows the Masoretic text with the Qere readings, as the KJV does. Words reconstructed from the Septuagint are left out.
- **Versification** matches the KJV, including Psalm titles (verse 0 in Hebrew), Malachi 4, and the New Testament verse-boundary differences.
- **Hebrew displays right to left**, including Hebrew quoted inside lexicon definitions.

## Keyboard shortcuts

| Keys | Action |
|---|---|
| ← / → | Previous / next chapter (crosses book boundaries) |
| Ctrl+F | Focus search |
| Esc | Clear search |
| Ctrl+B | Bookmark the selected verse |
| Ctrl+C | Copy the selected verse |
| Ctrl+Shift+C | Copy the whole chapter |

Click a verse number to select that verse, or type a number in the **Verse** box and press Enter.

## Building

Requires [Rust](https://rustup.rs/) (edition 2024).

```sh
cargo build --release
```

Run from the project root, so that `old_testament/`, `new_testament/`, and `data/` are found:

```sh
cargo run --release
```

Test:

```sh
cargo test --release
```

Besides unit tests, `tests/data_integrity.rs` checks the bundled data exactly as the app loads it. It covers canonical book, chapter, and verse counts, original-language words for every verse, Strong's-to-lexicon links, LORD/GOD rendering of the Hebrew divine name, and every red-letter quote.

| Directory | Contents |
|---|---|
| `old_testament/` | KJV Old Testament, one file per book, one `chapter:verse text` line per verse (Psalm titles are verse `0`) |
| `new_testament/` | KJV New Testament, same format |
| `data/` | STEP Bible original-language files and the red-letter map (optional: without them the app reads KJV only) |

Parsed original-language data is cached under your OS cache directory after the first load.

## Data Sources & Attribution

See [NOTICE](NOTICE) for full attribution text.

- **KJV text**: the 1769 standard text, public domain, via [CrossWire](https://crosswire.org/) / [eBible.org](https://ebible.org/find/details.php?id=eng-kjv)
- **Hebrew OT / Greek NT / lexicons**: [STEP Bible](https://www.STEPBible.org/) (TAHOT, TAGNT, TBESH, TBESG), [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). Source: [STEPBible-Data](https://github.com/STEPBible/STEPBible-Data)
- **Red-letter words of Christ**: [kjvstudy.org](https://github.com/kennethreitz/kjvstudy.org) by Kenneth Reitz (ISC License)
- **Fonts**: Noto Sans and Noto Sans Hebrew (SIL Open Font License)

## Project Structure

```
src/
├── main.rs                  # App entry point
├── lib.rs                   # Library root
├── models.rs                # Bible, Verse, ExtendedBible, Strong's types
├── parsing.rs               # KJV text file parser
├── paths.rs                 # Asset path resolution
├── red_letter.rs            # Words-of-Christ index
├── settings.rs              # Persistent settings, bookmarks, history
├── text.rs                  # Search folding and Hebrew right-to-left display
├── theme.rs                 # Semantic dark/light theme system
├── fonts.rs                 # Bundled Noto Hebrew/Greek fonts
├── original_languages/
│   ├── mod.rs
│   ├── loader.rs            # STEP Bible TSV parser (KJV versification, Textus Receptus)
│   └── cache.rs             # Binary cache for ExtendedBible
└── ui/
    ├── mod.rs
    ├── app.rs               # Main app state and update loop
    └── components.rs        # Reusable UI components
tests/
└── data_integrity.rs        # Checks over the bundled data
```

## License

Application source code is licensed under the [MIT License](LICENSE).

Bundled data remains under its own terms (public-domain KJV text, STEP Bible CC BY 4.0, and ISC for the red-letter map) as described in [NOTICE](NOTICE).
