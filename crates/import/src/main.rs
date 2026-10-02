//! kjv-import: builds `data/library/` from pinned upstream sources.
//!
//!     cargo run -p kjv-import -- pin            record every source's SHA-256 (after review)
//!     cargo run -p kjv-import -- fetch          download missing sources, verify every hash
//!     cargo run -p kjv-import -- build [ids…]   convert sources into data/library/
//!     cargo run -p kjv-import -- check          rebuild in memory and compare with data/library/
//!
//! Sources live in `.cache/sources/` (git-ignored); `data/library/sources.toml` pins
//! each one by URL, size, and SHA-256 so every build converts exactly the same bytes.

mod bibles;
mod sources;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("repository root")
}

pub fn cache() -> PathBuf {
    root().join(".cache/sources")
}

pub fn library() -> PathBuf {
    root().join("data/library")
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("pin") => sources::pin(),
        Some("fetch") => sources::fetch(),
        Some("build") => sources::verify().and_then(|_| bibles::build(&args[1..], bibles::Mode::Write)),
        Some("check") => sources::verify().and_then(|_| bibles::build(&[], bibles::Mode::Check)),
        _ => Err("usage: kjv-import pin | fetch | build [ids…] | check".to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::FAILURE
        }
    }
}
