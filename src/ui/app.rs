use eframe::egui::{self, Color32, ComboBox, Context, Key, RichText, ScrollArea, TextEdit, Ui};

use crate::models::{
    Bible, ExtendedBible, InterlinearVerse, SearchScope, Testament, Verse, VerseRef,
    normalize_strongs,
};
use crate::red_letter::RedLetterIndex;
use crate::settings::{DisplayMode, SettingsStore};
use crate::theme::{Theme, get_theme};
use crate::ui::components;

/// "1 verse", "2 verses"
fn count_label(n: usize, noun: &str) -> String {
    format!("{} {}{}", n, noun, if n == 1 { "" } else { "s" })
}

/// Frames to keep re-applying a scroll-to-verse while the layout settles
const SCROLL_FRAMES: u8 = 5;

/// Tab selection for sidebar
#[derive(Debug, Clone, PartialEq)]
enum SidebarTab {
    Bookmarks,
    History,
    Settings,
}

/// Main application state
pub struct BibleApp {
    bible: Bible,
    extended_bible: Option<ExtendedBible>,
    red_letter: Option<RedLetterIndex>,
    settings: SettingsStore,

    // Navigation state
    selected_book: String,
    selected_chapter: u32,
    selected_verse: u32,
    verse_input: String,

    // Display state
    current_chapter_verses: Vec<Verse>,
    /// Psalm superscription for the current chapter (verse 0)
    current_superscription: Option<Verse>,
    /// Verse to bring into view (0 = chapter title) and how many more frames to
    /// re-apply it, since the layout settles over the first frames after a jump
    scroll_to_verse: Option<(u32, u8)>,
    /// Height the search panels took last frame; the chapter gets the rest
    search_panels_height: f32,

    // Search state
    search_query: String,
    search_results: Vec<Verse>,
    last_search_query: String,
    /// Query being typed and the time at which it should be searched
    search_debounce: Option<(String, f64)>,

    // Strong's search state
    strongs_query: String,
    /// Normalized Strong's number of the last search (e.g. "H0430")
    strongs_key: Option<String>,
    strongs_results: Vec<VerseRef>,
    strongs_count: usize,

    // Lexicon popup state
    show_lexicon_popup: Option<String>,

    // UI state
    show_settings_window: bool,
    sidebar_tab: SidebarTab,
    navigate_to: Option<(String, u32, Option<u32>)>,

    // Clipboard
    clipboard: Option<arboard::Clipboard>,
    copy_feedback: Option<(String, f64)>,
}

impl BibleApp {
    pub fn new(
        bible: Bible,
        extended_bible: Option<ExtendedBible>,
        red_letter: Option<RedLetterIndex>,
    ) -> Self {
        let settings = SettingsStore::load();

        let selected_book = if bible.books.iter().any(|b| b.name == settings.last_book) {
            settings.last_book.clone()
        } else {
            bible
                .books
                .first()
                .map_or("Genesis".to_string(), |b| b.name.clone())
        };

        let selected_chapter = settings.last_chapter.max(1);
        let selected_verse = settings.last_verse.max(1);

        // Clamp chapter to the selected book's actual chapter count
        let selected_chapter = bible
            .books
            .iter()
            .find(|b| b.name == selected_book)
            .map(|b| selected_chapter.min(b.chapters.len() as u32).max(1))
            .unwrap_or(1);

        let mut app = Self {
            bible,
            extended_bible,
            red_letter,
            settings,
            selected_book,
            selected_chapter,
            selected_verse,
            verse_input: String::new(),
            current_chapter_verses: Vec::new(),
            current_superscription: None,
            scroll_to_verse: (selected_verse > 1).then_some((selected_verse, SCROLL_FRAMES)),
            search_panels_height: 120.0,
            search_query: String::new(),
            search_results: Vec::new(),
            last_search_query: String::new(),
            search_debounce: None,
            strongs_query: String::new(),
            strongs_key: None,
            strongs_results: Vec::new(),
            strongs_count: 0,
            show_lexicon_popup: None,
            show_settings_window: false,
            sidebar_tab: SidebarTab::Bookmarks,
            navigate_to: None,
            clipboard: arboard::Clipboard::new().ok(),
            copy_feedback: None,
        };

        app.update_chapter_display();
        app
    }

    /// Check if original language data is available
    fn has_original_languages(&self) -> bool {
        self.extended_bible
            .as_ref()
            .is_some_and(|e| e.is_loaded())
    }

    /// Get interlinear data for current verse
    fn get_current_interlinear(&self, verse_num: u32) -> Option<&InterlinearVerse> {
        self.extended_bible.as_ref()?.get_interlinear(
            &self.selected_book,
            self.selected_chapter,
            verse_num,
        )
    }

