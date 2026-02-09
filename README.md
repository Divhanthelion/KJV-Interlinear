KJV-Interlinear

A KJV Bible study app with Hebrew and Greek interlinear support, built in Rust with [egui](https://github.com/emilk/egui).

KJV-Interlinear extends a fast KJV reader with original language texts, Strong's concordance, lexicon lookups, bookmarks, reading history, and a polished theme system — all in a native desktop app.

## Features

- **KJV Reading** — Chapter-by-chapter navigation across all 66 books
- **Hebrew & Greek Interlinear** — Word-by-word original language text aligned with the KJV, parsed from STEP Bible data (TAHOT, TAGNT)
- **Strong's Concordance** — Every Hebrew and Greek word linked to its Strong's number with occurrence counts and cross-references
- **Lexicon** — Built-in Hebrew (TBESH) and Greek (TBESG) dictionary entries with definitions, transliterations, and morphology
- **Display Modes** — KJV only, parallel, interlinear, or original-language-only views
- **Search** — Full-text search with scope filtering (all, current book, Old Testament, New Testament)
- **Bookmarks & History** — Save verses and track your reading with persistent storage
- **Theming** — Dark and light modes with semantic colors for Hebrew, Greek, red-letter text, and Strong's links
- **Settings** — Configurable font sizes, transliteration display, morphology toggling, and more — all saved between sessions

## Building

Requires [Rust](https://rustup.rs/) (edition 2024).
```sh
cargo build --release
```

Run from the project root:
```sh
cargo run
```

The app expects these directories in the working directory:

| Directory | Contents |
|---|---|
| `old_testament/` | KJV Old Testament text files (one per book) |
| `new_testament/` | KJV New Testament text files (one per book) |
| `data/` | STEP Bible original language files (optional — the app runs without them) |

## Data Sources

- **KJV text**: [Project Gutenberg](https://www.gutenberg.org/) — plain text, `chapter:verse text` format
- **Hebrew OT**: TAHOT files from the [STEP Bible project](https://www.stepbible.org/) (Translators Amalgamated Hebrew OT)
- **Greek NT**: TAGNT files from the STEP Bible project (Translators Amalgamated Greek NT)
- **Lexicons**: TBESH (Hebrew) and TBESG (Greek) from the STEP Bible project

## Project Structure
```
src/
├── main.rs                  # App entry point
├── models.rs                # Bible, Verse, ExtendedBible, Strong's types
├── parsing.rs               # KJV text file parser
├── settings.rs              # Persistent settings, bookmarks, history
├── theme.rs                 # Semantic dark/light theme system
├── original_languages/
│   ├── mod.rs               # Module declarations
│   └── loader.rs            # STEP Bible TSV parser (Hebrew, Greek, lexicons)
└── ui/
    ├── mod.rs               # Module declarations
    ├── app.rs               # Main app state and update loop
    └── components.rs        # Reusable UI components
```

## See Also

For a minimal KJV-only reader without the original language features, see [Berean Lite](https://github.com/Divhanthelion/Bible-App).

## License

See [license.txt](license.txt).
