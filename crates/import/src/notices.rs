//! The notices of the open-source software the app is built from: every package in
//! the app's dependency graph (for every platform it builds on), with its licence and
//! the licence texts it ships, so the app can carry them as their licences require.
//!
//!     cargo run -p kjv-import -- notices        write NOTICE, ui/software.json, and THIRD-PARTY-SOFTWARE.md
//!     cargo run -p kjv-import -- notices check  fail if they are out of date
//!
//! NOTICE lists every work the app carries (from the catalogues in data/library/, with
//! each one's licence and credit) after the sections in licenses/NOTICE-fixed.txt.
//!
//! Packages come from `cargo metadata` for app/Cargo.toml; their texts from the files
//! they publish (LICENSE*, LICENCE*, COPYING*, COPYRIGHT*, NOTICE*, UNLICENSE*, and a
//! LICENSES/ folder). A package that publishes none is given the standard text of its
//! licence (the first of MIT, Apache-2.0, and the rest that it offers) from
//! crates/import/licenses/, under a copyright line naming its authors.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_members: Vec<String>,
}

#[derive(Deserialize)]
struct Package {
    id: String,
    name: String,
    version: String,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    authors: Vec<String>,
    #[serde(default)]
    repository: Option<String>,
    #[serde(default)]
    homepage: Option<String>,
    manifest_path: PathBuf,
}

#[derive(Serialize)]
struct Software {
    /// One entry per package
    packages: Vec<Entry>,
    /// Every distinct licence text, which entries refer to by number
    texts: Vec<String>,
}

#[derive(Serialize)]
struct Entry {
    name: String,
    version: String,
    license: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    texts: Vec<usize>,
}

/// Standard texts for packages that publish none
const STANDARD: &[(&str, &str)] = &[
    ("MIT", include_str!("../licenses/MIT.txt")),
    ("Apache-2.0", include_str!("../licenses/Apache-2.0.txt")),
    ("BSD-3-Clause", include_str!("../licenses/BSD-3-Clause.txt")),
    ("Zlib", include_str!("../licenses/Zlib.txt")),
    ("MPL-2.0", include_str!("../licenses/MPL-2.0.txt")),
    ("BSL-1.0", include_str!("../licenses/BSL-1.0.txt")),
];

/// Licences that need the copyright line the standard text leaves out
const NEEDS_COPYRIGHT: &[&str] = &["MIT", "BSD-3-Clause", "Zlib"];

fn is_licence_file(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    ["LICENSE", "LICENCE", "COPYING", "COPYRIGHT", "NOTICE", "UNLICENSE"].iter().any(|p| n.starts_with(p))
}

/// The licence texts package directory `dir` publishes, in name order.
fn texts_in(dir: &Path) -> Result<Vec<String>, String> {
    let mut files: Vec<PathBuf> = Vec::new();
    let entries = fs::read_dir(dir).map_err(|e| format!("{}: {} (run `cargo fetch --manifest-path app/Cargo.toml`)", dir.display(), e))?;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let path = e.path();
        if path.is_file() && is_licence_file(&name) {
            files.push(path);
        } else if path.is_dir() && name.eq_ignore_ascii_case("LICENSES") {
            for f in fs::read_dir(&path).map_err(|e| e.to_string())?.flatten() {
                if f.path().is_file() {
                    files.push(f.path());
                }
            }
        }
    }
    files.sort();
    files
        .iter()
        .map(|f| {
            let bytes = fs::read(f).map_err(|e| format!("{}: {}", f.display(), e))?;
            Ok(String::from_utf8_lossy(&bytes).replace("\r\n", "\n").trim().to_string() + "\n")
        })
        .collect()
}

/// The standard text for a package that publishes none: the first licence it offers
/// that we have a text for, MIT and Apache-2.0 first.
fn standard_text(p: &Package) -> Result<String, String> {
    let expr = p.license.clone().unwrap_or_default();
    let offered: Vec<&str> = expr.split(|c: char| c == '/' || c == '(' || c == ')' || c.is_whitespace()).filter(|t| !t.is_empty() && *t != "OR" && *t != "AND" && *t != "WITH").collect();
    let order = ["MIT", "Apache-2.0", "BSD-3-Clause", "Zlib", "BSL-1.0", "MPL-2.0"];
    let chosen = order.iter().find(|o| offered.contains(o)).ok_or_else(|| format!("{} {}: no licence file, and no standard text for {:?}", p.name, p.version, expr))?;
    let text = STANDARD.iter().find(|(n, _)| n == chosen).map(|(_, t)| *t).unwrap();
    if NEEDS_COPYRIGHT.contains(chosen) {
        let who = if p.authors.is_empty() { format!("the {} authors", p.name) } else { p.authors.join(", ") };
        Ok(format!("{} (as offered by {} {}; it publishes no licence file)\n\nCopyright (c) {}\n\n{}", chosen, p.name, p.version, who, text))
    } else {
        Ok(format!("{} (as offered by {} {}; it publishes no licence file)\n\n{}", chosen, p.name, p.version, text))
    }
}