    /// Perform Strong's number search
    fn perform_strongs_search(&mut self) {
        if self.strongs_query.trim().is_empty() {
            self.strongs_key = None;
            self.strongs_results.clear();
            self.strongs_count = 0;
            return;
        }

        // "h430", "H430" and "H0430" all mean H0430; a bare number is Hebrew
        self.strongs_key = normalize_strongs(&self.strongs_query);
        self.strongs_results.clear();
        self.strongs_count = 0;

        if let (Some(ext), Some(key)) = (&self.extended_bible, &self.strongs_key) {
            self.strongs_count = ext.strongs_count(key);
            if let Some(refs) = ext.strongs_index.get_occurrences(key) {
                // Limit to first 100 results for performance
                self.strongs_results = refs.iter().take(100).cloned().collect();
            }
        }
    }

    fn update_chapter_display(&mut self) {
        self.current_chapter_verses.clear();
        self.current_superscription = None;

        let Some(chapter) = self
            .bible
            .get_chapter(&self.selected_book, self.selected_chapter)
        else {
            return; // do NOT update position or history for a failed load
        };

        self.current_chapter_verses = chapter.verses.clone();
        self.current_superscription = chapter.superscription.clone();

        // Update settings with current position (defer disk write)
        self.settings.update_position(
            &self.selected_book,
            self.selected_chapter,
            self.selected_verse,
        );
        self.settings
            .add_history(self.selected_book.clone(), self.selected_chapter);
        self.settings.mark_dirty();
    }

    fn perform_search(&mut self) {
        self.last_search_query = self.search_query.clone();
        self.search_debounce = None;
        if self.search_query.trim().is_empty() {
            self.search_results.clear();
            return;
        }

        let results = match self.settings.search_scope {
            SearchScope::All => self.bible.search(&self.search_query),
            SearchScope::CurrentBook => self
                .bible
                .search_in_book(&self.search_query, &self.selected_book),
            SearchScope::OldTestament => self
                .bible
                .search_in_testament(&self.search_query, &Testament::Old),
            SearchScope::NewTestament => self
                .bible
                .search_in_testament(&self.search_query, &Testament::New),
        };

        self.search_results = results.into_iter().cloned().collect();
    }

    fn go_to_previous_chapter(&mut self) {
        if self.selected_chapter > 1 {
            self.selected_chapter -= 1;
            self.selected_verse = 1;
            self.update_chapter_display();
        } else {
            // Go to previous book's last chapter
            let current_idx = self
                .bible
                .books
                .iter()
                .position(|b| b.name == self.selected_book);
            if let Some(idx) = current_idx
                && idx > 0 {
                    let prev_book = &self.bible.books[idx - 1];
                    self.selected_book = prev_book.name.clone();
                    self.selected_chapter = prev_book.chapters.len() as u32;
                    self.selected_verse = 1;
                    self.update_chapter_display();
                }
        }
    }

    fn go_to_next_chapter(&mut self) {
        let chapter_count = self.bible.chapter_count(&self.selected_book).unwrap_or(1) as u32;

        if self.selected_chapter < chapter_count {
            self.selected_chapter += 1;
            self.selected_verse = 1;
            self.update_chapter_display();
        } else {
            // Go to next book's first chapter
            let current_idx = self
                .bible
                .books
                .iter()
                .position(|b| b.name == self.selected_book);
            if let Some(idx) = current_idx
                && idx < self.bible.books.len() - 1 {
                    self.selected_book = self.bible.books[idx + 1].name.clone();
                    self.selected_chapter = 1;
                    self.selected_verse = 1;
                    self.update_chapter_display();
                }
        }
    }

    fn copy_to_clipboard(&mut self, text: &str) {
        if let Some(ref mut clipboard) = self.clipboard
            && clipboard.set_text(text.to_string()).is_ok() {
                self.copy_feedback = Some(("Copied!".to_string(), 2.0));
            }
    }

    fn copy_current_verse(&mut self) {
        if let Some(verse) = self
            .current_chapter_verses
            .iter()
            .find(|v| v.verse_number == self.selected_verse)
        {
            let text = format!(
                "{} {}:{} - {}",
                verse.book, verse.chapter, verse.verse_number, verse.text
            );
            self.copy_to_clipboard(&text);
        }
    }

    fn copy_current_chapter(&mut self) {
        let mut text = format!(
            "{} Chapter {}\n\n",
            self.selected_book, self.selected_chapter
        );
        if let Some(title) = &self.current_superscription {
            text.push_str(&format!("{}\n", title.text));
        }
        for verse in &self.current_chapter_verses {
            text.push_str(&format!("{} {}\n", verse.verse_number, verse.text));
        }
        self.copy_to_clipboard(&text);
    }

    fn toggle_bookmark(&mut self) {
        let book = self.selected_book.clone();
        let chapter = self.selected_chapter;
        let verse = self.selected_verse;

        if self.settings.is_bookmarked(&book, chapter, verse) {
            self.settings.remove_bookmark(&book, chapter, verse);
        } else {
            self.settings.add_bookmark(book, chapter, verse, None);
        }
        self.settings.mark_dirty();
    }

    fn apply_theme(&self, ctx: &Context) -> Theme {
        let theme = get_theme(self.settings.dark_mode);
        theme.apply(ctx);
        theme
    }

