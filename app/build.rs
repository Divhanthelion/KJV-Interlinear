//! Builds the data bundle from the repository's text and data files, compresses it,
//! and hands it to the app via OUT_DIR (embedded with `include_bytes!`).

use std::env;
use std::fs;
use std::path::PathBuf;

use kjv_core::bundle::DataBundle;

fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("..");
    for dir in ["old_testament", "new_testament", "data"] {
        println!("cargo:rerun-if-changed={}", root.join(dir).display());
    }
    println!("cargo:rerun-if-changed=build.rs");

    let bundle = DataBundle::from_sources(&root).expect("build the data bundle");
    let bytes = bundle.to_bytes().expect("serialize the data bundle");
    // Maximum compression for shipped builds; fast for development
    let level = if env::var("PROFILE").as_deref() == Ok("release") { 19 } else { 3 };
    let compressed = zstd::encode_all(&bytes[..], level).expect("compress the data bundle");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("bundle.bin.zst");
    fs::write(&out, compressed).expect("write the data bundle");

    tauri_build::build();
}