fn collect(root: &Path) -> Result<Software, String> {
    let out = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--locked", "--manifest-path"])
        .arg(root.join("app/Cargo.toml"))
        .output()
        .map_err(|e| format!("cargo metadata: {}", e))?;
    if !out.status.success() {
        return Err(format!("cargo metadata: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let meta: Metadata = serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata: {}", e))?;
    let mut packages: Vec<&Package> = meta.packages.iter().filter(|p| !meta.workspace_members.contains(&p.id)).collect();
    packages.sort_by(|a, b| (a.name.as_str(), &a.version).cmp(&(b.name.as_str(), &b.version)));
    packages.dedup_by(|a, b| a.name == b.name && a.version == b.version);
    let mut texts: Vec<String> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut entries = Vec::new();
    for p in packages {
        let dir = p.manifest_path.parent().ok_or("bad manifest path")?;
        let mut found = texts_in(dir)?;
        if found.is_empty() {
            found.push(standard_text(p)?);
        }
        let numbers = found
            .into_iter()
            .map(|t| {
                *index.entry(t.clone()).or_insert_with(|| {
                    texts.push(t);
                    texts.len() - 1
                })
            })
            .collect();
        entries.push(Entry {
            name: p.name.clone(),
            version: p.version.clone(),
            license: p.license.clone().unwrap_or_else(|| "see its licence text".into()),
            url: p.repository.clone().or_else(|| p.homepage.clone()),
            texts: numbers,
        });
    }
    Ok(Software { packages: entries, texts })
}

/// The same notices as a document: each distinct text once, after the packages under it.
fn markdown(s: &Software) -> String {
    let mut out = String::from(
        "# Third-party software\n\n\
         The app is built from the open-source packages below (for every platform it builds on). \
         Each is listed with its version and licence, followed by the licence texts it publishes. \
         This file is generated by `cargo run -p kjv-import -- notices`; the app shows the same \
         list under Settings → Licences.\n",
    );
    let mut users: Vec<Vec<&Entry>> = vec![Vec::new(); s.texts.len()];
    for e in &s.packages {
        for &t in &e.texts {
            users[t].push(e);
        }
    }
    // Texts in the order their first package appears
    let mut order: Vec<usize> = (0..s.texts.len()).collect();
    order.sort_by_key(|&t| s.packages.iter().position(|e| e.texts.contains(&t)).unwrap_or(usize::MAX));
    for t in order {
        out.push_str("\n---\n\n");
        for e in &users[t] {
            out.push_str(&format!("- {} {} ({})", e.name, e.version, e.license));
            if let Some(u) = &e.url {
                out.push_str(&format!(" <{}>", u));
            }
            out.push('\n');
        }
        out.push_str("\n```text\n");
        out.push_str(&s.texts[t].replace("```", "'''"));
        out.push_str("```\n");
    }
    out
}

#[derive(Deserialize)]
struct Bibles {
    bible: Vec<Work>,
}

#[derive(Deserialize)]
struct Commentaries {
    commentary: Vec<Work>,
}

#[derive(Deserialize)]
struct Crossrefs {
    crossrefs: Vec<Work>,
}

/// A translation, commentary, or cross-reference collection, as its catalogue lists it.
#[derive(Deserialize)]
struct Work {
    name: String,
    licence: String,
    credit: String,
    #[serde(default)]
    abbr: Option<String>,
    #[serde(default)]
    year: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    ebible: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

/// What each licence is called, where it is, and what it asks, in the order NOTICE gives them.
pub const LICENCES: &[(&str, &str, &str, &str)] = &[
    ("pd", "Public domain", "", "Free of copyright: no conditions."),
    ("cc-by-4.0", "Creative Commons Attribution 4.0 (CC BY 4.0)", "https://creativecommons.org/licenses/by/4.0/", "Free to share and adapt, with credit and a note of any changes."),
    (
        "cc-by-sa-4.0",
        "Creative Commons Attribution-ShareAlike 4.0 (CC BY-SA 4.0)",
        "https://creativecommons.org/licenses/by-sa/4.0/",
        "Free to share and adapt, with credit and a note of any changes; adaptations under the same licence.",
    ),
    ("cc-by-nd-4.0", "Creative Commons Attribution-NoDerivatives 4.0 (CC BY-ND 4.0)", "https://creativecommons.org/licenses/by-nd/4.0/", "Free to share unaltered, with credit."),
    (
        "cc-by-nc-nd-4.0",
        "Creative Commons Attribution-NonCommercial-NoDerivatives 4.0 (CC BY-NC-ND 4.0)",
        "https://creativecommons.org/licenses/by-nc-nd/4.0/",
        "Free to share unaltered, with credit, and not for commercial use.",
    ),
];

fn section(title: &str) -> String {
    format!("\n---------------------------------------------------------------------------\n{}\n---------------------------------------------------------------------------\n", title)
}

/// The works of one catalogue, grouped by licence.
fn works(out: &mut String, title: &str, list: &[Work]) -> Result<(), String> {
    out.push_str(&section(title));
    for w in list {
        if !LICENCES.iter().any(|l| l.0 == w.licence) {
            return Err(format!("{}: unknown licence {:?}", w.name, w.licence));
        }
    }
    for (key, name, url, asks) in LICENCES {
        let mine: Vec<&Work> = list.iter().filter(|w| w.licence == *key).collect();
        if mine.is_empty() {
            continue;
        }
        out.push_str(&format!("\n{}{}\n{}\n\n", name, if url.is_empty() { String::new() } else { format!(" {}", url) }, asks));
        for w in mine {
            let mut head = match (&w.abbr, &w.author) {
                (Some(a), _) => format!("{} ({})", w.name, a),
                (None, Some(a)) if a != &w.name => format!("{} by {}", w.name, a),
                _ => w.name.clone(),
            };
            if let Some(y) = &w.year {
                head.push_str(&format!(", {}", y));
            }
            let source = match (&w.ebible, &w.url) {
                (Some(id), _) => format!("https://ebible.org/find/details.php?id={}", id),
                (None, Some(u)) => u.clone(),
                _ => String::new(),
            };
            out.push_str(&format!("- {}\n  {}\n", head, w.credit));
            if !source.is_empty() {
                out.push_str(&format!("  Source: {}\n", source));
            }
        }
    }
    Ok(())
}

/// NOTICE: the app's own licence, then every work it carries and its terms.
fn notice(root: &Path) -> Result<String, String> {
    let read = |rel: &str| fs::read_to_string(root.join(rel)).map_err(|e| format!("{}: {}", rel, e)).map(|t| t.replace("\r\n", "\n"));
    let bibles: Bibles = toml::from_str(&read("data/library/bibles.toml")?).map_err(|e| e.to_string())?;
    let commentaries: Commentaries = toml::from_str(&read("data/library/commentaries.toml")?).map_err(|e| e.to_string())?;
    let crossrefs: Crossrefs = toml::from_str(&read("data/library/crossrefs.toml")?).map_err(|e| e.to_string())?;
    let mut out = String::from(
        "Scriptorium\n\
         Copyright 2025-2026 Divhanthelion\n\
         \n\
         The app's own code is licensed under MIT No Attribution (see LICENSE). Everything\n\
         below comes with it under its own terms, which that licence doesn't change: each\n\
         work is listed with its licence and the credit it asks for. The open-source\n\
         software the app is built from, and its licence texts, are in\n\
         THIRD-PARTY-SOFTWARE.md. The app shows all of this under Settings, Licences.\n\
         \n\
         This file is generated by `cargo run -p kjv-import -- notices` from the\n\
         catalogues in data/library/ and crates/import/licenses/NOTICE-fixed.txt.\n",
    );
    works(&mut out, "Bible translations", &bibles.bible)?;
    works(&mut out, "Commentaries", &commentaries.commentary)?;
    works(&mut out, "Cross-references", &crossrefs.crossrefs)?;
    out.push('\n');
    out.push_str(include_str!("../licenses/NOTICE-fixed.txt"));
    Ok(out)
}

/// Write the notices (or with `check`, fail if the written ones differ).
pub fn run(args: &[String]) -> Result<(), String> {
    let root = crate::root();
    let s = collect(&root)?;
    let json = serde_json::to_string(&s).map_err(|e| e.to_string())? + "\n";
    let md = markdown(&s);
    let files = [(root.join("NOTICE"), notice(&root)?), (root.join("ui/software.json"), json), (root.join("THIRD-PARTY-SOFTWARE.md"), md)];
    if args.first().map(String::as_str) == Some("check") {
        for (path, text) in &files {
            let have = fs::read_to_string(path).unwrap_or_default().replace("\r\n", "\n");
            if &have != text {
                return Err(format!("{} is out of date: run `cargo run -p kjv-import -- notices`", path.display()));
            }
        }
        println!("notices: up to date ({} packages, {} texts)", s.packages.len(), s.texts.len());
        return Ok(());
    }
    for (path, text) in &files {
        fs::write(path, text).map_err(|e| format!("{}: {}", path.display(), e))?;
    }
    println!("notices: {} packages, {} distinct licence texts", s.packages.len(), s.texts.len());
    Ok(())
}
