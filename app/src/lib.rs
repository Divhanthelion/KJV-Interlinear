//! KJV Interlinear app: serves the web UI in `../ui` and answers its commands.

use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::OnceLock;

use kjv_core::bundle::DataBundle;
use kjv_core::dispatch::dispatch;
use serde_json::Value;
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_opener::OpenerExt;

/// All text and interlinear data, compressed at build time (see build.rs).
static BUNDLE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/bundle.bin.zst"));
static DATA: OnceLock<DataBundle> = OnceLock::new();

fn data() -> &'static DataBundle {
    DATA.get_or_init(|| {
        let mut bytes = Vec::new();
        ruzstd::decoding::StreamingDecoder::new(BUNDLE)
            .expect("embedded data is valid zstd")
            .read_to_end(&mut bytes)
            .expect("embedded data decompresses");
        DataBundle::from_bytes(&bytes).expect("embedded data matches this build")
    })
}

/// Run a data command (books, chapter, search, strongs, lexicon, copy_text).
#[tauri::command]
async fn call(name: String, args: Value) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || dispatch(data(), &name, args))
        .await
        .map_err(|e| e.to_string())?
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| format!("create {}: {}", dir.display(), e))?;
    Ok(dir.join("settings.json"))
}

/// Saved settings, or null on first run. The UI validates every field.
#[tauri::command]
fn settings_load(app: AppHandle) -> Result<Value, String> {
    let path = settings_path(&app)?;
    match fs::read_to_string(&path) {
        Ok(text) => Ok(serde_json::from_str(text.trim_start_matches('\u{FEFF}')).unwrap_or(Value::Null)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Value::Null),
        Err(e) => Err(format!("read {}: {}", path.display(), e)),
    }
}

/// Write settings atomically (temp file, then rename) so a crash can't leave half a file.
#[tauri::command]
fn settings_save(app: AppHandle, settings: Value) -> Result<(), String> {
    let path = settings_path(&app)?;
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    fs::write(&tmp, text).map_err(|e| format!("write {}: {}", tmp.display(), e))?;
    fs::rename(&tmp, &path).map_err(|e| format!("replace {}: {}", path.display(), e))
}

#[tauri::command]
fn copy_to_clipboard(app: AppHandle, text: String) -> Result<(), String> {
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

/// Sites the About section links to; nothing else can be opened.
const ALLOWED_LINKS: [&str; 3] = [
    "https://ebible.org/",
    "https://www.stepbible.org/",
    "https://github.com/Divhanthelion/KJV-Interlinear",
];

#[tauri::command]
fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    if !ALLOWED_LINKS.iter().any(|prefix| url.starts_with(prefix)) {
        return Err(format!("not an allowed link: {}", url));
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|_| {
            // Decompress the data while the window loads
            std::thread::spawn(|| {
                data();
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            call,
            settings_load,
            settings_save,
            copy_to_clipboard,
            open_url
        ])
        .run(tauri::generate_context!())
        .expect("error while running KJV Interlinear");
}
