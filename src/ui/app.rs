use eframe::egui::{self, Color32, ComboBox, Context, Key, RichText, ScrollArea, TextEdit, Ui};

use crate::models::{
    Bible, ExtendedBible, InterlinearVerse, SearchScope, Testament, Verse, VerseRef,
};
use crate::red_letter::RedLetterIndex;
use crate::settings::{DisplayMode, SettingsStore};
use crate::theme::{Theme, get_theme};
use crate::ui::components;

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

    // Search state
    search_query: String,
    search_results: Vec<Verse>,
    last_search_query: String,
    search_debounce_timer: f64,

    // Strong's search state
    strongs_query: String,
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
            search_query: String::new(),
            search_results: Vec::new(),
            last_search_query: String::new(),
            search_debounce_timer: 0.0,
            strongs_query: String::new(),
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
        if self.strongs_query.is_empty() {
            self.strongs_results.clear();
            self.strongs_count = 0;
            return;
        }

        // Normalize the query (uppercase H or G prefix)
        let query = self.strongs_query.trim().to_uppercase();
        let query = if query.starts_with('H') || query.starts_with('G') {
            query
        } else {
            // Assume Hebrew if no prefix
            format!("H{}", query)
        };

        if let Some(ref ext) = self.extended_bible {
            self.strongs_count = ext.strongs_count(&query);
            if let Some(refs) = ext.strongs_index.get_occurrences(&query) {
                // Limit to first 100 results for performance
                self.strongs_results = refs.iter().take(100).cloned().collect();
            } else {
                self.strongs_results.clear();
            }
        }
    }

    fn update_chapter_display(&mut self) {
        self.current_chapter_verses.clear();

        let Some(chapter) = self
            .bible
            .get_chapter(&self.selected_book, self.selected_chapter)
        else {
            return; // do NOT update position or history for a failed load
        };

        self.current_chapter_verses = chapter.verses.clone();

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
        if self.search_query.is_empty() {
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
            }

            // Ctrl+B: toggle bookmark
            if i.key_pressed(Key::B) && i.modifiers.command {
                self.toggle_bookmark();
            }

            // Ctrl+C: copy verse (when not in text input)
            if i.key_pressed(Key::C) && i.modifiers.command && !i.modifiers.shift {
                self.copy_current_verse();
            }

            // Ctrl+Shift+C: copy chapter
            if i.key_pressed(Key::C) && i.modifiers.command && i.modifiers.shift {
                self.copy_current_chapter();
            }
        });
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
                        RichText::new("Bible Reader")
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
                    egui::Stroke::new(1.0, theme.divider),
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

                    // Go button
                    let go_button =
                        egui::Button::new(RichText::new("Go").color(theme.primary).size(13.0))
                            .fill(Color32::from_rgba_unmultiplied(
                                theme.primary.r(),
                                theme.primary.g(),
                                theme.primary.b(),
                                20,
                            ))
                            .corner_radius(egui::CornerRadius::same(6));
                    if ui.add(go_button).clicked() {
                        self.update_chapter_display();
                    }

                    ui.add_space(8.0);

                    // Verse input with label
                    ui.label(RichText::new("Verse:").color(theme.text_muted).size(13.0));
                    let verse_response = ui.add(
                        TextEdit::singleline(&mut self.verse_input)
                            .desired_width(45.0)
                            .hint_text("#")
                            .font(egui::TextStyle::Body),
                    );
                    if verse_response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        if let Ok(v) = self.verse_input.parse::<u32>() {
                            let max_verse = self.current_chapter_verses.len() as u32;
                            self.selected_verse = if max_verse == 0 {
                                1
                            } else {
                                v.clamp(1, max_verse)
                            };
                            self.update_chapter_display();
                        }
                        self.verse_input.clear();
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
                    .stroke(egui::Stroke::new(1.0, theme.border)),
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
                    egui::Stroke::new(1.0, theme.divider),
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
            egui::Stroke::new(1.0, theme.divider),
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

        // Calculate reserved height for search panels
        let search_reserved = if self.settings.show_search_panel {
            self.settings.search_panel_height + 60.0 // panel + header
        } else {
            0.0
        };
        let strongs_reserved = if self.settings.show_strongs_panel && self.has_original_languages()
        {
            self.settings.strongs_panel_height + 60.0 // panel + header
        } else {
            0.0
        };
        let total_reserved = search_reserved + strongs_reserved + 20.0; // extra padding

        // Chapter content
        let available_height = ui.available_height() - total_reserved;
        let mut clicked_strongs: Option<String> = None;

        ScrollArea::vertical()
            .max_height(available_height.max(200.0))
            .id_salt("chapter_scroll")
            .show(ui, |ui| {
                let highlight_terms: Vec<String> = if !self.search_query.is_empty() {
                    vec![self.search_query.clone()]
                } else {
                    vec![]
                };

                let verse_count = self.current_chapter_verses.len();
                for i in 0..verse_count {
                    let verse = self.current_chapter_verses[i].clone();
                    match self.settings.display_mode {
                        DisplayMode::KjvOnly => {
                            components::render_verse(
                                ui,
                                &verse,
                                &self.settings,
                                &highlight_terms,
                                theme,
                                self.red_letter.as_ref(),
                            );
                        }
                        DisplayMode::Parallel => {
                            let interlinear = self.get_current_interlinear(verse.verse_number);
                            components::render_verse_parallel(
                                ui,
                                &verse,
                                interlinear,
                                &self.settings,
                                &highlight_terms,
                                theme,
                                self.red_letter.as_ref(),
                            );
                        }
                        DisplayMode::Interlinear => {
                            let interlinear = self.get_current_interlinear(verse.verse_number);
                            if let Some(strongs) = components::render_verse_interlinear(
                                ui,
                                &verse,
                                interlinear,
                                &self.settings,
                                theme,
                                self.red_letter.as_ref(),
                            ) {
                                clicked_strongs = Some(strongs);
                            }
                        }
                        DisplayMode::OriginalOnly => {
                            let interlinear = self.get_current_interlinear(verse.verse_number);
                            if let Some(orig) = interlinear {
                                let font_size = self.settings.font_size.pixels();
                                let orig_color = match orig.language {
                                    crate::models::OriginalLanguage::Greek => theme.greek_text,
                                    _ => theme.hebrew_text,
                                };
                                let font_offset = match orig.language {
                                    crate::models::OriginalLanguage::Greek => {
                                        self.settings.greek_font_size_offset
                                    }
                                    _ => self.settings.hebrew_font_size_offset,
                                };
                                let original_text: String = orig
                                    .original_words
                                    .iter()
                                    .map(|w| w.original_text.as_str())
                                    .collect::<Vec<&str>>()
                                    .join(" ");

                                ui.horizontal_wrapped(|ui| {
                                    if self.settings.show_verse_numbers {
                                        ui.label(
                                            RichText::new(format!("{} ", verse.verse_number))
                                                .size(font_size)
                                                .strong()
                                                .color(theme.verse_number),
                                        );
                                    }
                                    ui.label(
                                        RichText::new(&original_text)
                                            .size(font_size + font_offset)
                                            .color(orig_color),
                                    );
                                });
                                ui.add_space(8.0);
                            } else {
                                components::render_verse(
                                    ui,
                                    &verse,
                                    &self.settings,
                                    &highlight_terms,
                                    theme,
                                    self.red_letter.as_ref(),
                                );
                            }
                        }
                    }
                }
            });

        // Handle Strong's number clicks
        if let Some(strongs) = clicked_strongs {
            self.show_lexicon_popup = Some(strongs);
        }

        ui.separator();
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
                .button(RichText::new(toggle_icon).size(12.0))
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
                            RichText::new(format!("{} results", self.search_results.len()))
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
                        self.search_results.clear();
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
                            let reference = format!(
                                "{} {}:{} - {}",
                                result.book,
                                result.chapter,
                                result.verse_number,
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
                    .button(RichText::new(toggle_icon).size(12.0))
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
                                RichText::new(format!("{} occurrences", self.strongs_count))
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
                            self.strongs_results.clear();
                            self.strongs_count = 0;
                        }
                });

                // Strong's search results
                if !self.strongs_results.is_empty() {
                    ui.add_space(5.0);
                    ui.label(format!(
                        "Showing {} of {} results:",
                        self.strongs_results.len(),
                        self.strongs_count
                    ));

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
                .default_size([350.0, 400.0])
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
                            ui.label("Project Gutenberg eBook #10");
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
        self.handle_keyboard(ctx);

        // Handle navigation queue
        if let Some((book, chapter, verse)) = self.navigate_to.take() {
            self.selected_book = book;
            self.selected_chapter = chapter;
            if let Some(v) = verse {
                self.selected_verse = v;
            }
            self.update_chapter_display();
        }

        // Update copy feedback timer
        if let Some((_, ref mut time)) = self.copy_feedback {
            *time -= ctx.input(|i| i.predicted_dt as f64);
            if *time <= 0.0 {
                self.copy_feedback = None;
            }
        }

        // Live search debounce
        if self.search_query != self.last_search_query {
            self.search_debounce_timer = 0.3;
        }
        if self.search_debounce_timer > 0.0 {
            self.search_debounce_timer -= ctx.input(|i| i.predicted_dt as f64);
            if self.search_debounce_timer <= 0.0 {
                self.perform_search();
            }
            ctx.request_repaint();
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
                self.render_search_panels(ui, &theme);
            });

        self.render_lexicon_popup(ctx);
        self.render_settings_window(ctx);
        self.settings.save_if_dirty();
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.settings.force_save();
    }
}
