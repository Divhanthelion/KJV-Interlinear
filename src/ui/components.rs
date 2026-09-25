use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke, Ui, Vec2};

use crate::models::{
    Bookmark, HistoryEntry, InterlinearVerse, LexiconEntry, OriginalLanguage, OriginalWord, Verse,
};
use crate::red_letter::{red_letter_segments, RedLetterIndex};
use crate::settings::{DisplayMode, FontSize, Settings};
use crate::text::{find_folded_ranges, hebrew_visual, visual_bidi};
use crate::theme::Theme;

/// What the user clicked while a verse was drawn.
#[derive(Debug, Default)]
pub struct VerseInteraction {
    /// The verse number was clicked (select this verse)
    pub selected: bool,
    /// A Strong's number was clicked in interlinear view
    pub strongs: Option<String>,
}

/// Byte ranges of the verse spoken by Christ, when red letter is on.
fn red_ranges(
    verse: &Verse,
    settings: &Settings,
    red_letter: Option<&RedLetterIndex>,
) -> Vec<(usize, usize)> {
    if !settings.red_letter {
        return Vec::new();
    }
    let Some(spec) = red_letter.and_then(|idx| idx.get(&verse.book, verse.chapter, verse.verse_number))
    else {
        return Vec::new();
    };
    let base = verse.text.as_ptr() as usize;
    red_letter_segments(&verse.text, spec)
        .into_iter()
        .filter(|(_, is_red)| *is_red)
        .map(|(segment, _)| {
            let start = segment.as_ptr() as usize - base;
            (start, start + segment.len())
        })
        .collect()
}

/// KJV verse text as one wrapping label, coloured for red letter and search matches.
/// Verse 0 is a Psalm superscription and is set in italics.
fn render_kjv_text(
    ui: &mut Ui,
    verse: &Verse,
    highlight_terms: &[String],
    theme: &Theme,
    settings: &Settings,
    red_letter: Option<&RedLetterIndex>,
) {
    let font_size = settings.font_size.pixels();
    let text = verse.text.as_str();
    let is_title = verse.verse_number == 0;

    let red = red_ranges(verse, settings, red_letter);
    let highlights: Vec<(usize, usize)> = highlight_terms
        .iter()
        .flat_map(|term| find_folded_ranges(text, term))
        .collect();

    let mut cuts = vec![0, text.len()];
    for &(start, end) in red.iter().chain(&highlights) {
        cuts.push(start);
        cuts.push(end);
    }
    cuts.sort_unstable();
    cuts.dedup();
    let covers = |ranges: &[(usize, usize)], at: usize| ranges.iter().any(|&(s, e)| s <= at && at < e);

    let mut job = LayoutJob::default();
    for span in cuts.windows(2) {
        let (start, end) = (span[0], span[1]);
        let color = if is_title {
            theme.text_secondary
        } else if covers(&red, start) {
            theme.red_letter
        } else {
            theme.text_primary
        };
        let mut format = TextFormat::simple(egui::FontId::proportional(font_size), color);
        format.italics = is_title;
        if covers(&highlights, start) {
            format.background = theme.highlight_bg;
        }
        job.append(&text[start..end], 0.0, format);
    }
    ui.label(job);
}

/// Clickable verse number; returns true when clicked. Titles (verse 0) have none.
fn verse_number(ui: &mut Ui, verse: &Verse, settings: &Settings, theme: &Theme, trailing: &str) -> bool {
    if !settings.show_verse_numbers || verse.verse_number == 0 {
        return false;
    }
    ui.add(
        egui::Label::new(
            RichText::new(format!("{}{}", verse.verse_number, trailing))
                .size(settings.font_size.pixels())
                .strong()
                .color(theme.verse_number),
        )
        .sense(egui::Sense::click()),
    )
    .on_hover_text("Select this verse")
    .clicked()
}

/// Render a verse with theme colors and accurate red-letter spans.
pub fn render_verse(
    ui: &mut Ui,
    verse: &Verse,
    settings: &Settings,
    highlight_terms: &[String],
    theme: &Theme,
    red_letter: Option<&RedLetterIndex>,
) -> VerseInteraction {
    let mut interaction = VerseInteraction::default();
    ui.horizontal_wrapped(|ui: &mut Ui| {
        interaction.selected = verse_number(ui, verse, settings, theme, " ");
        render_kjv_text(ui, verse, highlight_terms, theme, settings, red_letter);
    });

    ui.add_space(10.0);
    interaction
}

