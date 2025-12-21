mod models;
mod original_languages;
mod parsing;
mod settings;
mod theme;
mod ui;

use std::path::Path;

use eframe::{NativeOptions, egui};

use models::Bible;
use ui::BibleApp;

fn main() -> eframe::Result<()> {
    // Define paths to testament directories
    let old_testament_path = Path::new("old_testament");
    let new_testament_path = Path::new("new_testament");
    let data_path = Path::new("data");

    // Load the Bible data (KJV)
    let bible = match Bible::from_directories(old_testament_path, new_testament_path) {
        Ok(bible) => bible,
        Err(e) => {
            eprintln!("Error loading Bible: {}", e);
            return Ok(());
        }
    };

    // Load original language data if available
    let extended_bible = if data_path.exists() {
        match original_languages::load_extended_bible(data_path) {
            Ok(ext) => {
                if ext.is_loaded() {
                    eprintln!(
                        "Loaded original language data: {} OT verses, {} NT verses",
                        ext.interlinear_ot.len(),
                        ext.interlinear_nt.len()
                    );
                    Some(ext)
                } else {
                    eprintln!("No original language data found in data/");
                    None
                }
            }
            Err(e) => {
                eprintln!("Warning: Could not load original language data: {}", e);
                None
            }
        }
    } else {
        eprintln!("Note: data/ directory not found. Original language features disabled.");
        None
    };

    // Set up window options
    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([800.0, 600.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    // Run the app
    eframe::run_native(
        "Bible App",
        options,
        Box::new(|_cc| Ok(Box::new(BibleApp::new(bible, extended_bible)))),
    )
}
