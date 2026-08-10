# Action Plan: KJV-Interlinear Review Fixes

Context for whoever implements this: `KJV-Interlinear` is a Rust desktop app (egui/eframe 0.31, edition 2024, ~4,900 lines in `src/`). It compiles cleanly and all 30 unit tests pass. This plan comes from a full code review; every item below cites the exact location and current code. Work through the phases in order — Phase 1 and 2 are real bugs, Phase 3 is correctness polish, Phase 4 is maintainability.

Global rules:
- Make minimal, scoped edits. No drive-by refactors beyond what each item says.
- After each phase, run `cargo check --all-targets && cargo test && cargo clippy --all-targets` and confirm no new warnings.
- Run from the project root so `old_testament/`, `new_testament/`, `data/` are found: `cargo run --release`.

---

## Phase 1 — High-severity UI bugs

### 1.1 Global shortcuts fire while typing in text fields (`src/ui/app.rs:310-341`)

`handle_keyboard` acts on Ctrl+C, Ctrl+Shift+C, Ctrl+B, and Escape even when an egui `TextEdit` (search box, Strong's box, verse input) has focus. Consequence: pressing Ctrl+C to copy selected search text first copies the selection, then the handler immediately overwrites the clipboard with the current verse. Ctrl+B bookmarks while typing "b" with cmd held; Escape clears search state while focus is elsewhere.

Current code ends `handle_keyboard` with a `ctx.input(|i| { ... })` block containing the shortcut checks (Escape at ~321, Ctrl+B at ~327, Ctrl+C at ~332, Ctrl+Shift+C at ~337).

Fix: at the top of `handle_keyboard` (or the top of the `ctx.input` closure), early-return when a widget wants keyboard input:

```rust
fn handle_keyboard(&mut self, ctx: &Context) {
    if ctx.wants_keyboard_input() {
        return;
    }
    ctx.input(|i| {
        // ... existing checks unchanged ...
    });
}
```

Verify manually: type text in the search box, select it, Ctrl+C, paste elsewhere — the selected text (not the verse) must be on the clipboard. Arrow-key chapter navigation must still work when nothing is focused.

### 1.2 Infinite debounce/repaint loop after clearing search (`src/ui/app.rs:367-377` + `183-187`)

`perform_search` early-returns on an empty query *before* updating `last_search_query`:

```rust
fn perform_search(&mut self) {
    if self.search_query.is_empty() {
        self.search_results.clear();
        return;              // <-- last_search_query never updated
    }
    // ... later: self.last_search_query = self.search_query.clone();
}
```

The debounce block in `update` compares `self.search_query != self.last_search_query`; after the user clears the box, `"" != "foo"` stays true forever → timer resets every frame, `ctx.request_repaint()` every frame, empty `perform_search()` every 0.3 s. The app busy-repaints indefinitely.

Fix: record the query before the early return:

```rust
fn perform_search(&mut self) {
    self.last_search_query = self.search_query.clone();
    if self.search_query.is_empty() {
        self.search_results.clear();
        return;
    }
    // ... rest unchanged (remove the now-duplicated assignment below if present) ...
}
```

Verify: type a query, wait for results, clear the box. CPU/repaint activity must return to idle (check with a debug overlay or activity monitor — before the fix it spins).

---

## Phase 2 — Data safety

### 2.1 Atomic settings save + backup on corruption (`src/settings.rs:213-234`)

Current `save` truncates `settings.json` in place; a crash mid-write leaves a truncated file, and `load` then silently returns `Self::default()` — the user's bookmarks and history are gone with no backup and no message.

Fix `save` (settings.rs:225-234) to write-temp-then-rename:

```rust
pub fn save(&self) -> Result<(), String> {
    if let Some(path) = Self::settings_path() {
        let contents = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize settings: {}", e))?;
        let tmp_path = path.with_extension("json.tmp");
        fs::write(&tmp_path, contents)
            .map_err(|e| format!("Failed to write settings: {}", e))?;
        fs::rename(&tmp_path, &path)
            .map_err(|e| format!("Failed to replace settings: {}", e))?;
        Ok(())
    } else {
        Err("Could not determine settings path".to_string())
    }
}
```

Fix `load` (settings.rs:213-222) to preserve a corrupt file instead of silently discarding it:

```rust
pub fn load() -> Self {
    if let Some(path) = Self::settings_path() {
        if let Ok(contents) = fs::read_to_string(&path) {
            match serde_json::from_str(&contents) {
                Ok(settings) => return settings,
                Err(e) => {
                    eprintln!("Warning: corrupt settings.json ({}), backing up and resetting", e);
                    let _ = fs::rename(&path, path.with_extension("json.bak"));
                }
            }
        }
    }
    Self::default()
}
```

Verify: hand-truncate `settings.json`, launch — app starts with defaults and a `settings.json.bak` exists alongside. Add bookmarks, quit, relaunch — they persist.

### 2.2 Bound bincode cache deserialization (`src/original_languages/cache.rs:99`)

`bincode::deserialize_from` trusts length prefixes; a tampered `extended_bible_v1.bin` claiming a huge `Vec` length aborts the process or exhausts memory. Fix with a size cap (bincode 1.3 `Options` trait):

```rust
use bincode::Options;

const MAX_CACHE_BYTES: u64 = 512 * 1024 * 1024; // generous cap; real cache is far smaller

// in try_load_cached, replace line 99:
let file = File::open(&cache_path).ok()?;
match bincode::options()
    .with_limit(MAX_CACHE_BYTES)
    .deserialize_from(BufReader::new(file))
{
    // ... arms unchanged ...
}
```

Also: the corrupt-cache arm already deletes the cache and returns `None`, which is correct — a tampered cache just triggers a rebuild. Keep that.

Note (do not implement unless trivial): the cache fingerprint (`cache.rs:33-73`) is name+len+mtime-seconds; a same-size same-second edit goes undetected. Leave as-is for now — acceptable for a local cache.

### 2.3 Slice panic in `extract_greek_transliteration` (`src/original_languages/loader.rs:178-185`)

`text.find(')')` searches the whole string, not the region after `(`. For input like `foo) bar (baz`, `start + 1 > end` and `text[start + 1..end]` panics. Current TAGNT data doesn't trigger it; a malformed/updated STEP file would crash the app at startup.

Fix:

```rust
fn extract_greek_transliteration(text: &str) -> String {
    if let Some(start) = text.find('(') {
        if let Some(rel_end) = text[start + 1..].find(')') {
            return text[start + 1..start + 1 + rel_end].to_string();
        }
    }
    String::new()
}
```

Add a unit test: `extract_greek_transliteration("foo) bar (baz")` returns `""` without panicking, and the normal `"Βίβλος (Biblos)"` case still returns `"Biblos"`.

### 2.4 Byte/char confusion in `find_folded` (`src/red_letter.rs:140-143`, `181-184`, `189-192`)

`hay_folded.find(&needle_folded)` returns a **byte** offset, but `hay_map` has one entry per **char**, and `needle_folded.chars().count()` is a char count. It is only correct while the folded text is pure ASCII; one unrecognized multibyte char in a data file before the match point = index-out-of-bounds panic at startup.

Fix all three sites the same way — convert the byte offset to a char index:

```rust
if let Some(byte_pos) = hay_folded.find(&needle_folded) {
    let char_pos = hay_folded[..byte_pos].chars().count();
    let needle_chars = needle_folded.chars().count();
    let start = hay_map[char_pos].0;
    let end = hay_map[char_pos + needle_chars - 1].1;
    return Some((start, end));
}
```

Add a regression test with a multibyte char before the match (e.g. haystack containing `ʽ` or `°` before the needle) asserting no panic and a correct range.

### 2.5 Sanity-bound chapter numbers in the KJV parser (`src/parsing.rs:96`)

The parser does `while book.chapters.len() < chapter_num { push empty chapter }`. A corrupt line like `999999999:1 text` pushes ~1 billion empty `Chapter` structs → hang/OOM.

Fix: before the loop, skip lines with an implausible chapter (no Bible book exceeds 150 chapters):

```rust
if chapter_num == 0 || chapter_num > 200 {
    continue;
}
```

Also bound `verse_num` similarly (`> 200 → continue`) for the same reason.

---

## Phase 3 — Correctness polish

### 3.1 Search scope: persist + re-run (`src/ui/app.rs:915-939`)

The four `selectable_value(&mut self.settings.search_scope, ...)` calls mutate settings without `mark_dirty()` (every other settings control calls it), so scope reverts on restart. Changing scope also leaves stale results from the old scope until the query text changes.

Fix: after the scope selector block, detect a change and handle both:

```rust
let prev_scope = self.settings.search_scope;
// ... existing selectable_value calls ...
if self.settings.search_scope != prev_scope {
    self.settings.mark_dirty();
    self.perform_search();
}
```

(Requires `SearchScope` to be `PartialEq` + `Copy`/`Clone` — add derives if missing.)

### 3.2 Clamp stale chapter state; don't record failed loads (`src/ui/app.rs:77`, `154-181`)

Startup does `settings.last_chapter.max(1)` with no upper bound, and `update_chapter_display` writes a history entry + persists position even when `get_chapter` returns `None` (the `else` branch sets `chapter_text = "Chapter not found"` but execution falls through to `update_position`/`add_history`). One out-of-range state permanently pollutes history with entries that reproduce the broken state.

Fix in `update_chapter_display`:

```rust
fn update_chapter_display(&mut self) {
    self.chapter_text.clear();
    self.current_chapter_verses.clear();

    let Some(chapter) = self.bible.get_chapter(&self.selected_book, self.selected_chapter) else {
        self.chapter_text = "Chapter not found".to_string();
        return; // do NOT update position or history for a failed load
    };
    self.current_chapter_verses = chapter.verses.clone();
    for verse in &chapter.verses {
        self.chapter_text
            .push_str(&format!("{} {}\n\n", verse.verse_number, verse.text));
    }

    self.settings.update_position(&self.selected_book, self.selected_chapter, self.selected_verse);
    self.settings.add_history(self.selected_book.clone(), self.selected_chapter);
    self.settings.mark_dirty();
}
```

And at startup (app.rs:77 area), clamp `selected_chapter` to the book's actual chapter count after loading the Bible: look up `bible.get_book(&selected_book)` and clamp to `book.chapters.len() as u32`, defaulting to 1 if the book is missing.

### 3.3 Book/chapter combo boxes leave stale state (`src/ui/app.rs:451-464`, `475-489`)

Picking a new book changes `selected_book` but not `selected_chapter`; the old chapter text stays on screen, and if the old chapter number exceeds the new book's chapter count, "Go" produces "Chapter not found" (+ the history pollution fixed in 3.2).

Fix: when the book combo selection changes, reset `self.selected_chapter = 1` and call `self.update_chapter_display()`. When the chapter combo changes, clamp to the selected book's chapter count and call `update_chapter_display()`.

### 3.4 Byte-slice panics on non-ASCII text

- `src/ui/app.rs:973-975`: `&result.text[..60]` panics if byte 60 falls inside a multibyte char. Replace with a char-safe truncation: `let preview: String = result.text.chars().take(60).collect();`
- `src/ui/components.rs:75-115` (`render_highlighted_text`): offsets computed on `text.to_lowercase()` are used to slice the original `text`; byte lengths can differ (e.g. `İ`). Compute match positions on the original text using `(start, start + matched.len())` from `str::match_indices` on the lowercased copy only when ASCII-safe, or simpler: since the query is matched case-insensitively, iterate `text.match_indices(&query)` after confirming both are ASCII, and fall back to plain rendering otherwise. Minimal acceptable fix: guard with `if text.is_ascii() && query.is_ascii() { ...highlight... } else { ...plain label... }`.

### 3.5 Greek verses get the Hebrew font-size offset (`src/ui/app.rs:817-822`, `843-847`)

`OriginalOnly` mode applies `hebrew_font_size_offset` unconditionally. Apply `greek_font_size_offset` when `verse.language == OriginalLanguage::Greek` (match the pattern already used in `components.rs:473-477`).

### 3.6 Validate verse-number input (`src/ui/app.rs:530-536`)

Entering `999` for a 30-verse chapter sets `selected_verse = 999`; copy-verse then silently no-ops and `toggle_bookmark` bookmarks a nonexistent verse. Clamp the parsed value to the current chapter's verse count (`current_chapter_verses.len() as u32`), min 1, before assigning.

### 3.7 `main` returns success with no window on missing data (`src/main.rs:11-32`)

When `old_testament/`/`new_testament/` are missing or loading fails, `main` prints to stderr and returns `Ok(())` — a GUI user double-clicking gets nothing, and scripts see exit code 0. Change the failure path to return `Err(...)` (or `std::process::exit(1)`) after the error message.

---

## Phase 4 — Maintainability (do after Phases 1-3 are verified)

Order these by value; each is independent.

1. **Consolidate settings defaults** (`src/settings.rs:133-157` vs `172-201`). `impl Default` and the `#[serde(default)]` annotations have drifted: `hebrew_font_size_offset` is `4.0` in `Default` but `0.0` when the field is missing from an existing file (Greek: `2.0` vs `0.0`). Use `#[serde(default = "default_hebrew_offset")]`-style functions that `Default` also calls, so there is one source of truth.

2. **Hoist `get_book_name_mapping()`** (`src/original_languages/loader.rs:94`). It builds a 66-entry `HashMap` per parsed TSV line (~450k times) — likely the dominant cold-parse cost. Make it a `static` via `std::sync::LazyLock` and have `parse_reference` use `&'static` map.

3. **Run `cargo clippy --fix --all-targets`** for the ~35 style warnings (mostly `collapsible_if`, which edition-2024 let-chains fix naturally), then hand-check the diff. Also fix the 4 `derivable_impls` (`models.rs:161`, `settings.rs:29,66,83`) manually if `--fix` doesn't.

4. **Remove dead/unused surface**:
   - `BibleApp::chapter_text` (`app.rs:33`, built in `update_chapter_display`, never rendered — the UI renders `current_chapter_verses`).
   - `Settings::lexicon_view`, `Settings::auto_load_original`, and the `LexiconView` enum (`settings.rs:56-81`, `146-148`) — persisted but read nowhere. Removing them from the struct is backwards-compatible only with `#[serde(default)]` on the struct; keep unknown fields tolerated (serde ignores them by default) so old files still load.
   - `components::icon_button` (`components.rs:213-218`, already `#[allow(dead_code)]`).
   - Dead match arms in `loader.rs:393` (`tag_name` is computed after `trim_start_matches('/')`, so the `"/b"`, `"/i"`, `"/ref"` arms are unreachable).

5. **Unify KJV verse rendering** (`src/ui/components.rs:21-62`, `415-452`, `541-583`). The red-letter-segmentation + search-highlight + plain-fallback block is triplicated across `render_verse`, `render_verse_parallel`, `render_verse_interlinear`. Extract one `render_kjv_text(ui, verse, query, theme, settings)` helper and call it from all three.

6. **Break up `BibleApp::update`** (`src/ui/app.rs:345-1168`, ~820 lines). Extract `render_top_panel`, `render_sidebar`, `render_chapter_view`, `render_search_panels`, `render_settings_window` methods. This also eliminates most per-frame `.clone()` borrow workarounds (`search_results.clone()` at :967, `strongs_results.clone()` at :1064, `bookmarks.clone()` at :629, `history.clone()` at :666, per-verse clones at :773). Do this last — it touches the most code and is purely structural; keep behavior identical.

7. **Dedup small repeated blocks** (optional, low value): the search-panel and Strong's-panel header blocks (`app.rs:875-899` vs `997-1022`, including the `+30.0`/`.min(400.0)`/`.max(60.0)` magic numbers), the five copies of the `hline` divider snippet, and the two divergent attribution texts (`components.rs:366-377` vs `app.rs:1127-1157` — make them share one source).

8. **Fix `settings_panel` double instantiation** (`app.rs:688` vs `1110`). It's rendered in both the sidebar tab and the Settings window with identical egui IDs (`"font_size_setting"`, `"display_mode_setting"`), so when both are visible the combo boxes interfere. Salt IDs per-instance (e.g. pass an `id_source` suffix) or give the panel a single home.

---

## Out of scope (documented, not fixed)

- Cache fingerprint weakness (name+len+mtime-seconds) — acceptable for a local cache.
- `bincode` 1.x is the legacy line; no known open advisory at these versions. Consider `cargo audit` in CI as a follow-up; migrating to bincode 2 is optional.
- Silent parse-failure policy in the loaders (skipped lines produce no diagnostics). If desired later, add per-file skip counters logged at load time.
- `paths.rs:10-12` using the CWD as first asset root — harmless for normal use; revisit only if shipping a bundled app.
- Aramaic portions of Daniel/Ezra labeled Hebrew (`loader.rs:260`; `OriginalLanguage::Aramaic` at `models.rs:200` is never constructed) — a data-accuracy nicety, not a bug affecting function.

## Final verification checklist

- `cargo check --all-targets` — clean.
- `cargo test` — all tests pass, including the new regression tests from 2.3 and 2.4.
- `cargo clippy --all-targets` — no new warnings (ideally zero after Phase 4.3).
- Manual smoke test (`cargo run --release`): navigate books/chapters with arrows and combos; search with each scope; copy selected search text with Ctrl+C; bookmark with Ctrl+B; clear search and confirm CPU goes idle; corrupt `settings.json` and confirm backup + clean start; delete the bincode cache and confirm rebuild.