/// Original-language words as a wrapping paragraph (right-to-left for Hebrew),
/// led by the verse number when `verse` is given. Returns true if the number was clicked.
pub fn render_original_paragraph(
    ui: &mut Ui,
    orig: &InterlinearVerse,
    verse: Option<&Verse>,
    settings: &Settings,
    theme: &Theme,
) -> bool {
    let font_size = settings.font_size.pixels();
    let is_hebrew = matches!(
        orig.language,
        OriginalLanguage::Hebrew | OriginalLanguage::Aramaic
    );
    let (color, offset) = if is_hebrew {
        (theme.hebrew_text, settings.hebrew_font_size_offset)
    } else {
        (theme.greek_text, settings.greek_font_size_offset)
    };

    let words: Vec<&str> = orig
        .original_words
        .iter()
        .map(|w| w.original_text.trim())
        .filter(|t| !t.is_empty())
        .collect();
    if words.is_empty() {
        ui.label(RichText::new("(empty)").italics().color(theme.text_muted));
        return false;
    }

    // egui has no bidi support: lay words out right-to-left and reverse each
    // word's letters so Hebrew reads correctly on screen.
    let layout = if is_hebrew {
        egui::Layout::right_to_left(egui::Align::TOP).with_main_wrap(true)
    } else {
        egui::Layout::left_to_right(egui::Align::TOP).with_main_wrap(true)
    };
    let mut selected = false;
    ui.with_layout(layout, |ui| {
        ui.spacing_mut().item_spacing.x = font_size * 0.3;
        if let Some(verse) = verse {
            // First item: at the right edge for Hebrew, the left edge for Greek
            selected = verse_number(ui, verse, settings, theme, "");
        }
        for word in words {
            let shown = if is_hebrew {
                hebrew_visual(word)
            } else {
                word.to_string()
            };
            ui.label(RichText::new(shown).size(font_size + offset).color(color));
        }
    });
    selected
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
            1.0_f32,
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

/// Settings panel - returns true if any setting changed.
/// `id_source` salts egui widget IDs so sidebar and window instances don't clash.
pub fn settings_panel(ui: &mut Ui, settings: &mut Settings, id_source: &str) -> bool {
    let mut changed = false;

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
        egui::ComboBox::from_id_salt(format!("font_size_setting_{}", id_source))
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
            "KJV text: 1769 standard text via CrossWire / eBible.org (public domain). \
Original languages: STEP Bible (CC BY 4.0). Red-letter map: Kenneth Reitz / kjvstudy.org (ISC). \
App code: MIT. See NOTICE.",
        )
        .size(11.0)
        .color(Color32::GRAY),
    );

    changed
}

// ============================================================================
// Original Language Components
// ============================================================================

/// Render a verse in parallel view: equal columns, KJV | original language.
pub fn render_verse_parallel(
    ui: &mut Ui,
    verse: &Verse,
    interlinear: Option<&InterlinearVerse>,
    settings: &Settings,
    highlight_terms: &[String],
    theme: &Theme,
    red_letter: Option<&RedLetterIndex>,
) -> VerseInteraction {
    let font_size = settings.font_size.pixels();
    let mut interaction = VerseInteraction::default();

    // Shared verse number above both columns keeps rows aligned
    if verse_number(ui, verse, settings, theme, "") {
        interaction.selected = true;
    }
    if settings.show_verse_numbers && verse.verse_number > 0 {
        ui.add_space(2.0);
    }

    ui.columns(2, |cols| {
        // —— Left: KJV ——
        {
            let ui = &mut cols[0];
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                render_kjv_text(ui, verse, highlight_terms, theme, settings, red_letter);
            });
        }

        // —— Right: original language as a single wrapping paragraph ——
        {
            let ui = &mut cols[1];
            ui.set_min_width(ui.available_width());

            if let Some(orig) = interlinear {
                render_original_paragraph(ui, orig, None, settings, theme);
            } else {
                ui.label(
                    RichText::new("(no original language data)")
                        .italics()
                        .size(font_size - 2.0)
                        .color(theme.text_muted),
                );
            }
        }
    });

    ui.add_space(6.0);
    let rect = ui.available_rect_before_wrap();
    ui.painter()
        .hline(rect.x_range(), rect.top(), Stroke::new(1.0_f32, theme.divider));
    ui.add_space(10.0);
    interaction
}