    fn handle_keyboard(&mut self, ctx: &Context) {
        if ctx.wants_keyboard_input() {
            return;
        }
        ctx.input(|i| {
            // Left arrow: previous chapter
            if i.key_pressed(Key::ArrowLeft) && !i.modifiers.any() {
                self.go_to_previous_chapter();
            }

            // Right arrow: next chapter
            if i.key_pressed(Key::ArrowRight) && !i.modifiers.any() {
                self.go_to_next_chapter();
            }

            // Ctrl+F: focus search (handled by egui focus)
            // Escape: clear search
            if i.key_pressed(Key::Escape) {
                self.search_query.clear();
                self.search_results.clear();
                self.last_search_query.clear();
                self.search_debounce = None;
            }

            // Ctrl+B: toggle bookmark
            if i.key_pressed(Key::B) && i.modifiers.command {
                self.toggle_bookmark();
            }

        });
    }

    /// Ctrl+C copies the selected verse, Ctrl+Shift+C the chapter.
    ///
    /// egui turns these shortcuts into `Event::Copy` (never a `Key::C` press), and uses the
    /// same event to copy selected label text, so this runs after the frame is drawn and
    /// only acts when egui didn't copy anything itself.
    fn handle_copy_shortcut(&mut self, ctx: &Context, was_typing: bool) {
        let (copy, shift) = ctx.input(|i| {
            (
                i.events.iter().any(|e| matches!(e, egui::Event::Copy)),
                i.modifiers.shift,
            )
        });
        if !copy || was_typing {
            return;
        }
        let egui_copied = ctx.output(|o| {
            o.commands
                .iter()
                .any(|c| matches!(c, egui::OutputCommand::CopyText(_)))
        });
        if egui_copied {
            return;
        }
        if shift {
            self.copy_current_chapter();
        } else {
            self.copy_current_verse();
        }
    }

    /// Jump the view to a verse in the current chapter (after the chapter is loaded).
    fn go_to_verse(&mut self, verse: u32) {
        let max_verse = self.current_chapter_verses.len() as u32;
        self.selected_verse = verse.clamp(1, max_verse.max(1));
        let target = if verse == 0 { 0 } else { self.selected_verse };
        self.scroll_to_verse = Some((target, SCROLL_FRAMES));
        self.settings.update_position(
            &self.selected_book,
            self.selected_chapter,
            self.selected_verse,
        );
        self.settings.mark_dirty();
    }

    /// Apply the verse box ("Verse: #") if it holds a number.
    fn apply_verse_input(&mut self) {
        if let Ok(v) = self.verse_input.trim().parse::<u32>() {
            self.go_to_verse(v);
        }
        self.verse_input.clear();
    }

    fn render_top_panel(&mut self, ctx: &Context, theme: &Theme) {
        egui::TopBottomPanel::top("top_panel")
            .frame(
                egui::Frame::new()
                    .fill(theme.bg_elevated)
                    .inner_margin(egui::Margin::symmetric(16, 12)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // App title with accent color
                    ui.label(
                        RichText::new("KJV Interlinear")
                            .size(22.0)
                            .strong()
                            .color(theme.text_primary),
                    );

                    ui.add_space(8.0);

                    // Subtle testament indicator
                    let testament_text = if self
                        .bible
                        .books
                        .iter()
                        .find(|b| b.name == self.selected_book)
                        .map(|b| &b.testament)
                        == Some(&crate::models::Testament::Old)
                    {
                        "Old Testament"
                    } else {
                        "New Testament"
                    };
                    ui.label(
                        RichText::new(testament_text)
                            .size(12.0)
                            .color(theme.text_muted),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Settings button with icon
                        if components::styled_icon_button(ui, "\u{2699}", "Settings", theme) {
                            self.show_settings_window = !self.show_settings_window;
                        }

                        // Copy feedback with success color
                        if let Some((ref msg, _)) = self.copy_feedback {
                            ui.label(RichText::new(msg).color(theme.success));
                        }
                    });
                });

                ui.add_space(8.0);

                // Custom separator with theme color
                let rect = ui.available_rect_before_wrap();
                ui.painter().hline(
                    rect.x_range(),
                    rect.top(),
                    egui::Stroke::new(1.0_f32, theme.divider),
                );

                ui.add_space(8.0);

                // Navigation bar with improved styling
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;

                    // Book selector with custom width
                    let prev_book = self.selected_book.clone();
                    ComboBox::from_id_salt("book_select")
                        .selected_text(RichText::new(&self.selected_book).color(theme.text_primary))
                        .width(160.0)
                        .show_ui(ui, |ui| {
                            for book in &self.bible.books {
                                let is_selected = self.selected_book == book.name;
                                let text = RichText::new(&book.name).color(if is_selected {
                                    theme.primary
                                } else {
                                    theme.text_primary
                                });
                                ui.selectable_value(
                                    &mut self.selected_book,
                                    book.name.clone(),
                                    text,
                                );
                            }
                        });
                    if self.selected_book != prev_book {
                        self.selected_chapter = 1;
                        self.selected_verse = 1;
                        self.update_chapter_display();
                    }

