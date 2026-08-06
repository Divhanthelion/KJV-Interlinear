//! Resolve asset directories for development and bundled builds.

use std::path::{Path, PathBuf};

/// Candidate roots: cwd, executable directory, and parents of the executable
/// (covers `target/release/` layouts and macOS `.app` Resources siblings).
pub fn asset_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.to_path_buf());
            // target/release -> project root
            if let Some(parent) = dir.parent() {
                roots.push(parent.to_path_buf());
                if let Some(grand) = parent.parent() {
                    roots.push(grand.to_path_buf());
                }
            }
            // macOS: App.app/Contents/MacOS -> Resources
            if dir.ends_with("MacOS") {
                if let Some(contents) = dir.parent() {
                    roots.push(contents.join("Resources"));
                }
            }
        }
    }

    roots
}

/// Find a directory that exists under one of the asset roots.
pub fn find_dir(name: &str) -> Option<PathBuf> {
    for root in asset_roots() {
        let candidate = root.join(name);
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    None
}

/// Find a file that exists under one of the asset roots.
pub fn find_file(relative: &str) -> Option<PathBuf> {
    for root in asset_roots() {
        let candidate = root.join(relative);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub fn old_testament_dir() -> Option<PathBuf> {
    find_dir("old_testament")
}

pub fn new_testament_dir() -> Option<PathBuf> {
    find_dir("new_testament")
}

pub fn data_dir() -> Option<PathBuf> {
    find_dir("data")
}

pub fn red_letter_path() -> Option<PathBuf> {
    find_file("data/red_letter_verses.json")
}

/// ProjectDirs for settings + cache (must stay in sync with settings.rs).
pub fn project_dirs() -> Option<directories::ProjectDirs> {
    directories::ProjectDirs::from("com", "divhanthelion", "kjv-interlinear")
}

#[allow(dead_code)]
pub fn exists_under(root: &Path, name: &str) -> bool {
    root.join(name).exists()
}
