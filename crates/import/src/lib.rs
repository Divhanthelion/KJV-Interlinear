//! kjv-import: builds `data/library/` from pinned upstream sources (see docs/LIBRARY.md).
//!
//! The command line is in `main.rs`; the converters live here so that tests can call them.

pub mod align;
pub mod bibles;
pub mod commentaries;
pub mod crossrefs;
pub mod markup;
pub mod sources;

use std::path::{Path, PathBuf};

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("repository root")
}

pub fn cache() -> PathBuf {
    root().join(".cache/sources")
}

pub fn library() -> PathBuf {
    root().join("data/library")
}
