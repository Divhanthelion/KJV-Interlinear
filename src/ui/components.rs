use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke, Ui, Vec2};

use crate::models::{
    Bookmark, HistoryEntry, InterlinearVerse, LexiconEntry, OriginalLanguage, OriginalWord, Verse,
};
use crate::red_letter::{red_letter_segments, RedLetterIndex};
use crate::settings::{DisplayMode, FontSize, Settings};
use crate::theme::Theme;

/// Render a verse with theme colors and accurate red-letter spans.
pub fn render_verse(
    ui: &mut Ui,
    verse: &Verse,
    settings: &Settings,
    highlight_terms: &[String],
    theme: &Theme,
    red_letter: Option<&RedLetterIndex>,
) {
    let font_size = settings.font_size.pixels();

    ui.horizontal_wrapped(|ui: &mut Ui| {
        if settings.show_verse_numbers {
            ui.label(
                RichText::new(format!("{} ", verse.verse_number))
                    .size(font_size)
                    .strong()
                    .color(theme.verse_number),
            );
        }

        if !highlight_terms.is_empty() {
            render_highlighted_text(ui, &verse.text, highlight_terms, font_size, theme);
        } else if settings.red_letter {
            if let Some(spec) = red_letter.and_then(|idx| {
                idx.get(&verse.book, verse.chapter, verse.verse_number)
            }) {
                for (segment, is_red) in red_letter_segments(&verse.text, spec) {
                    if segment.is_empty() {
                        continue;
                    }
                    let color = if is_red {
                        theme.red_letter
                    } else {
                        theme.text_primary
                    };
                    ui.label(RichText::new(segment).size(font_size).color(color));
                }
            } else {
                ui.label(
                    RichText::new(&verse.text)
                        .size(font_size)
                        .color(theme.text_primary),
                );
            }
        } else {
            ui.label(
                RichText::new(&verse.text)
                    .size(font_size)
                    .color(theme.text_primary),
            );
        }
    });

    ui.add_space(10.0);
}

/// Render text with search term highlighting
fn render_highlighted_text(
    ui: &mut Ui,
    text: &str,
    terms: &[String],
    font_size: f32,
    theme: &Theme,
) {
    let text_lower = text.to_lowercase();

    let mut highlights: Vec<(usize, usize)> = Vec::new();
    for term in terms {
        let term_lower = term.to_lowercase();
        let mut start = 0;
        while let Some(pos) = text_lower[start..].find(&term_lower) {
            let abs_pos = start + pos;
            highlights.push((abs_pos, abs_pos + term.len()));
            start = abs_pos + 1;
        }
    }

    highlights.sort_by_key(|h| h.0);

    if highlights.is_empty() {
        ui.label(
            RichText::new(text)
                .size(font_size)
                .color(theme.text_primary),
        );
        return;
    }

    let mut last_end = 0;
    for (start, end) in highlights {
        if start > last_end {
            ui.label(
                RichText::new(&text[last_end..start])
                    .size(font_size)
                    .color(theme.text_primary),
            );
        }
        if start >= last_end {
            ui.label(
                RichText::new(&text[start..end])
                    .size(font_size)
                    .background_color(theme.highlight_bg),
            );
            last_end = end;
        }
    }
    if last_end < text.len() {
        ui.label(
            RichText::new(&text[last_end..])
                .size(font_size)
                .color(theme.text_primary),
        );
    }
}

/// Render a bookmark item - returns (clicked, delete)
pub fn render_bookmark(ui: &mut Ui, bookmark: &Bookmark, settings: &Settings, theme: &Theme) -> (bool, bool) {
    let font_size = settings.font_size.pixels() - 2.0;

    let reference = format!("{} {}:{}", bookmark.book, bookmark.chapter, bookmark.verse);

    let mut clicked = false;
    let mut delete = false;

    ui.horizontal(|ui: &mut Ui| {
        if ui
            .selectable_label(
                false,
                RichText::new(&reference)
                    .size(font_size)
                    .color(theme.text_accent),
            )
            .clicked()
        {
            clicked = true;
        }

        ui.with_layout(
            egui::Layout::right_to_left(egui::Align::Center),
            |ui: &mut Ui| {
                if ui.small_button("X").clicked() {
                    delete = true;
                }
            },
        );
    });

    (clicked, delete)
}

/// Render a history entry - returns true if clicked
pub fn render_history_entry(
    ui: &mut Ui,
    entry: &HistoryEntry,
    settings: &Settings,
    theme: &Theme,
) -> bool {
    let font_size = settings.font_size.pixels() - 2.0;

    let reference = format!("{} {}", entry.book, entry.chapter);
    let time_ago = format_time_ago(entry.timestamp);

    let mut clicked = false;

    ui.horizontal(|ui: &mut Ui| {
        if ui
            .selectable_label(false, RichText::new(&reference).size(font_size))
            .clicked()
        {
            clicked = true;
        }
        ui.label(
            RichText::new(&time_ago)
                .size(font_size - 2.0)
                .color(theme.text_muted),
        );
    });

    clicked
}

