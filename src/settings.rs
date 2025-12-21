use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::models::{Bookmark, HistoryEntry, SearchScope};

/// Font size options
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum FontSize {
    Small,
    Medium,
    Large,
    ExtraLarge,
}

/// Display mode for original language texts
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum DisplayMode {
    /// KJV text only (default)
    KjvOnly,
    /// KJV and Hebrew/Greek side by side
    Parallel,
    /// Word-by-word interlinear alignment
    Interlinear,
    /// Original language text only
    OriginalOnly,
}

impl Default for DisplayMode {
    fn default() -> Self {
        DisplayMode::KjvOnly
    }
}

impl DisplayMode {
    pub fn label(&self) -> &'static str {
        match self {
            DisplayMode::KjvOnly => "KJV Only",
            DisplayMode::Parallel => "Parallel",
            DisplayMode::Interlinear => "Interlinear",
            DisplayMode::OriginalOnly => "Original Only",
        }
    }

    pub fn all() -> &'static [DisplayMode] {
        &[
            DisplayMode::KjvOnly,
            DisplayMode::Parallel,
            DisplayMode::Interlinear,
            DisplayMode::OriginalOnly,
        ]
    }
}

/// Lexicon view preference
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum LexiconView {
    /// Show definition on hover tooltip
    Tooltip,
    /// Show in sidebar panel
    Sidebar,
    /// Show in popup window
    Popup,
}

impl Default for LexiconView {
    fn default() -> Self {
        LexiconView::Tooltip
    }
}

#[allow(dead_code)]
impl LexiconView {
    pub fn label(&self) -> &'static str {
        match self {
            LexiconView::Tooltip => "Tooltip",
            LexiconView::Sidebar => "Sidebar",
            LexiconView::Popup => "Popup",
        }
    }
}

impl Default for FontSize {
    fn default() -> Self {
        FontSize::Medium
    }
}

impl FontSize {
    pub fn pixels(&self) -> f32 {
        match self {
            FontSize::Small => 12.0,
            FontSize::Medium => 16.0,
            FontSize::Large => 20.0,
            FontSize::ExtraLarge => 24.0,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            FontSize::Small => "Small",
            FontSize::Medium => "Medium",
            FontSize::Large => "Large",
            FontSize::ExtraLarge => "Extra Large",
        }
    }

    pub fn all() -> &'static [FontSize] {
        &[
            FontSize::Small,
            FontSize::Medium,
            FontSize::Large,
            FontSize::ExtraLarge,
        ]
    }
}

/// Application settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub font_size: FontSize,
    pub dark_mode: bool,
    pub show_verse_numbers: bool,
    pub red_letter: bool,
    pub last_book: String,
    pub last_chapter: u32,
    pub last_verse: u32,
    pub search_scope: SearchScope,
    pub bookmarks: Vec<Bookmark>,
    pub history: Vec<HistoryEntry>,
    pub show_sidebar: bool,
    // Original language settings
    #[serde(default)]
    pub display_mode: DisplayMode,
    #[serde(default = "default_true")]
    pub show_transliteration: bool,
    #[serde(default = "default_true")]
    pub show_strongs_inline: bool,
    #[serde(default)]
    pub show_morphology: bool,
    #[serde(default)]
    pub hebrew_font_size_offset: f32,
    #[serde(default)]
    pub greek_font_size_offset: f32,
    #[serde(default)]
    pub lexicon_view: LexiconView,
    #[serde(default)]
    pub auto_load_original: bool,
    // Search panel visibility and sizing
    #[serde(default = "default_true")]
    pub show_search_panel: bool,
    #[serde(default = "default_true")]
    pub show_strongs_panel: bool,
    #[serde(default = "default_search_height")]
    pub search_panel_height: f32,
    #[serde(default = "default_strongs_height")]
    pub strongs_panel_height: f32,
}

fn default_search_height() -> f32 {
    150.0
}

fn default_strongs_height() -> f32 {
    120.0
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            font_size: FontSize::Medium,
            dark_mode: false,
            show_verse_numbers: true,
            red_letter: true,
            last_book: "Genesis".to_string(),
            last_chapter: 1,
            last_verse: 1,
            search_scope: SearchScope::All,
            bookmarks: Vec::new(),
            history: Vec::new(),
            show_sidebar: true,
            // Original language defaults
            display_mode: DisplayMode::KjvOnly,
            show_transliteration: true,
            show_strongs_inline: true,
            show_morphology: false,
            hebrew_font_size_offset: 4.0,
            greek_font_size_offset: 2.0,
            lexicon_view: LexiconView::Tooltip,
            auto_load_original: false,
            show_search_panel: true,
            show_strongs_panel: true,
            search_panel_height: 150.0,
            strongs_panel_height: 120.0,
        }
    }
}

impl Settings {
    /// Get the settings file path
    fn settings_path() -> Option<PathBuf> {
        if let Some(proj_dirs) = directories::ProjectDirs::from("com", "BibleApp", "BibleApp") {
            let config_dir = proj_dirs.config_dir();
            fs::create_dir_all(config_dir).ok()?;
            Some(config_dir.join("settings.json"))
        } else {
            None
        }
    }

    /// Load settings from disk, or return defaults if not found
    pub fn load() -> Self {
        if let Some(path) = Self::settings_path() {
            if let Ok(contents) = fs::read_to_string(&path) {
                if let Ok(settings) = serde_json::from_str(&contents) {
                    return settings;
                }
            }
        }
        Self::default()
    }

    /// Save settings to disk
    pub fn save(&self) -> Result<(), String> {
        if let Some(path) = Self::settings_path() {
            let contents = serde_json::to_string_pretty(self)
                .map_err(|e| format!("Failed to serialize settings: {}", e))?;
            fs::write(&path, contents).map_err(|e| format!("Failed to write settings: {}", e))?;
            Ok(())
        } else {
            Err("Could not determine settings path".to_string())
        }
    }

    /// Add a bookmark
    pub fn add_bookmark(&mut self, book: String, chapter: u32, verse: u32, note: Option<String>) {
        // Don't add duplicates
        if self
            .bookmarks
            .iter()
            .any(|b| b.book == book && b.chapter == chapter && b.verse == verse)
        {
            return;
        }

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        self.bookmarks.push(Bookmark {
            book,
            chapter,
            verse,
            note,
            created_at: timestamp,
        });
    }

    /// Remove a bookmark
    pub fn remove_bookmark(&mut self, book: &str, chapter: u32, verse: u32) {
        self.bookmarks
            .retain(|b| !(b.book == book && b.chapter == chapter && b.verse == verse));
    }

    /// Check if a verse is bookmarked
    pub fn is_bookmarked(&self, book: &str, chapter: u32, verse: u32) -> bool {
        self.bookmarks
            .iter()
            .any(|b| b.book == book && b.chapter == chapter && b.verse == verse)
    }

    /// Add a history entry
    pub fn add_history(&mut self, book: String, chapter: u32) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Remove existing entry for same book/chapter
        self.history
            .retain(|h| !(h.book == book && h.chapter == chapter));

        // Add to front
        self.history.insert(
            0,
            HistoryEntry {
                book,
                chapter,
                timestamp,
            },
        );

        // Keep only last 50 entries
        self.history.truncate(50);
    }

    /// Clear history
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Update last read position
    pub fn update_position(&mut self, book: &str, chapter: u32, verse: u32) {
        self.last_book = book.to_string();
        self.last_chapter = chapter;
        self.last_verse = verse;
    }
}
