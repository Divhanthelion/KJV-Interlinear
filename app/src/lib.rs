//! KJV Interlinear app: serves the web UI in `../ui` and answers its commands.

mod secrets;

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use kjv_core::bundle::DataBundle;
use kjv_ai::assistant::{AskArgs, ModelsArgs};
use kjv_ai::conversations::Conversations;
use kjv_ai::{Event, ModelInfo};
use kjv_core::dispatch::dispatch_all;
use kjv_library::Library;
use serde_json::Value;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tokio::sync::Notify;

use secrets::{KeyStatus, Secrets, Storage};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_opener::OpenerExt;

/// All text and interlinear data, compressed at build time (see build.rs).
static BUNDLE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/bundle.bin.zst"));
static DATA: OnceLock<DataBundle> = OnceLock::new();

/// Translations and commentaries, compressed per book (see kjv_library::archive).
static LIBRARY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/library.bin"));
static LIBRARY_DATA: OnceLock<Library> = OnceLock::new();

fn library() -> &'static Library {
    LIBRARY_DATA.get_or_init(|| Library::open(LIBRARY).expect("embedded library matches this build"))
}

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
    tauri::async_runtime::spawn_blocking(move || dispatch_all(data(), library(), &name, args))
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

// ---------------------------------------------------------------- AI chat

struct Ai {
    client: kjv_ai::Client,
    secrets: Secrets,
    /// Replies in progress, by the id the page gave them, so Stop can end them
    running: Mutex<HashMap<String, Arc<Notify>>>,
    /// Saved conversations, in the app's private config folder
    conversations: Conversations,
}

fn api_key(ai: &Ai, provider_id: &str) -> Result<Option<String>, String> {
    ai.secrets.get(provider_id)
}

/// The models a provider offers (with context sizes where it reports them).
#[tauri::command]
async fn ai_models(ai: State<'_, Ai>, args: ModelsArgs) -> Result<Vec<ModelInfo>, String> {
    let key = api_key(&ai, &args.provider_id)?;
    kjv_ai::models(&ai.client, &args.endpoint(key)).await
}

/// Stream an answer to `on_event`. Resolves when the answer ends or Stop is pressed.
#[tauri::command]
async fn ai_chat(ai: State<'_, Ai>, id: String, args: AskArgs, on_event: Channel<Event>) -> Result<(), String> {
    let stop = Arc::new(Notify::new());
    ai.running.lock().unwrap().insert(id.clone(), stop.clone());
    let result = async {
        let key = api_key(&ai, &args.provider_id)?;
        let request = tauri::async_runtime::spawn_blocking(move || kjv_ai::assistant::prepare(data(), &args, key))
            .await
            .map_err(|e| e.to_string())??;
        let send = |event: Event| {
            let _ = on_event.send(event);
        };
        tokio::select! {
            result = kjv_ai::chat(&ai.client, &request, send) => result,
            // Dropping the request closes the connection, so the server stops generating
            () = stop.notified() => {
                let _ = on_event.send(Event::Done { reason: Some("cancelled".into()) });
                Ok(())
            }
        }
    }
    .await;
    ai.running.lock().unwrap().remove(&id);
    result
}

#[tauri::command]
fn ai_cancel(ai: State<'_, Ai>, id: String) {
    if let Some(stop) = ai.running.lock().unwrap().get(&id) {
        // Stored as a permit if the reply hasn't started waiting yet
        stop.notify_one();
    }
}

#[tauri::command(async)]
fn conversations_list(ai: State<'_, Ai>) -> Result<Vec<Value>, String> {
    ai.conversations.list()
}

#[tauri::command(async)]
fn conversation_load(ai: State<'_, Ai>, id: String) -> Result<Value, String> {
    ai.conversations.load(&id)
}

#[tauri::command(async)]
fn conversation_save(ai: State<'_, Ai>, conversation: Value) -> Result<(), String> {
    ai.conversations.save(&conversation)
}

#[tauri::command(async)]
fn conversation_delete(ai: State<'_, Ai>, id: String) -> Result<(), String> {
    ai.conversations.delete(&id)
}

#[tauri::command]
fn ai_key_status(ai: State<'_, Ai>, provider_id: String) -> Result<KeyStatus, String> {
    ai.secrets.status(&provider_id)
}

#[tauri::command]
fn ai_key_set(ai: State<'_, Ai>, provider_id: String, key: String) -> Result<Storage, String> {
    ai.secrets.set(&provider_id, &key)
}

#[tauri::command]
fn ai_key_delete(ai: State<'_, Ai>, provider_id: String) -> Result<(), String> {
    ai.secrets.delete(&provider_id)
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
        .setup(|app| {
            // Decompress the data while the window loads
            std::thread::spawn(|| {
                data();
            });
            let dir = app.path().app_config_dir()?;
            app.manage(Ai {
                client: kjv_ai::client(),
                conversations: Conversations::new(dir.join("conversations")),
                secrets: Secrets::new(dir),
                running: Mutex::default(),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            call,
            settings_load,
            settings_save,
            copy_to_clipboard,
            open_url,
            ai_models,
            ai_chat,
            ai_cancel,
            ai_key_status,
            ai_key_set,
            ai_key_delete,
            conversations_list,
            conversation_load,
            conversation_save,
            conversation_delete
        ])
        .run(tauri::generate_context!())
        .expect("error while running KJV Interlinear");
}
