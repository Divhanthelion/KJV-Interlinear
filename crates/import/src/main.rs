//! kjv-import: builds `data/library/` from pinned upstream sources.
//!
//!     cargo run -p kjv-import -- pin            record every source's SHA-256 (after review)
//!     cargo run -p kjv-import -- fetch          download missing sources, verify every hash
//!     cargo run -p kjv-import -- build [ids…]   convert sources into data/library/ (ids: translations,
//!                                               commentaries, "commentaries", "crossrefs")
//!     cargo run -p kjv-import -- check          rebuild in memory and compare with data/library/
//!     cargo run -p kjv-import -- inventory      list every markup element each commentary source uses
//!     cargo run -p kjv-import -- notices        write the app's third-party software notices
//!
//! Sources live in `.cache/sources/` (git-ignored); `data/library/sources.toml` pins
//! each one by URL, size, and SHA-256 so every build converts exactly the same bytes.

use std::process::ExitCode;

use kjv_import::{align, bibles, commentaries, crossrefs, notices, sources};

/// `build` with no ids converts everything, then aligns every translation with the KJV;
/// each id names a translation or a commentary ("commentaries" means all of them, and
/// "crossrefs" the cross-reference collections).
fn build(ids: &[String], mode: bibles::Mode) -> Result<(), String> {
    if ids.is_empty() {
        bibles::build(&[], mode)?;
        commentaries::build(&[], mode)?;
        crossrefs::build(mode)?;
        return align::build(&[], mode);
    }
    let known = commentaries::catalogue()?;
    let (mut translations, mut commentary_ids, mut all_commentaries) = (Vec::new(), Vec::new(), false);
    for id in ids {
        if id == "crossrefs" {
            crossrefs::build(mode)?;
        } else if id == "commentaries" {
            all_commentaries = true;
        } else if known.iter().any(|c| &c.id == id) {
            commentary_ids.push(id.clone());
        } else {
            translations.push(id.clone());
        }
    }
    if !translations.is_empty() {
        bibles::build(&translations, mode)?;
    }
    if all_commentaries {
        commentaries::build(&[], mode)?;
    } else if !commentary_ids.is_empty() {
        commentaries::build(&commentary_ids, mode)?;
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("pin") => sources::pin(),
        Some("fetch") => sources::fetch(),
        Some("build") => sources::verify().and_then(|_| build(&args[1..], bibles::Mode::Write)),
        Some("check") => sources::verify().and_then(|_| build(&[], bibles::Mode::Check)),
        Some("align") => align::build(&args[1..], bibles::Mode::Write),
        Some("inventory") => commentaries::inventory(&args[1..]),
        Some("notices") => notices::run(&args[1..]),
        _ => Err("usage: kjv-import pin | fetch | build [ids…] | check | inventory [commentary ids…] | notices [check]".to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::FAILURE
        }
    }
}
