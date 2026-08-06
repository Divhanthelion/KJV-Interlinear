use eframe::{NativeOptions, egui};

use kjv_interlinear::fonts;
use kjv_interlinear::models::Bible;
use kjv_interlinear::original_languages;
use kjv_interlinear::paths;
use kjv_interlinear::red_letter::RedLetterIndex;
use kjv_interlinear::ui::BibleApp;

fn main() -> eframe::Result<()> {
    let old_testament_path = match paths::old_testament_dir() {
        Some(p) => p,
        None => {
            eprintln!("Error: old_testament/ directory not found. Run from the project root.");
            return Ok(());
        }
    };
    let new_testament_path = match paths::new_testament_dir() {
        Some(p) => p,
        None => {
            eprintln!("Error: new_testament/ directory not found. Run from the project root.");
            return Ok(());
        }
    };

    let bible = match Bible::from_directories(&old_testament_path, &new_testament_path) {
        Ok(bible) => bible,
        Err(e) => {
            eprintln!("Error loading Bible: {}", e);
            return Ok(());
        }
    };

    let extended_bible = if let Some(data_path) = paths::data_dir() {
        match original_languages::load_extended_bible_cached(&data_path) {
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

    let red_letter = match paths::red_letter_path() {
        Some(path) => match RedLetterIndex::load(&path) {
            Ok(idx) => {
                eprintln!("Loaded {} red-letter verse entries", idx.len());
                Some(idx)
            }
            Err(e) => {
                eprintln!("Warning: Could not load red-letter data: {}", e);
                None
            }
        },
        None => {
            eprintln!("Note: data/red_letter_verses.json not found. Red letter disabled.");
            None
        }
    };

    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title("KJV Interlinear"),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "KJV Interlinear",
        options,
        Box::new(move |cc| {
            fonts::configure_fonts(&cc.egui_ctx);
            Ok(Box::new(BibleApp::new(bible, extended_bible, red_letter)))
        }),
    )
}