                    // Chapter selector
                    if let Some(book) = self
                        .bible
                        .books
                        .iter()
                        .find(|b| b.name == self.selected_book)
                    {
                        let chapter_count = book.chapters.len() as u32;

                        let prev_chapter = self.selected_chapter;
                        ComboBox::from_id_salt("chapter_select")
                            .selected_text(
                                RichText::new(format!("Chapter {}", self.selected_chapter))
                                    .color(theme.text_primary),
                            )
                            .width(100.0)
                            .show_ui(ui, |ui| {
                                for chapter_num in 1..=chapter_count {
                                    ui.selectable_value(
                                        &mut self.selected_chapter,
                                        chapter_num,
                                        chapter_num.to_string(),
                                    );
                                }
                            });
                        if self.selected_chapter != prev_chapter {
                            self.selected_chapter =
                                self.selected_chapter.clamp(1, chapter_count.max(1));
                            self.selected_verse = 1;
                            self.update_chapter_display();
                        }
                    }

                    // Navigation buttons
                    if components::nav_button(
                        ui,
                        "\u{25C0}",
                        "Previous Chapter (Left Arrow)",
                        theme,
                    ) {
                        self.go_to_previous_chapter();
                    }
                    if components::nav_button(ui, "\u{25B6}", "Next Chapter (Right Arrow)", theme)
                    {
                        self.go_to_next_chapter();
                    }

