//! Builds the data bundle from the repository's text and data files, compresses it,
//! and hands it to the app via OUT_DIR (embedded with `include_bytes!`).

use std::env;
use std::fs;
use std::io::Read;
use std::path::PathBuf;

use kjv_core::bundle::DataBundle;
use sha2::{Digest, Sha256};

fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("..");
    for dir in ["old_testament", "new_testament", "data"] {
        println!("cargo:rerun-if-changed={}", root.join(dir).display());
    }
    println!("cargo:rerun-if-changed=build.rs");

    // from_sources validates the bundle: an incomplete text fails the build here
    let bundle = DataBundle::from_sources(&root).expect("build the data bundle");
    let bytes = bundle.to_bytes().expect("serialize the data bundle");
    // Maximum compression for shipped builds; fast for development
    let release = env::var("PROFILE").as_deref() == Ok("release");
    let level = if release { 19 } else { 3 };
    let compressed = zstd::encode_all(&bytes[..], level).expect("compress the data bundle");

    // The app unpacks with ruzstd, not the zstd library that packed it: prove they agree
    let mut unpacked = Vec::with_capacity(bytes.len());
    ruzstd::decoding::StreamingDecoder::new(&compressed[..])
        .expect("the compressed bundle is valid zstd")
        .read_to_end(&mut unpacked)
        .expect("the compressed bundle decompresses");
    assert!(unpacked == bytes, "the compressed bundle does not decompress to the original bytes");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("bundle.bin.zst");
    fs::write(&out, &compressed).expect("write the data bundle");

    // Identifies the exact data a build embeds. Not reproducible across builds:
    // the bundle's hash maps serialize in their (randomized) in-memory order.
    let hash: String = Sha256::digest(&compressed).iter().map(|b| format!("{:02x}", b)).collect();
    let summary = format!(
        "data bundle: {} bytes, zstd level {} -> {} bytes, sha256 {}",
        bytes.len(),
        level,
        compressed.len(),
        hash
    );
    // Shown in the build output for shipped builds; kept in the build script's log otherwise
    if release {
        println!("cargo:warning={}", summary);
    } else {
        println!("{}", summary);
    }

    tauri_build::build();
}