/// Format a timestamp as relative time
fn format_time_ago(timestamp: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let diff = now.saturating_sub(timestamp);

    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86400)
    }
}

/// Icon button helper
#[allow(dead_code)]
pub fn icon_button(ui: &mut Ui, icon: &str, tooltip: &str) -> bool {
    ui.add(egui::Button::new(icon).min_size(Vec2::new(28.0, 28.0)))
        .on_hover_text(tooltip)
        .clicked()
}

/// Styled icon button with theme support
pub fn styled_icon_button(ui: &mut Ui, icon: &str, tooltip: &str, theme: &Theme) -> bool {
    let button = egui::Button::new(RichText::new(icon).size(18.0).color(theme.text_secondary))
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::same(6))
        .min_size(Vec2::new(32.0, 32.0));

    let response = ui.add(button);

    // Custom hover effect
    if response.hovered() {
        let rect = response.rect;
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), theme.hover_overlay);
    }

    response.on_hover_text(tooltip).clicked()
}

/// Styled navigation button (prev/next)
pub fn nav_button(ui: &mut Ui, icon: &str, tooltip: &str, theme: &Theme) -> bool {
    let button = egui::Button::new(RichText::new(icon).size(16.0).color(theme.primary))
        .fill(Color32::from_rgba_unmultiplied(
            theme.primary.r(),
            theme.primary.g(),
            theme.primary.b(),
            15,
        ))
        .stroke(Stroke::new(
            1.0,
            Color32::from_rgba_unmultiplied(
                theme.primary.r(),
                theme.primary.g(),
                theme.primary.b(),
                40,
            ),
        ))
        .corner_radius(CornerRadius::same(6))
        .min_size(Vec2::new(32.0, 28.0));

    ui.add(button).on_hover_text(tooltip).clicked()
}

/// Styled action button (bookmark, copy, etc)
pub fn action_button(ui: &mut Ui, icon: &str, tooltip: &str, active: bool, theme: &Theme) -> bool {
    let (text_color, bg_color) = if active {
        (
            theme.warning,
            Color32::from_rgba_unmultiplied(
                theme.warning.r(),
                theme.warning.g(),
                theme.warning.b(),
                20,
            ),
        )
    } else {
        (theme.text_secondary, Color32::TRANSPARENT)
    };

    let button = egui::Button::new(RichText::new(icon).size(16.0).color(text_color))
        .fill(bg_color)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::same(6))
        .min_size(Vec2::new(28.0, 28.0));

    ui.add(button).on_hover_text(tooltip).clicked()
}

/// Settings panel - returns true if any setting changed
pub fn settings_panel(ui: &mut Ui, settings: &mut Settings) -> bool {
    let mut changed = false;

    ui.heading("Settings");
    ui.separator();

    // Dark mode toggle
    ui.horizontal(|ui: &mut Ui| {
        ui.label("Dark Mode:");
        if ui.checkbox(&mut settings.dark_mode, "").changed() {
            changed = true;
        }
    });

    // Font size
    ui.horizontal(|ui: &mut Ui| {
        ui.label("Font Size:");
        egui::ComboBox::from_id_salt("font_size_setting")
            .selected_text(settings.font_size.label())
            .show_ui(ui, |ui: &mut Ui| {
                for size in FontSize::all() {
                    if ui
                        .selectable_value(&mut settings.font_size, *size, size.label())
                        .changed()
                    {
                        changed = true;
                    }
                }
            });
    });

    // Show verse numbers
    ui.horizontal(|ui: &mut Ui| {
        ui.label("Verse Numbers:");
        if ui.checkbox(&mut settings.show_verse_numbers, "").changed() {
            changed = true;
        }
    });

    // Red letter mode
    ui.horizontal(|ui: &mut Ui| {
        ui.label("Red Letter (words of Christ):");
        if ui.checkbox(&mut settings.red_letter, "").changed() {
            changed = true;
        }
    });

    // Show sidebar
    ui.horizontal(|ui: &mut Ui| {
        ui.label("Show Sidebar:");
        if ui.checkbox(&mut settings.show_sidebar, "").changed() {
            changed = true;
        }
    });

    ui.add_space(10.0);
    ui.separator();
    ui.label(RichText::new("Search Panels").strong());
    ui.add_space(5.0);

    // Show search panel
    ui.horizontal(|ui: &mut Ui| {
        ui.label("Show Search:");
        if ui.checkbox(&mut settings.show_search_panel, "").changed() {
            changed = true;
        }
    });

    // Show Strong's panel
    ui.horizontal(|ui: &mut Ui| {
        ui.label("Show Strong's:");
        if ui.checkbox(&mut settings.show_strongs_panel, "").changed() {
            changed = true;
        }
    });

    ui.add_space(16.0);
    ui.separator();
    ui.label(RichText::new("About & Attribution").strong());
    ui.add_space(6.0);
    ui.label(
        RichText::new(
            "KJV text: Project Gutenberg. Original languages: STEP Bible (CC BY 4.0). \
Red-letter map: Kenneth Reitz / kjvstudy.org (ISC). App code: MIT. See NOTICE.",
        )
        .size(11.0)
        .color(Color32::GRAY),
    );

    changed
}