                    // Verse input with label
                    ui.add_space(8.0);
                    ui.label(RichText::new("Verse:").color(theme.text_muted).size(13.0));
                    let verse_response = ui.add(
                        TextEdit::singleline(&mut self.verse_input)
                            .desired_width(45.0)
                            .hint_text("#")
                            .font(egui::TextStyle::Body),
                    );
                    let verse_entered =
                        verse_response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));

                    // Go button jumps to the verse typed in the box
                    let go_button =
                        egui::Button::new(RichText::new("Go").color(theme.primary).size(13.0))
                            .fill(Color32::from_rgba_unmultiplied(
                                theme.primary.r(),
                                theme.primary.g(),
                                theme.primary.b(),
                                20,
                            ))
                            .corner_radius(egui::CornerRadius::same(6));
                    if ui
                        .add(go_button)
                        .on_hover_text("Go to the verse number entered")
                        .clicked()
                        || verse_entered
                    {
                        self.apply_verse_input();
                    }

                    ui.add_space(8.0);

                    // Action buttons with theme styling
                    let is_bookmarked = self.settings.is_bookmarked(
                        &self.selected_book,
                        self.selected_chapter,
                        self.selected_verse,
                    );
                    let bookmark_icon = if is_bookmarked {
                        "\u{2605}"
                    } else {
                        "\u{2606}"
                    };
                    if components::action_button(
                        ui,
                        bookmark_icon,
                        "Toggle Bookmark (Ctrl+B)",
                        is_bookmarked,
                        theme,
                    ) {
                        self.toggle_bookmark();
                    }

                    if components::action_button(
                        ui,
                        "\u{1F4CB}",
                        "Copy Verse (Ctrl+C)",
                        false,
                        theme,
                    ) {
                        self.copy_current_verse();
                    }
                    if components::action_button(
                        ui,
                        "\u{1F4C4}",
                        "Copy Chapter (Ctrl+Shift+C)",
                        false,
                        theme,
                    ) {
                        self.copy_current_chapter();
                    }
                });
            });
    }

    fn render_sidebar(&mut self, ctx: &Context, theme: &Theme) {
        egui::SidePanel::left("sidebar")
            .default_width(220.0)
            .frame(
                egui::Frame::new()
                    .fill(theme.bg_panel)
                    .inner_margin(egui::Margin::same(12))
                    .stroke(egui::Stroke::new(1.0_f32, theme.border)),
            )
            .show(ctx, |ui| {
                // Tab bar with styled buttons
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for (tab, label) in [
                        (SidebarTab::Bookmarks, "Bookmarks"),
                        (SidebarTab::History, "History"),
                        (SidebarTab::Settings, "Settings"),
                    ] {
                        let is_selected = self.sidebar_tab == tab;
                        let text = RichText::new(label).size(12.0).color(if is_selected {
                            theme.primary
                        } else {
                            theme.text_muted
                        });
                        if ui.selectable_label(is_selected, text).clicked() {
                            self.sidebar_tab = tab;
                        }
                    }
                });

                ui.add_space(8.0);
                let rect = ui.available_rect_before_wrap();
                ui.painter().hline(
                    rect.x_range(),
                    rect.top(),
                    egui::Stroke::new(1.0_f32, theme.divider),
                );
                ui.add_space(8.0);

                match self.sidebar_tab {
                    SidebarTab::Bookmarks => {
                        ScrollArea::vertical().show(ui, |ui| {
                            if self.settings.bookmarks.is_empty() {
                                ui.label("No bookmarks yet");
                            } else {
                                let mut to_remove: Option<(String, u32, u32)> = None;

                                for i in 0..self.settings.bookmarks.len() {
                                    let bookmark = &self.settings.bookmarks[i];
                                    let (clicked, delete) = components::render_bookmark(
                                        ui,
                                        bookmark,
                                        &self.settings,
                                        theme,
                                    );
                                    if clicked {
                                        self.navigate_to = Some((
                                            bookmark.book.clone(),
                                            bookmark.chapter,
                                            Some(bookmark.verse),
                                        ));
                                    }
                                    if delete {
                                        to_remove = Some((
                                            bookmark.book.clone(),
                                            bookmark.chapter,
                                            bookmark.verse,
                                        ));
                                    }
                                }

                                if let Some((book, chapter, verse)) = to_remove {
                                    self.settings.remove_bookmark(&book, chapter, verse);
                                    self.settings.mark_dirty();
                                }
                            }
                        });
                    }
                    SidebarTab::History => {
                        ScrollArea::vertical().show(ui, |ui| {
                            if self.settings.history.is_empty() {
                                ui.label("No history yet");
                            } else {
                                for i in 0..self.settings.history.len() {
                                    let entry = &self.settings.history[i];
                                    if components::render_history_entry(
                                        ui,
                                        entry,
                                        &self.settings,
                                        theme,
                                    ) {
                                        self.navigate_to =
                                            Some((entry.book.clone(), entry.chapter, None));
                                    }
                                }

                                ui.separator();
                                if ui.button("Clear History").clicked() {
                                    self.settings.clear_history();
                                    self.settings.mark_dirty();
                                }
                            }
                        });
                    }
                    SidebarTab::Settings => {
                        if components::settings_panel(ui, &mut self.settings, "sidebar") {
                            self.settings.mark_dirty();
                        }
                    }
                }
            });
    }

    fn render_chapter_view(&mut self, ui: &mut Ui, theme: &Theme) {
        // Chapter heading with improved typography
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(&self.selected_book)
                    .size(26.0)
                    .strong()
                    .color(theme.text_primary),
            );
            ui.label(
                RichText::new(format!("Chapter {}", self.selected_chapter))
                    .size(26.0)
                    .color(theme.text_secondary),
            );
        });
        ui.add_space(8.0);
        let rect = ui.available_rect_before_wrap();
        ui.painter().hline(
            rect.x_range(),
            rect.top(),
            egui::Stroke::new(1.0_f32, theme.divider),
        );
        ui.add_space(12.0);

        // Display mode selector (only show if original languages available)
        if self.has_original_languages() {
            ui.horizontal(|ui| {
                ui.label("View:");
                for mode in DisplayMode::all() {
                    if ui
                        .selectable_label(self.settings.display_mode == *mode, mode.label())
                        .clicked()
                    {
                        self.settings.display_mode = *mode;
                        self.settings.mark_dirty();
                    }
                }
            });
            ui.add_space(4.0);
        }

        // Leave room for the search panels as drawn last frame (they only take
        // space for results when there are results)
        let total_reserved = self.search_panels_height + 20.0;

        // Chapter content
        let available_height = ui.available_height() - total_reserved;
        let mut clicked_strongs: Option<String> = None;
        let mut clicked_verse: Option<u32> = None;

        ScrollArea::vertical()
            .max_height(available_height.max(200.0))
            .id_salt("chapter_scroll")
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width());
                let highlight_terms: Vec<String> = if !self.search_query.trim().is_empty() {
                    vec![self.search_query.trim().to_string()]
                } else {
                    vec![]
                };

                // Psalm superscription first (verse 0), then the verses
                let verses: Vec<Verse> = self
                    .current_superscription
                    .iter()
                    .chain(self.current_chapter_verses.iter())
                    .cloned()
                    .collect();

                let mut scrolled = false;
                for verse in &verses {
                    // Placeholder so the selection highlight is painted under the verse
                    let background = ui.painter().add(egui::Shape::Noop);
                    let block = ui.vertical(|ui| self.render_one_verse(ui, verse, &highlight_terms, theme));
                    let rect = block.response.rect;

                    if verse.verse_number > 0 && verse.verse_number == self.selected_verse {
                        // Faint tint plus an accent bar: the verse that copy/bookmark act on
                        let area = rect
                            .with_max_y((rect.max.y - 8.0).max(rect.min.y))
                            .expand2(egui::vec2(6.0, 2.0));
                        let bar = egui::Rect::from_min_max(
                            area.left_top(),
                            egui::pos2(area.left() + 3.0, area.bottom()),
                        );
                        ui.painter().set(
                            background,
                            egui::Shape::Vec(vec![
                                egui::Shape::rect_filled(
                                    area,
                                    egui::CornerRadius::same(4),
                                    theme.selection.gamma_multiply(0.25),
                                ),
                                egui::Shape::rect_filled(bar, egui::CornerRadius::same(2), theme.primary),
                            ]),
                        );
                    }
                    if let Some((target, frames)) = self.scroll_to_verse
                        && target == verse.verse_number
                    {
                        ui.scroll_to_rect(rect, Some(egui::Align::TOP));
                        scrolled = true;
                        self.scroll_to_verse = frames.checked_sub(1).map(|f| (target, f));
                        ui.ctx().request_repaint();
                    }

                    let interaction = block.inner;
                    if interaction.selected {
                        clicked_verse = Some(verse.verse_number);
                    }
                    if interaction.strongs.is_some() {
                        clicked_strongs = interaction.strongs;
                    }
                }
                // Target not in this chapter (e.g. no title): don't keep trying
                if !scrolled {
                    self.scroll_to_verse = None;
                }
            });

        if let Some(verse) = clicked_verse {
            self.selected_verse = verse;
            self.settings
                .update_position(&self.selected_book, self.selected_chapter, verse);
            self.settings.mark_dirty();
        }

        // Handle Strong's number clicks
        if let Some(strongs) = clicked_strongs {
            self.show_lexicon_popup = Some(strongs);
        }

        ui.separator();
    }

    /// Draw one verse (or Psalm title, verse 0) in the current display mode.
    fn render_one_verse(
        &self,
        ui: &mut Ui,
        verse: &Verse,
        highlight_terms: &[String],
        theme: &Theme,
    ) -> components::VerseInteraction {
        let red_letter = self.red_letter.as_ref();
        let interlinear = self.get_current_interlinear(verse.verse_number);
        match self.settings.display_mode {
            DisplayMode::KjvOnly => components::render_verse(
                ui,
                verse,
                &self.settings,
                highlight_terms,
                theme,
                red_letter,
            ),
            DisplayMode::Parallel => components::render_verse_parallel(
                ui,
                verse,
                interlinear,
                &self.settings,
                highlight_terms,
                theme,
                red_letter,
            ),
            DisplayMode::Interlinear => components::render_verse_interlinear(
                ui,
                verse,
                interlinear,
                &self.settings,
                theme,
                red_letter,
            ),
            DisplayMode::OriginalOnly => match interlinear {
                Some(orig) => {
                    let interaction = components::VerseInteraction {
                        selected: components::render_original_paragraph(
                            ui,
                            orig,
                            Some(verse),
                            &self.settings,
                            theme,
                        ),
                        strongs: None,
                    };
                    ui.add_space(10.0);
                    interaction
                }
                None => components::render_verse(
                    ui,
                    verse,
                    &self.settings,
                    highlight_terms,
                    theme,
                    red_letter,
                ),
            },
        }
    }

    fn render_search_panels(&mut self, ui: &mut Ui, theme: &Theme) {
        // Search section header with toggle
        ui.horizontal(|ui| {
            let toggle_icon = if self.settings.show_search_panel {
                "\u{25BC}"
            } else {
                "\u{25B6}"
            };
            if ui
                .button(RichText::new(toggle_icon).size(12.0).monospace())
                .on_hover_text("Toggle search panel")
                .clicked()
            {
                self.settings.show_search_panel = !self.settings.show_search_panel;
                self.settings.mark_dirty();
            }
            ui.heading("Search");

            if self.settings.show_search_panel {
                // Height adjustment buttons
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("+")
                        .on_hover_text("Increase panel height")
                        .clicked()
                    {
                        self.settings.search_panel_height =
                            (self.settings.search_panel_height + 30.0).min(400.0);
                        self.settings.mark_dirty();
                    }
                    if ui
                        .small_button("-")
                        .on_hover_text("Decrease panel height")
                        .clicked()
                    {
                        self.settings.search_panel_height =
                            (self.settings.search_panel_height - 30.0).max(60.0);
                        self.settings.mark_dirty();
                    }
                    if !self.search_results.is_empty() {
                        ui.label(
                            RichText::new(count_label(self.search_results.len(), "result"))
                                .size(12.0)
                                .color(theme.text_muted),
                        );
                    }
                });
            }
        });

        if self.settings.show_search_panel {
            ui.horizontal(|ui| {
                let search_response = ui.add(
                    TextEdit::singleline(&mut self.search_query)
                        .hint_text("Search for text...")
                        .desired_width(250.0),
                );

                // Focus search on Ctrl+F
                if ui.input(|i| i.modifiers.command && i.key_pressed(Key::F)) {
                    search_response.request_focus();
                }
                // Enter searches immediately
                if search_response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    self.perform_search();
                }

                // Scope selector
                let prev_scope = self.settings.search_scope;
                ComboBox::from_id_salt("search_scope")
                    .selected_text(self.settings.search_scope.label())
                    .width(120.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.settings.search_scope,
                            SearchScope::All,
                            "All",
                        );
                        ui.selectable_value(
                            &mut self.settings.search_scope,
                            SearchScope::CurrentBook,
                            "Current Book",
                        );
                        ui.selectable_value(
                            &mut self.settings.search_scope,
                            SearchScope::OldTestament,
                            "Old Testament",
                        );
                        ui.selectable_value(
                            &mut self.settings.search_scope,
                            SearchScope::NewTestament,
                            "New Testament",
                        );
                    });
                if self.settings.search_scope != prev_scope {
                    self.settings.mark_dirty();
                    self.perform_search();
                }

                // Clear button
                if !self.search_query.is_empty()
                    && ui
                        .button("X")
                        .on_hover_text("Clear Search (Escape)")
                        .clicked()
                    {
                        self.search_query.clear();
                        self.perform_search();
                    }

                // Manual search button
                if ui.button("Search").clicked() {
                    self.perform_search();
                }
            });

            // Search results
            if !self.search_results.is_empty() {
                ui.add_space(5.0);

                ScrollArea::vertical()
                    .max_height(self.settings.search_panel_height)
                    .id_salt("search_results_scroll")
                    .show(ui, |ui| {
                        for i in 0..self.search_results.len() {
                            let result = &self.search_results[i];
                            let preview: String = result.text.chars().take(60).collect();
                            let location = if result.verse_number == 0 {
                                format!("{} {} (title)", result.book, result.chapter)
                            } else {
                                format!("{} {}:{}", result.book, result.chapter, result.verse_number)
                            };
                            let reference = format!(
                                "{} - {}",
                                location,
                                if result.text.chars().count() > 60 {
                                    format!("{}...", preview)
                                } else {
                                    result.text.clone()
                                }
                            );

                            if ui.selectable_label(false, &reference).clicked() {
                                self.navigate_to = Some((
                                    result.book.clone(),
                                    result.chapter,
                                    Some(result.verse_number),
                                ));
                            }
                        }
                    });
            }
        }

        // Strong's search section (only if original languages available)
        if self.has_original_languages() {
            ui.add_space(10.0);
            ui.separator();

            // Strong's search header with toggle
            ui.horizontal(|ui| {
                let toggle_icon = if self.settings.show_strongs_panel {
                    "\u{25BC}"
                } else {
                    "\u{25B6}"
                };
                if ui
                    .button(RichText::new(toggle_icon).size(12.0).monospace())
                    .on_hover_text("Toggle Strong's panel")
                    .clicked()
                {
                    self.settings.show_strongs_panel = !self.settings.show_strongs_panel;
                    self.settings.mark_dirty();
                }
                ui.heading("Strong's Search");

                if self.settings.show_strongs_panel {
                    // Height adjustment buttons
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("+")
                            .on_hover_text("Increase panel height")
                            .clicked()
                        {
                            self.settings.strongs_panel_height =
                                (self.settings.strongs_panel_height + 30.0).min(400.0);
                            self.settings.mark_dirty();
                        }
                        if ui
                            .small_button("-")
                            .on_hover_text("Decrease panel height")
                            .clicked()
                        {
                            self.settings.strongs_panel_height =
                                (self.settings.strongs_panel_height - 30.0).max(60.0);
                            self.settings.mark_dirty();
                        }
                        if self.strongs_count > 0 {
                            ui.label(
                                RichText::new(count_label(self.strongs_count, "verse"))
                                    .size(12.0)
                                    .color(theme.text_muted),
                            );
                        }
                    });
                }
            });

            if self.settings.show_strongs_panel {
                ui.horizontal(|ui| {
                    ui.label("Strong's #:");
                    let strongs_response = ui.add(
                        TextEdit::singleline(&mut self.strongs_query)
                            .hint_text("e.g., H430, G2316")
                            .desired_width(120.0),
                    );

                    if strongs_response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        self.perform_strongs_search();
                    }

                    if ui.button("Search").clicked() {
                        self.perform_strongs_search();
                    }

                    if !self.strongs_query.is_empty()
                        && ui.button("X").on_hover_text("Clear").clicked() {
                            self.strongs_query.clear();
                            self.perform_strongs_search();
                        }
                });

                if let Some(key) = &self.strongs_key {
                    let display = components::format_strongs_display(key);
                    let gloss = self
                        .extended_bible
                        .as_ref()
                        .and_then(|ext| ext.get_lexicon_entry(key))
                        .map(|entry| entry.gloss.clone());
                    ui.add_space(5.0);
                    if self.strongs_results.is_empty() {
                        ui.label(
                            RichText::new(format!("No verses found for {}", display))
                                .color(theme.text_muted),
                        );
                    } else {
                        let summary = match gloss {
                            Some(g) if !g.is_empty() => format!("{} \u{2014} {}", display, g),
                            _ => display,
                        };
                        ui.label(format!(
                            "{}: showing {} of {}",
                            summary,
                            self.strongs_results.len(),
                            count_label(self.strongs_count, "verse")
                        ));
                    }
                } else if !self.strongs_query.trim().is_empty() {
                    ui.label(
                        RichText::new("Enter a number like H430 or G2316")
                            .color(theme.text_muted),
                    );
                }

                // Strong's search results
                if !self.strongs_results.is_empty() {

                    ScrollArea::vertical()
                        .max_height(self.settings.strongs_panel_height)
                        .id_salt("strongs_results_scroll")
                        .show(ui, |ui| {
                            for i in 0..self.strongs_results.len() {
                                let verse_ref = &self.strongs_results[i];
                                let reference = format!(
                                    "{} {}:{}",
                                    verse_ref.book, verse_ref.chapter, verse_ref.verse
                                );
                                if ui.selectable_label(false, &reference).clicked() {
                                    self.navigate_to = Some((
                                        verse_ref.book.clone(),
                                        verse_ref.chapter,
                                        Some(verse_ref.verse),
                                    ));
                                }
                            }
                        });
                }
            }
        }
    }

    fn render_lexicon_popup(&mut self, ctx: &Context) {
        if let Some(ref strongs_number) = self.show_lexicon_popup {
            let entry = self
                .extended_bible
                .as_ref()
                .and_then(|ext| ext.get_lexicon_entry(strongs_number));
            let mut open = true;
            components::render_lexicon_popup(
                ctx,
                strongs_number,
                entry,
                &mut open,
                &self.settings,
            );
            if !open {
                self.show_lexicon_popup = None;
            }
        }
    }

    fn render_settings_window(&mut self, ctx: &Context) {
        if self.show_settings_window {
            egui::Window::new("Settings")
                .collapsible(false)
                .resizable(true)
                .default_size([380.0, 560.0])
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        if components::settings_panel(ui, &mut self.settings, "window") {
                            self.settings.mark_dirty();
                        }

                        // Original language settings (only if available)
                        if self.has_original_languages() {
                            ui.add_space(10.0);
                            if components::original_language_settings_panel(ui, &mut self.settings)
                            {
                                self.settings.mark_dirty();
                            }
                        }

                        ui.add_space(10.0);
                        ui.separator();

                        // Attribution
                        ui.collapsing("About", |ui| {
                            ui.label("KJV Interlinear");
                            ui.label("Application code: MIT License");
                            ui.add_space(5.0);
                            ui.label(RichText::new("KJV text:").strong());
                            ui.label("1769 standard text (public domain) via CrossWire / eBible.org");
                            ui.hyperlink_to("eBible.org", "https://ebible.org/find/details.php?id=eng-kjv");
                            ui.add_space(5.0);
                            if self.has_original_languages() {
                                ui.label(RichText::new("Original language data:").strong());
                                ui.label(
                                    "TAHOT, TAGNT, TBESH, TBESG from STEP Bible (CC BY 4.0).",
                                );
                                ui.hyperlink_to(
                                    "STEPBible.org",
                                    "https://www.STEPBible.org/",
                                );
                                ui.hyperlink_to(
                                    "STEPBible-Data",
                                    "https://github.com/STEPBible/STEPBible-Data",
                                );
                                ui.add_space(5.0);
                            }
                            ui.label(RichText::new("Red-letter words of Christ:").strong());
                            ui.label("Kenneth Reitz / kjvstudy.org (ISC License)");
                            ui.hyperlink_to(
                                "kjvstudy.org",
                                "https://github.com/kennethreitz/kjvstudy.org",
                            );
                            ui.add_space(5.0);
                            ui.label("See NOTICE in the project root for full attribution.");
                        });

                        ui.add_space(10.0);
                        if ui.button("Close").clicked() {
                            self.show_settings_window = false;
                        }
                    });
                });
        }
    }
}

