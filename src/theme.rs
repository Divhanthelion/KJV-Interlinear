//! Semantic theme system for Bible App
//!
//! Implements design tokens pattern from the egui design guide for
//! consistent, high-fidelity UI styling.

use eframe::egui::{self, Color32, CornerRadius, Stroke, Visuals};

/// Design tokens - semantic color system
#[allow(dead_code)]
pub struct Theme {
    // Primary colors
    pub primary: Color32,
    pub primary_hover: Color32,
    pub primary_active: Color32,

    // Background colors
    pub bg_base: Color32,
    pub bg_elevated: Color32,
    pub bg_panel: Color32,
    pub bg_input: Color32,

    // Text colors
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,
    pub text_accent: Color32,

    // Semantic colors
    pub verse_number: Color32,
    pub strongs_link: Color32,
    pub hebrew_text: Color32,
    pub greek_text: Color32,
    pub highlight_bg: Color32,
    pub red_letter: Color32,
    pub success: Color32,
    pub warning: Color32,

    // Border and divider
    pub border: Color32,
    pub divider: Color32,

    // Interactive states
    pub hover_overlay: Color32,
    pub selection: Color32,
}

impl Theme {
    /// Create dark theme with carefully tuned colors
    pub fn dark() -> Self {
        Self {
            // Primary - warm blue accent
            primary: Color32::from_rgb(100, 160, 255),
            primary_hover: Color32::from_rgb(130, 180, 255),
            primary_active: Color32::from_rgb(80, 140, 230),

            // Backgrounds - off-black for depth
            bg_base: Color32::from_rgb(18, 18, 22),
            bg_elevated: Color32::from_rgb(28, 28, 34),
            bg_panel: Color32::from_rgb(24, 24, 30),
            bg_input: Color32::from_rgb(14, 14, 18),

            // Text hierarchy
            text_primary: Color32::from_rgb(230, 230, 235),
            text_secondary: Color32::from_rgb(180, 180, 190),
            text_muted: Color32::from_rgb(120, 120, 130),
            text_accent: Color32::from_rgb(100, 160, 255),

            // Bible-specific semantics
            verse_number: Color32::from_rgb(130, 160, 220),
            strongs_link: Color32::from_rgb(80, 180, 255),
            hebrew_text: Color32::from_rgb(180, 150, 220),
            greek_text: Color32::from_rgb(150, 200, 180),
            highlight_bg: Color32::from_rgb(100, 100, 40),
            red_letter: Color32::from_rgb(255, 120, 120),
            success: Color32::from_rgb(80, 200, 120),
            warning: Color32::from_rgb(255, 180, 80),

            // Borders
            border: Color32::from_rgb(50, 50, 60),
            divider: Color32::from_rgb(40, 40, 50),

            // Interactive
            hover_overlay: Color32::from_rgba_unmultiplied(255, 255, 255, 10),
            selection: Color32::from_rgba_unmultiplied(100, 160, 255, 60),
        }
    }

    /// Create light theme
    pub fn light() -> Self {
        Self {
            // Primary - deeper blue for light mode
            primary: Color32::from_rgb(45, 100, 180),
            primary_hover: Color32::from_rgb(60, 120, 200),
            primary_active: Color32::from_rgb(35, 85, 160),

            // Backgrounds - warm whites
            bg_base: Color32::from_rgb(252, 252, 250),
            bg_elevated: Color32::from_rgb(255, 255, 255),
            bg_panel: Color32::from_rgb(248, 248, 246),
            bg_input: Color32::from_rgb(245, 245, 243),

            // Text hierarchy
            text_primary: Color32::from_rgb(30, 30, 35),
            text_secondary: Color32::from_rgb(80, 80, 90),
            text_muted: Color32::from_rgb(130, 130, 140),
            text_accent: Color32::from_rgb(45, 100, 180),

            // Bible-specific semantics
            verse_number: Color32::from_rgb(70, 100, 180),
            strongs_link: Color32::from_rgb(0, 100, 200),
            hebrew_text: Color32::from_rgb(100, 50, 150),
            greek_text: Color32::from_rgb(40, 120, 80),
            highlight_bg: Color32::from_rgb(255, 255, 150),
            red_letter: Color32::from_rgb(180, 30, 30),
            success: Color32::from_rgb(40, 160, 80),
            warning: Color32::from_rgb(220, 140, 30),

            // Borders
            border: Color32::from_rgb(220, 220, 215),
            divider: Color32::from_rgb(230, 230, 225),

            // Interactive
            hover_overlay: Color32::from_rgba_unmultiplied(0, 0, 0, 8),
            selection: Color32::from_rgba_unmultiplied(45, 100, 180, 40),
        }
    }