/// Render a verse in interlinear view (KJV line + aligned word columns).
pub fn render_verse_interlinear(
    ui: &mut Ui,
    verse: &Verse,
    interlinear: Option<&InterlinearVerse>,
    settings: &Settings,
    theme: &Theme,
    red_letter: Option<&RedLetterIndex>,
) -> VerseInteraction {
    let font_size = settings.font_size.pixels();
    let mut interaction = VerseInteraction::default();

    // Verse number + KJV English line (context for the stacks below)
    ui.horizontal_wrapped(|ui| {
        interaction.selected = verse_number(ui, verse, settings, theme, " ");
        render_kjv_text(ui, verse, &[], theme, settings, red_letter);
    });

    ui.add_space(6.0);

    if let Some(orig) = interlinear {
        let is_hebrew = matches!(
            orig.language,
            OriginalLanguage::Hebrew | OriginalLanguage::Aramaic
        );

        // Frame/vertical inside a wrapping layout won't wrap unless each card
        // reports an explicit size via allocate_ui (known egui limitation).
        let wrap_width = ui.available_width();
        let layout = if is_hebrew {
            egui::Layout::right_to_left(egui::Align::Min).with_main_wrap(true)
        } else {
            egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true)
        };
        ui.allocate_ui_with_layout(egui::vec2(wrap_width, 0.0), layout, |ui| {
            ui.set_max_width(wrap_width);
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 10.0);

            for word in &orig.original_words {
                if let Some(strongs) =
                    render_interlinear_word_block(ui, word, settings, theme, is_hebrew)
                {
                    interaction.strongs = Some(strongs);
                }
            }
        });
    } else {
        ui.label(
            RichText::new("No original-language data for this verse.")
                .italics()
                .size(font_size - 2.0)
                .color(theme.text_muted),
        );
    }

    ui.add_space(4.0);
    let rect = ui.available_rect_before_wrap();
    ui.painter().hline(
        rect.x_range(),
        rect.top(),
        Stroke::new(1.0_f32, theme.divider),
    );
    ui.add_space(10.0);

    interaction
}