impl eframe::App for BibleApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        let theme = self.apply_theme(ctx);
        let was_typing = ctx.wants_keyboard_input();
        self.handle_keyboard(ctx);

        // Handle navigation queue
        if let Some((book, chapter, verse)) = self.navigate_to.take() {
            self.selected_book = book;
            self.selected_chapter = chapter;
            self.selected_verse = 1;
            self.update_chapter_display();
            self.go_to_verse(verse.unwrap_or(1));
        }

        // Update copy feedback timer
        if let Some((_, ref mut time)) = self.copy_feedback {
            *time -= ctx.input(|i| i.predicted_dt as f64);
            if *time <= 0.0 {
                self.copy_feedback = None;
            }
        }

        // Live search: run 0.3 s after the user stops typing
        if self.search_query != self.last_search_query {
            let now = ctx.input(|i| i.time);
            match &self.search_debounce {
                Some((pending, due)) if *pending == self.search_query => {
                    if now >= *due {
                        self.perform_search();
                    } else {
                        ctx.request_repaint_after(std::time::Duration::from_secs_f64(due - now));
                    }
                }
                _ => {
                    self.search_debounce = Some((self.search_query.clone(), now + 0.3));
                    ctx.request_repaint_after(std::time::Duration::from_millis(300));
                }
            }
        }

        self.render_top_panel(ctx, &theme);
        if self.settings.show_sidebar {
            self.render_sidebar(ctx, &theme);
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme.bg_base)
                    .inner_margin(egui::Margin::symmetric(24, 16)),
            )
            .show(ctx, |ui| {
                self.render_chapter_view(ui, &theme);
                let top = ui.cursor().top();
                self.render_search_panels(ui, &theme);
                let height = ui.cursor().top() - top;
                if (height - self.search_panels_height).abs() > 0.5 {
                    self.search_panels_height = height;
                    ui.ctx().request_repaint();
                }
            });

        self.render_lexicon_popup(ctx);
        self.render_settings_window(ctx);
        self.handle_copy_shortcut(ctx, was_typing);
        self.settings.save_if_dirty();
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.settings.force_save();
    }
}