    /// Apply this theme to the egui Context
    pub fn apply(&self, ctx: &egui::Context) {
        let mut style = (*ctx.style()).clone();

        // Configure spacing for breathing room
        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.window_margin = egui::Margin::same(16);
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        style.spacing.indent = 20.0;
        style.spacing.scroll = egui::style::ScrollStyle {
            bar_width: 8.0,
            ..Default::default()
        };

        // Snappy animation
        style.animation_time = 0.12;

        // Modern visuals
        let mut visuals = if self.bg_base.r() < 50 {
            Visuals::dark()
        } else {
            Visuals::light()
        };

        // Panel and window styling
        visuals.panel_fill = self.bg_panel;
        visuals.window_fill = self.bg_elevated;
        visuals.extreme_bg_color = self.bg_input;
        visuals.faint_bg_color = self.bg_base;

        // Modern corner radius
        visuals.window_corner_radius = CornerRadius::same(10);
        visuals.menu_corner_radius = CornerRadius::same(8);

        // Subtle window shadow
        visuals.window_shadow = egui::epaint::Shadow {
            blur: 16,
            spread: 0,
            color: Color32::from_black_alpha(40),
            offset: [0, 4],
        };

        // Window border - subtle glass edge effect
        visuals.window_stroke = Stroke::new(1.0, Color32::from_white_alpha(15));

        // Selection
        visuals.selection.bg_fill = self.selection;
        visuals.selection.stroke = Stroke::new(1.0, self.primary);

        // Widget styling - inactive state (ghost button style)
        visuals.widgets.inactive.bg_fill = Color32::TRANSPARENT;
        visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        visuals.widgets.inactive.bg_stroke = Stroke::NONE;
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, self.text_secondary);
        visuals.widgets.inactive.corner_radius = CornerRadius::same(6);

        // Widget styling - hovered state
        visuals.widgets.hovered.bg_fill = Color32::from_rgba_unmultiplied(
            self.primary.r(),
            self.primary.g(),
            self.primary.b(),
            30,
        );
        visuals.widgets.hovered.weak_bg_fill = self.hover_overlay;
        visuals.widgets.hovered.bg_stroke = Stroke::new(
            1.0,
            Color32::from_rgba_unmultiplied(
                self.primary.r(),
                self.primary.g(),
                self.primary.b(),
                80,
            ),
        );
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, self.text_primary);
        visuals.widgets.hovered.corner_radius = CornerRadius::same(6);
        visuals.widgets.hovered.expansion = 1.0;

        // Widget styling - active state
        visuals.widgets.active.bg_fill = Color32::from_rgba_unmultiplied(
            self.primary.r(),
            self.primary.g(),
            self.primary.b(),
            50,
        );
        visuals.widgets.active.weak_bg_fill = self.primary_active;
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, self.primary);
        visuals.widgets.active.fg_stroke = Stroke::new(1.0, self.text_primary);
        visuals.widgets.active.corner_radius = CornerRadius::same(6);

        // Non-interactive widgets
        visuals.widgets.noninteractive.bg_fill = self.bg_panel;
        visuals.widgets.noninteractive.weak_bg_fill = self.bg_base;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, self.border);
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, self.text_muted);
        visuals.widgets.noninteractive.corner_radius = CornerRadius::same(6);

        // Open widgets (combo boxes, etc)
        visuals.widgets.open.bg_fill = self.bg_elevated;
        visuals.widgets.open.weak_bg_fill = self.bg_panel;
        visuals.widgets.open.bg_stroke = Stroke::new(1.0, self.primary);
        visuals.widgets.open.fg_stroke = Stroke::new(1.0, self.text_primary);
        visuals.widgets.open.corner_radius = CornerRadius::same(6);

        style.visuals = visuals;
        ctx.set_style(style);
    }
}

/// Get the current theme based on dark mode setting
pub fn get_theme(dark_mode: bool) -> Theme {
    if dark_mode {
        Theme::dark()
    } else {
        Theme::light()
    }
}
