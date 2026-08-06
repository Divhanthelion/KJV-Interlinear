//! Register Hebrew and Greek capable fonts with egui.

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

/// Prefer bundled Noto fonts so Hebrew/Greek glyphs render reliably.
pub fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    let sans = include_bytes!("../assets/fonts/NotoSans-Regular.ttf");
    let hebrew = include_bytes!("../assets/fonts/NotoSansHebrew-Regular.ttf");

    fonts.font_data.insert(
        "NotoSans".to_owned(),
        FontData::from_owned(sans.to_vec()).into(),
    );
    fonts.font_data.insert(
        "NotoSansHebrew".to_owned(),
        FontData::from_owned(hebrew.to_vec()).into(),
    );

    // Proportional: Latin/Greek first, Hebrew as fallback for Hebrew codepoints
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "NotoSans".to_owned());
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .push("NotoSansHebrew".to_owned());

    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .push("NotoSans".to_owned());
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .push("NotoSansHebrew".to_owned());

    ctx.set_fonts(fonts);
}
