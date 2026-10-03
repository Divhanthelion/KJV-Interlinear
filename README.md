# Scriptorium

A library for reading and studying the Bible: 44 English translations, commentaries from the Church Fathers to the Reformation and after, two collections of cross-references, and the Hebrew and Greek behind the King James Version, all on your device and all free. For Windows, macOS, Linux, iPhone and iPad, and Android. Built with Rust and [Tauri](https://tauri.app/).

![Psalm 23 in four columns: the KJV, its Hebrew, the World English Bible, and the Douay-Rheims, whose Psalm 22:1 sits beside the KJV's title and first verse](assets/screenshots/parallel-psalm-23.jpg)

<img src="assets/screenshots/phone-parallel-john-3.jpg" alt="John 3 on a phone, each verse in the KJV, the Greek, the WEB, and the Douay-Rheims, one under another" width="260" align="right">

## What's in it

- **44 English translations**, from Wycliffe's and Tyndale's through the Geneva, the KJV, the Douay-Rheims, and Brenton's Septuagint to the World English Bible and the Berean Standard Bible, each checked character for character against its source
- **Commentaries**: Matthew Henry, John Gill, Jamieson-Fausset-Brown, Keil & Delitzsch, Wesley's Notes, Aquinas's Catena Aurea, the Tyndale Open Study Notes with their profiles and articles, and the Fathers: Chrysostom's homilies and Augustine's expositions and tractates, each placed on the passage it expounds
- **Cross-references**: the Treasury of Scripture Knowledge and OpenBible.info's, with each place's words in the translation you're reading
- **The Hebrew, Aramaic, and Greek** behind the KJV, word by word, with transliteration, Strong's number, grammar, and gloss, and the lexicon entry for any word

<br clear="right">

## Reading and studying

- **Parallel**: up to four translations side by side, or beside the Hebrew and Greek, verse by verse. Translations number some verses differently (the Psalms in the Douay-Rheims and the Septuagint, Malachi 4 in the Hebrew, the end of Romans in the WEB); every column is lined up through a verse alignment made from what the verses say, and numbered as that translation numbers it
- **Commentary and Cross-references panels** for the verse you select, in any translation
- **Search** the translation you're reading, or chosen translations and commentaries, or everything at once, with results grouped by source as they arrive; `Caesar's` finds `Cæsar’s`
- **Red letters**, the divine name in small capitals, poetry set in lines, and each translation's own notes
- **Bookmarks and history**, light and dark themes, adjustable text size and font

![Searching everything for Melchizedek: 422 results in 40 sources, grouped by translation and commentary, the WEB's Genesis 14 open with the name marked](assets/screenshots/search-everything.jpg)

## The study assistant (optional)

Ask about exactly what you choose: a verse from Luke, two from Romans, and a chapter of Micah, say, in any of the translations, with notes from any of the commentaries, cross-references with their words, and the Hebrew and Greek if you like. It shows how much that is against the model's window, shows exactly what is sent (and lets you copy it for any assistant), saves contexts to use again, and remembers what each conversation was asked with.

Use your own AI: a server on your network (vLLM, Ollama, LM Studio, llama.cpp) or your API key for Anthropic, OpenAI, Gemini, DeepSeek, OpenRouter, or Groq. Keys stay in the system keychain; your questions go only to the provider you set up.

![John 3:16 with the Commentary panel open: Matthew Henry, the Tyndale Open Study Notes, Chrysostom, and Augustine](assets/screenshots/commentary-john-3-16.jpg)

## Free and private

No ads, no account, nothing to buy, ever. Everything is on the device and works offline; nothing is collected ([privacy policy](PRIVACY.md)).

## Keyboard shortcuts (desktop)

| Keys | Action |
|---|---|
| ← / → | Previous / next chapter (crosses book boundaries) |
| Ctrl+F | Search |
| Ctrl+J | Ask the study assistant |
| Esc | Deselect, close the panel, or clear search highlights |
| Ctrl+B | Bookmark the selected verse |
| Ctrl+C / Ctrl+Shift+C | Copy the selected verse / the whole chapter |

Click or tap a verse to select it.

## Building

Requires [Rust](https://rustup.rs/) and the [Tauri CLI](https://tauri.app/start/prerequisites/) (`cargo install tauri-cli --version "^2"`). On Linux, also install the [WebKitGTK prerequisites](https://tauri.app/start/prerequisites/#linux).

```sh
cargo tauri dev          # run the app (from the app/ directory)
cargo tauri build        # installers for this platform
cargo test --release -p kjv-core -p kjv-ai   # text, library, search, assistant, and parallel tests
```

The build compiles the KJV, its Hebrew and Greek, and the whole library into compressed bundles embedded in the app, so it needs no data files or network at runtime.

To work on the interface in a browser with the real data:

```sh
cargo run --release -p kjv-devserver   # then open http://localhost:1420
```

The library (`data/library/`) is converted from pinned sources by `cargo run -p kjv-import -- build`, and `-- check` proves the conversion reproduces it byte for byte; see [docs/LIBRARY.md](docs/LIBRARY.md). Release builds for every platform run in GitHub Actions when a `v*` tag is pushed; see [docs/RELEASING.md](docs/RELEASING.md).

## Project structure

```
crates/core/      The KJV, Hebrew/Greek, search, parallel reading, the assistant's context, and the app's API
crates/library/   The library: translations, commentaries, cross-references, verse alignment, search index
crates/import/    Converts the pinned sources into data/library/, and writes the notices
crates/ai/        Study assistant: streaming client for OpenAI-compatible, Anthropic, and Gemini APIs
crates/devserver/ Browser preview server for UI work
app/              Tauri app: embeds the data and serves the UI
ui/               The interface: HTML, CSS, and JavaScript modules (no build step)
tests/ui/         Headless UI tests and a stand-in model server for them
data/             STEP Bible Hebrew/Greek, lexicons, the words of Christ, and data/library/
```

## Licences

The app's own code is free for anyone to use for anything: [MIT No Attribution](LICENSE). Everything it carries keeps its own terms: 31 of the 44 translations and nine of the commentaries are in the public domain; the rest are under Creative Commons licences (CC BY, BY-SA, BY-ND, and BY-NC-ND), each credited as it asks. [NOTICE](NOTICE) lists every work with its licence and credit, and [THIRD-PARTY-SOFTWARE.md](THIRD-PARTY-SOFTWARE.md) the open-source software the app is built from; the app shows both under Settings, Licences.