// ============================================================================
// Original Language Components
// ============================================================================

/// Render a verse in parallel view (KJV left, original language right)
pub fn render_verse_parallel(
    ui: &mut Ui,
    verse: &Verse,
    interlinear: Option<&InterlinearVerse>,
    settings: &Settings,
    highlight_terms: &[String],
    theme: &Theme,
    red_letter: Option<&RedLetterIndex>,
) {
    let font_size = settings.font_size.pixels();

    ui.horizontal(|ui| {
        // Left column: KJV
        ui.vertical(|ui| {
            ui.set_min_width(ui.available_width() * 0.45);
            render_verse(ui, verse, settings, highlight_terms, theme, red_letter);
        });

        ui.separator();

        // Right column: Original language
        ui.vertical(|ui| {
            if let Some(orig) = interlinear {
                let orig_color = if matches!(orig.language, OriginalLanguage::Hebrew | OriginalLanguage::Aramaic)
                {
                    theme.hebrew_text
                } else {
                    theme.greek_text
                };
                let original_text: String = orig
                    .original_words
                    .iter()
                    .map(|w| w.original_text.as_str())
                    .collect::<Vec<&str>>()
                    .join(" ");
                ui.label(
                    RichText::new(original_text)
                        .size(font_size + settings.hebrew_font_size_offset)
                        .color(orig_color),
                );
            } else {
                ui.label(
                    RichText::new("(no original language data)")
                        .italics()
                        .color(theme.text_muted),
                );
            }
        });
    });
}

/// Render a verse in interlinear view (word-by-word alignment)
pub fn render_verse_interlinear(
    ui: &mut Ui,
    verse: &Verse,
    interlinear: Option<&InterlinearVerse>,
    settings: &Settings,
) -> Option<String> {
    let font_size = settings.font_size.pixels();
    let mut clicked_strongs: Option<String> = None;

    ui.horizontal_wrapped(|ui| {
        // Verse number
        if settings.show_verse_numbers {
            let number_color = if settings.dark_mode {
                Color32::from_rgb(150, 180, 255)
            } else {
                Color32::from_rgb(70, 100, 180)
            };
            ui.label(
                RichText::new(format!("{} ", verse.verse_number))
                    .size(font_size)
                    .strong()
                    .color(number_color),
            );
        }

        if let Some(orig) = interlinear {
            for word in &orig.original_words {
                if let Some(strongs) = render_interlinear_word_block(ui, word, settings) {
                    clicked_strongs = Some(strongs);
                }
            }
        } else {
            // Fallback to KJV only
            let text_color = if settings.dark_mode {
                Color32::from_rgb(220, 220, 220)
            } else {
                Color32::from_rgb(30, 30, 30)
            };
            ui.label(RichText::new(&verse.text).size(font_size).color(text_color));
        }
    });

    ui.add_space(12.0);
    clicked_strongs
}

/// Render a single word block in interlinear view
/// Returns Some(strongs_number) if clicked
fn render_interlinear_word_block(
    ui: &mut Ui,
    word: &OriginalWord,
    settings: &Settings,
) -> Option<String> {
    let base_size = settings.font_size.pixels();
    let mut clicked_strongs: Option<String> = None;

    let orig_color = if settings.dark_mode {
        Color32::from_rgb(180, 150, 220)
    } else {
        Color32::from_rgb(100, 50, 150)
    };

    let translit_color = if settings.dark_mode {
        Color32::from_rgb(150, 150, 150)
    } else {
        Color32::GRAY
    };

    let strongs_color = if settings.dark_mode {
        Color32::from_rgb(100, 180, 255)
    } else {
        Color32::from_rgb(0, 100, 200)
    };

    let gloss_color = if settings.dark_mode {
        Color32::from_rgb(220, 220, 220)
    } else {
        Color32::from_rgb(30, 30, 30)
    };

    ui.vertical(|ui| {
        ui.set_min_width(60.0);

        // Original text (larger)
        ui.label(
            RichText::new(&word.original_text)
                .size(base_size + 4.0)
                .color(orig_color),
        );

        // Transliteration (if enabled)
        if settings.show_transliteration && !word.transliteration.is_empty() {
            ui.label(
                RichText::new(&word.transliteration)
                    .size(base_size - 2.0)
                    .italics()
                    .color(translit_color),
            );
        }

        // Strong's number (clickable, if enabled)
        if settings.show_strongs_inline {
            if let Some(ref strongs) = word.strongs_number {
                let strongs_response = ui.add(
                    egui::Label::new(
                        RichText::new(strongs)
                            .size(base_size - 3.0)
                            .color(strongs_color)
                            .underline(),
                    )
                    .sense(egui::Sense::click()),
                );

                if strongs_response.clicked() {
                    clicked_strongs = Some(strongs.clone());
                }

                strongs_response.on_hover_text("Click for definition");
            }
        }

        // Morphology (if enabled)
        if settings.show_morphology {
            if let Some(ref morph) = word.morphology {
                ui.label(
                    RichText::new(morph)
                        .size(base_size - 4.0)
                        .color(translit_color),
                );
            }
        }

        // English gloss
        ui.label(
            RichText::new(&word.english_gloss)
                .size(base_size - 2.0)
                .color(gloss_color),
        );
    });

    ui.add_space(6.0);
    clicked_strongs
}