/// Tidy a STEP gloss for display: "and/ <obj.>" -> "and (obj.)".
/// Slashes mirror Hebrew prefix/suffix boundaries; <..> marks words best left untranslated.
fn format_gloss(gloss: &str) -> String {
    let text = gloss.replace('/', " ").replace('<', "(").replace('>', ")");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn format_strongs_display(strongs: &str) -> String {
    // G0027 → G27 for display (keep leading letter)
    if let Some(letter) = strongs.chars().next()
        && (letter == 'H' || letter == 'G') {
            let digits: String = strongs
                .chars()
                .skip(1)
                .skip_while(|c| *c == '0')
                .collect();
            if digits.is_empty() {
                return format!("{}0", letter);
            }
            return format!("{}{}", letter, digits);
        }
    strongs.to_string()
}

/// Render a single word column: original / translit / Strong's / gloss.
fn render_interlinear_word_block(
    ui: &mut Ui,
    word: &OriginalWord,
    settings: &Settings,
    theme: &Theme,
    is_hebrew: bool,
) -> Option<String> {
    let base_size = settings.font_size.pixels();
    let mut clicked_strongs: Option<String> = None;

    let orig_color = if is_hebrew {
        theme.hebrew_text
    } else {
        theme.greek_text
    };

    let gloss = format_gloss(&word.english_gloss);
    let orig_display = word.original_text.trim();
    if orig_display.is_empty() && gloss.is_empty() {
        return None;
    }

    let orig_size = base_size
        + if is_hebrew {
            settings.hebrew_font_size_offset
        } else {
            settings.greek_font_size_offset
        };

    // Column width from the widest visible line, measured at the size it is drawn
    let strongs_label = word
        .strongs_number
        .as_deref()
        .map(format_strongs_display)
        .unwrap_or_default();
    let morph = word.morphology.as_deref().unwrap_or("");
    let lines = [
        (orig_display, orig_size.max(base_size), true),
        (word.transliteration.as_str(), (base_size - 2.0).max(10.0), settings.show_transliteration),
        (strongs_label.as_str(), (base_size - 3.0).max(9.0), settings.show_strongs_inline),
        (morph, (base_size - 4.0).max(8.0), settings.show_morphology),
        (gloss.as_str(), (base_size - 1.0).max(11.0), true),
    ];
    let mut col_width = 56.0_f32;
    for (text, size, shown) in lines {
        if shown && !text.is_empty() {
            let width = ui.fonts(|f| {
                f.layout_no_wrap(text.to_string(), egui::FontId::proportional(size), Color32::WHITE)
                    .size()
                    .x
            });
            col_width = col_width.max(width + 4.0);
        }
    }
    // Never wider than the parent wrap row
    let max_card = (ui.available_width() - 8.0).max(56.0);
    col_width = col_width.min(max_card);

    // Explicit outer size so horizontal_wrapped can measure the card.
    // Frame/vertical alone reports unbounded width and egui won't wrap them.
    let pad_x = 16.0;
    let pad_y = 12.0;
    let mut content_h = orig_size.max(base_size) + 4.0;
    if settings.show_transliteration {
        content_h += (base_size - 2.0).max(10.0) + 4.0;
    }
    if settings.show_strongs_inline {
        content_h += (base_size - 3.0).max(9.0) + 4.0;
    }
    if settings.show_morphology && word.morphology.is_some() {
        content_h += (base_size - 4.0).max(8.0) + 4.0;
    }
    content_h += (base_size - 1.0).max(11.0) + 4.0; // gloss
    let outer = egui::vec2(col_width + pad_x, content_h + pad_y);

    let frame = egui::Frame::new()
        .fill(theme.bg_elevated)
        .stroke(Stroke::new(1.0_f32, theme.border))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(8, 6));

    ui.allocate_ui(outer, |ui| {
        ui.set_min_size(outer);
        ui.set_max_size(outer);
        frame.show(ui, |ui| {
            ui.set_min_width(col_width);
            ui.set_max_width(col_width);
            ui.vertical_centered(|ui| {
                ui.set_min_width(col_width);
                ui.set_max_width(col_width);

                let shown = if is_hebrew {
                    hebrew_visual(orig_display)
                } else {
                    orig_display.to_string()
                };
                ui.label(
                    RichText::new(shown)
                        .size(orig_size.max(base_size))
                        .strong()
                        .color(orig_color),
                );

                if settings.show_transliteration && !word.transliteration.is_empty() {
                    ui.label(
                        RichText::new(&word.transliteration)
                            .size((base_size - 2.0).max(10.0))
                            .italics()
                            .color(theme.text_muted),
                    );
                } else if settings.show_transliteration {
                    ui.add_space((base_size - 2.0).max(10.0) + 2.0);
                }

                if settings.show_strongs_inline {
                    if let Some(ref strongs) = word.strongs_number {
                        let label = format_strongs_display(strongs);
                        let strongs_response = ui.add(
                            egui::Label::new(
                                RichText::new(label)
                                    .size((base_size - 3.0).max(9.0))
                                    .color(theme.strongs_link)
                                    .underline(),
                            )
                            .sense(egui::Sense::click()),
                        );

                        if strongs_response.clicked() {
                            clicked_strongs = Some(strongs.clone());
                        }
                        strongs_response.on_hover_text(format!("Look up {}", strongs));
                    } else {
                        ui.add_space((base_size - 3.0).max(9.0) + 2.0);
                    }
                }

                if settings.show_morphology
                    && let Some(ref morph) = word.morphology
                {
                    ui.label(
                        RichText::new(morph)
                            .size((base_size - 4.0).max(8.0))
                            .color(theme.text_secondary),
                    );
                }

                ui.label(
                    RichText::new(&gloss)
                        .size((base_size - 1.0).max(11.0))
                        .color(theme.text_primary),
                );
            });
        });
    });

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

    egui::Window::new(format!("Strong's {}", format_strongs_display(strongs_number)))
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

                ui.heading(RichText::new(visual_bidi(&entry.original_word)).color(word_color));
                ui.label(
                    RichText::new(&entry.transliteration)
                        .italics()
                        .size(font_size),
                );

                ui.separator();

                // Gloss
                ui.strong("Gloss:");
                ui.label(visual_bidi(&entry.gloss));

                ui.add_space(8.0);

                // Full definition (markup already stripped at load; clean again defensively)
                ui.strong("Definition:");
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .show(ui, |ui| {
                        // Definitions quote Hebrew inline; reorder it line by line
                        let definition =
                            crate::original_languages::loader::clean_lexicon_markup(&entry.definition)
                                .lines()
                                .map(visual_bidi)
                                .collect::<Vec<_>>()
                                .join("\n");
                        ui.label(
                            RichText::new(definition)
                                .size(font_size - 1.0)
                                .color(if settings.dark_mode {
                                    Color32::from_rgb(210, 210, 215)
                                } else {
                                    Color32::from_rgb(40, 40, 45)
                                }),
                        );
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
