//! All app data in one value, built from the source files at compile time and
//! embedded in the app, so no platform has to parse ~110 MB of text at startup.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::models::{Bible, ExtendedBible};
use crate::original_languages::load_extended_bible;
use crate::red_letter::RedLetterIndex;

/// Bump when the bundle layout changes.
pub const BUNDLE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataBundle {
    pub version: u32,
    pub bible: Bible,
    pub extended: ExtendedBible,
    pub red_letter: RedLetterIndex,
}

impl DataBundle {
    /// Build from the repository's source data: `old_testament/`, `new_testament/`, `data/`.
    pub fn from_sources(root: &Path) -> Result<Self, String> {
        let bible = Bible::from_directories(&root.join("old_testament"), &root.join("new_testament"))
            .map_err(|e| format!("KJV text: {}", e))?;
        let extended = load_extended_bible(&root.join("data"))?;
        let red_letter = RedLetterIndex::load(&root.join("data/words_of_jesus.json"))?;
        Ok(Self {
            version: BUNDLE_VERSION,
            bible,
            extended,
            red_letter,
        })
    }

    /// Compact binary form (bincode).
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        bincode::serialize(self).map_err(|e| format!("serialize bundle: {}", e))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let bundle: Self =
            bincode::deserialize(bytes).map_err(|e| format!("read bundle: {}", e))?;
        if bundle.version != BUNDLE_VERSION {
            return Err(format!(
                "bundle version {} does not match {}",
                bundle.version, BUNDLE_VERSION
            ));
        }
        let mut bundle = bundle;
        bundle.extended.rebuild_strongs_index();
        Ok(bundle)
    }
}