/// Render a lexicon entry popup
pub fn render_lexicon_popup(
    ctx: &egui::Context,
    strongs_number: &str,
    entry: Option<&LexiconEntry>,
    open: &mut bool,
    settings: &Settings,
) {
    let font_size = settings.font_size.pixels();

    egui::Window::new(format!("Strong's {}", strongs_number))
        .collapsible(true)
        .resizable(true)
        .default_size([400.0, 300.0])
        .open(open)
        .show(ctx, |ui| {
            if let Some(entry) = entry {
                // Header with original word
                let word_color = if settings.dark_mode {
                    Color32::from_rgb(180, 150, 220)
                } else {
                    Color32::from_rgb(100, 50, 150)
                };

                ui.heading(RichText::new(&entry.original_word).color(word_color));
                ui.label(
                    RichText::new(&entry.transliteration)
                        .italics()
                        .size(font_size),
                );

                ui.separator();

                // Gloss
                ui.strong("Gloss:");
                ui.label(&entry.gloss);

                ui.add_space(8.0);

                // Full definition
                ui.strong("Definition:");
                egui::ScrollArea::vertical()
                    .max_height(150.0)
                    .show(ui, |ui| {
                        ui.label(&entry.definition);
                    });

                ui.add_space(8.0);

                // Morphology
                if !entry.morph.is_empty() {
                    ui.horizontal(|ui| {
                        ui.strong("Morphology:");
                        ui.label(&entry.morph);
                    });
                }
            } else {
                ui.label("Lexicon entry not found.");
            }
        });
}

/// Extended settings panel with original language options
pub fn original_language_settings_panel(ui: &mut Ui, settings: &mut Settings) -> bool {
    let mut changed = false;

    ui.heading("Original Languages");
    ui.separator();

    // Display mode
    ui.horizontal(|ui| {
        ui.label("Display Mode:");
        egui::ComboBox::from_id_salt("display_mode_setting")
            .selected_text(settings.display_mode.label())
            .show_ui(ui, |ui| {
                for mode in DisplayMode::all() {
                    if ui
                        .selectable_value(&mut settings.display_mode, *mode, mode.label())
                        .changed()
                    {
                        changed = true;
                    }
                }
            });
    });

    // Only show interlinear options if in Interlinear mode
    if settings.display_mode == DisplayMode::Interlinear {
        ui.add_space(5.0);

        ui.horizontal(|ui| {
            ui.label("Show Transliteration:");
            if ui
                .checkbox(&mut settings.show_transliteration, "")
                .changed()
            {
                changed = true;
            }
        });

        ui.horizontal(|ui| {
            ui.label("Show Strong's Numbers:");
            if ui.checkbox(&mut settings.show_strongs_inline, "").changed() {
                changed = true;
            }
        });

        ui.horizontal(|ui| {
            ui.label("Show Morphology:");
            if ui.checkbox(&mut settings.show_morphology, "").changed() {
                changed = true;
            }
        });
    }

    // Font size offsets
    ui.add_space(5.0);
    ui.horizontal(|ui| {
        ui.label("Hebrew Font Offset:");
        if ui
            .add(egui::Slider::new(
                &mut settings.hebrew_font_size_offset,
                -4.0..=8.0,
            ))
            .changed()
        {
            changed = true;
        }
    });

    ui.horizontal(|ui| {
        ui.label("Greek Font Offset:");
        if ui
            .add(egui::Slider::new(
                &mut settings.greek_font_size_offset,
                -4.0..=8.0,
            ))
            .changed()
        {
            changed = true;
        }
    });

    changed
}
