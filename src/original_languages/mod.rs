//! Original language support for Hebrew OT and Greek NT
//!
//! This module provides parsing and loading of STEP Bible data files
//! which include Hebrew/Greek texts with Strong's numbers and morphology.

pub mod cache;
pub mod loader;

use std::path::Path;

use crate::models::ExtendedBible;

pub use loader::load_extended_bible;

/// Load ExtendedBible from cache when valid, otherwise parse TSV and refresh cache.
pub fn load_extended_bible_cached(data_dir: &Path) -> Result<ExtendedBible, String> {
    if let Some(cached) = cache::try_load_cached(data_dir) {
        if cached.is_loaded() {
            return Ok(cached);
        }
    }

    let extended = load_extended_bible(data_dir)?;
    if extended.is_loaded() {
        cache::save_cache(data_dir, &extended);
    }
    Ok(extended)
}
