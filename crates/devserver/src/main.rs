//! Browser preview of the app: serves `ui/` and answers `POST /api/<command>`
//! with the same dispatcher the app uses. Settings and API keys are kept in memory.
//! AI replies stream back as one JSON event per line.
//!
//! Only its own pages may call `/api/`: a request must name localhost or 127.0.0.1 at
//! this server's port as its Host (a DNS-rebinding page names its own), come from no
//! Origin or this server's own, and be a JSON POST (which a page on another site
//! can't send without the browser asking this server first).
//!
//! Run from the repository root: `cargo run -p kjv-devserver -- [port]`

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use kjv_ai::Event;
use kjv_ai::assistant::{AskArgs, ModelsArgs};
use kjv_ai::conversations::Conversations;
use kjv_ai::keys;
use kjv_core::bundle::DataBundle;
use kjv_core::dispatch::dispatch;
use serde::Deserialize;
use serde_json::{Value, json};
use tiny_http::{Header, Method, Request, Response, Server};
use tokio::sync::Notify;

/// Largest request body accepted; a long conversation is far smaller
const MAX_BODY: usize = 8 * 1024 * 1024;

/// What `ai_key_status` and `ai_key_set` say keys are kept in
const STORAGE: &str = "memory";

const STREAM_HEAD: &str = "HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nCache-Control: no-store\r\n\
                           Transfer-Encoding: chunked\r\nConnection: close\r\n\r\n";

struct State {
    ui: PathBuf,
    /// The port the server listens on, for checking Host and Origin
    port: u16,
    data: Arc<DataBundle>,
    settings: Mutex<Value>,
    /// Stored as the app stores them: bound to a server (see `kjv_ai::keys`)
    keys: Mutex<HashMap<String, String>>,
    running: Arc<Mutex<HashMap<String, Arc<Notify>>>>,
    runtime: tokio::runtime::Runtime,
    client: kjv_ai::Client,
    conversations: Conversations,
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ttf") => "font/ttf",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("valid header")
}

/// Map a URL path to a file under `root`, refusing anything that escapes it.
fn static_path(root: &Path, url: &str) -> Option<PathBuf> {
    let path = url.split('?').next().unwrap_or("/");
    let rel = Path::new(path.trim_start_matches('/'));
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }
    let full = if path == "/" { root.join("index.html") } else { root.join(rel) };
    full.is_file().then_some(full)
}

fn main() {
    let root = std::env::current_dir().expect("current directory");
    let port: u16 = std::env::args().nth(1).and_then(|p| p.parse().ok()).unwrap_or(1420);

    let server = Server::http(("127.0.0.1", port)).expect("port is free");
    // The port really bound (asking for 0 picks a free one)
    let port = server.server_addr().to_ip().map_or(port, |a| a.port());

    eprintln!("Loading data…");
    let state = Arc::new(State {
        ui: root.join("ui"),
        port,
        data: Arc::new(DataBundle::from_sources(&root).expect("run from the repository root")),
        settings: Mutex::new(Value::Null),
        keys: Mutex::default(),
        running: Arc::default(),
        runtime: tokio::runtime::Runtime::new().expect("async runtime"),
        client: kjv_ai::client(),
        // A fresh folder each run, like the in-memory settings
        conversations: Conversations::new(
            std::env::temp_dir().join(format!("kjv-devserver-conversations-{}", std::process::id())),
        ),
    });
    eprintln!("Preview at http://localhost:{}", port);

    for request in server.incoming_requests() {
        let state = state.clone();
        // A reply can stream for minutes; don't hold up everything else
        std::thread::spawn(move || handle(&state, request));
    }
}

fn handle(state: &State, mut request: Request) {
    let url = request.url().to_string();
    let Some(name) = url.strip_prefix("/api/").map(str::to_string) else {
        let response = if request.method() == &Method::Get {
            match static_path(&state.ui, &url).and_then(|p| std::fs::read(&p).ok().map(|b| (p, b))) {
                Some((path, bytes)) => Response::from_data(bytes)
                    .with_header(header("Content-Type", content_type(&path)))
                    .with_header(header("Cache-Control", "no-store")),
                None => Response::from_string("not found").with_status_code(404),
            }
        } else {
            Response::from_string("method not allowed").with_status_code(405)
        };
        let _ = request.respond(response);
        return;
    };
    let checked =
        check_api_request(request.method(), request.headers(), state.port).and_then(|()| read_body(&mut request));
    let body = match checked {
        Ok(body) => body,
        Err((status, message)) => {
            let response =
                Response::from_string(message).with_status_code(status).with_header(header("Connection", "close"));
            let _ = request.respond(response);
            return;
        }
    };
    let args: Value = serde_json::from_str(&body).unwrap_or(Value::Null);

    if name == "ai_chat" {
        // Written by hand, one chunk per event, flushed at once: tiny_http buffers
        // the bodies it encodes itself
        let (events, stop) = chat(state, args);
        let mut out = request.into_writer();
        let mut write = |bytes: &[u8]| out.write_all(bytes).and_then(|()| out.flush());
        // The page went away: stop the model too, rather than let it run on unread
        let page_gone = || {
            if let Some(stop) = &stop {
                stop.notify_one();
            }
        };
        if write(STREAM_HEAD.as_bytes()).is_err() {
            return page_gone();
        }
        for line in events {
            if write(&chunk(&line)).is_err() {
                return page_gone();
            }
        }
        let _ = write(b"0\r\n\r\n");
        return;
    }
    let result = match name.as_str() {
        "settings_load" => Ok(state.settings.lock().unwrap().clone()),
        "settings_save" => {
            *state.settings.lock().unwrap() = args;
            Ok(Value::Null)
        }
        "ai_models" => {
            // A key typed into the provider form, tried before it's saved: used as
            // is (empty means none), never stored, and the saved keys aren't looked at
            let typed = args.get("apiKey").and_then(Value::as_str).map(str::to_string);
            parse::<ModelsArgs>(args).and_then(|a| {
                let key = match typed {
                    Some(typed) => Some(typed.trim().to_string()).filter(|k| !k.is_empty()),
                    None => saved_key(state, &a.provider_id, &a.base_url)?,
                };
                let list = state.runtime.block_on(kjv_ai::models(&state.client, &a.endpoint(key)))?;
                Ok(json!(list))
            })
        }
        "conversations_list" => state.conversations.list().map(Value::from),
        "conversation_load" => parse::<IdArgs>(args).and_then(|a| state.conversations.load(&a.id)),
        "conversation_save" => {
            parse::<SaveArgs>(args).and_then(|a| state.conversations.save(&a.conversation).map(|()| Value::Null))
        }
        "conversation_delete" => {
            parse::<IdArgs>(args).and_then(|a| state.conversations.delete(&a.id).map(|()| Value::Null))
        }
        "ai_cancel" => parse::<IdArgs>(args).map(|a| {
            if let Some(stop) = state.running.lock().unwrap().get(&a.id) {
                stop.notify_one();
            }
            Value::Null
        }),
        "ai_key_status" => parse::<KeyArgs>(args).map(|a| {
            let stored = state.keys.lock().unwrap().get(&a.provider_id).cloned();
            json!({
                "stored": stored.is_some(),
                "storage": STORAGE,
                "origin": stored.as_deref().and_then(keys::bound_origin),
            })
        }),
        "ai_key_set" => parse::<KeyArgs>(args).and_then(|a| {
            let key = a.key.unwrap_or_default().trim().to_string();
            if key.is_empty() {
                state.keys.lock().unwrap().remove(&a.provider_id);
                return Ok(json!(STORAGE));
            }
            if keys::is_reserved_id(&a.provider_id) {
                return Err(format!("Keys can't be saved under {:?}.", a.provider_id));
            }
            // Bound to the server when the page says which; otherwise on first use
            let stored = match a.base_url.as_deref() {
                Some(base) => keys::bind(&key, base)?,
                None => key,
            };
            state.keys.lock().unwrap().insert(a.provider_id, stored);
            Ok(json!(STORAGE))
        }),
        "ai_key_delete" => parse::<KeyArgs>(args).map(|a| {
            state.keys.lock().unwrap().remove(&a.provider_id);
            Value::Null
        }),
        _ => dispatch(&state.data, &name, args),
    };
    let response = match result {
        Ok(value) => Response::from_string(value.to_string()).with_header(header("Content-Type", "application/json")),
        Err(message) => Response::from_string(message).with_status_code(400),
    };
    let _ = request.respond(response);
}

/// Refuse `/api/` requests that don't come from this server's own pages.
fn check_api_request(method: &Method, headers: &[Header], port: u16) -> Result<(), (u16, &'static str)> {
    if method != &Method::Post {
        return Err((405, "API calls are POST requests."));
    }
    let get = |name: &str| {
        headers.iter().find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name)).map(|h| h.value.as_str().trim())
    };
    if let Some(host) = get("Host")
        && !own_address(host, port, "")
    {
        return Err((403, "This server only answers requests to localhost."));
    }
    if let Some(origin) = get("Origin")
        && !own_address(origin, port, "http://")
    {
        return Err((403, "This server only answers its own pages."));
    }
    match get("Content-Type") {
        Some(media) if media.split(';').next().unwrap_or("").trim().eq_ignore_ascii_case("application/json") => Ok(()),
        _ => Err((415, "API calls must send Content-Type: application/json.")),
    }
}

/// `localhost:PORT` or `127.0.0.1:PORT` after `scheme` (the port may be left out when
/// it's 80, as browsers do).
fn own_address(value: &str, port: u16, scheme: &str) -> bool {
    let value = value.to_ascii_lowercase();
    let Some(rest) = value.strip_prefix(scheme) else {
        return false;
    };
    ["localhost", "127.0.0.1"].iter().any(|host| {
        rest.strip_prefix(host).is_some_and(|after| after == format!(":{}", port) || (port == 80 && after.is_empty()))
    })
}

fn read_body(request: &mut Request) -> Result<String, (u16, &'static str)> {
    const TOO_LARGE: (u16, &str) = (413, "The request is too large.");
    if request.body_length().is_some_and(|n| n > MAX_BODY) {
        return Err(TOO_LARGE);
    }
    let mut bytes = Vec::new();
    request
        .as_reader()
        .take(MAX_BODY as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| (400, "The request couldn't be read."))?;
    if bytes.len() > MAX_BODY {
        return Err(TOO_LARGE);
    }
    String::from_utf8(bytes).map_err(|_| (400, "The request isn't UTF-8 text."))
}

/// One chunk of a chunked HTTP body.
fn chunk(data: &[u8]) -> Vec<u8> {
    let mut out = format!("{:x}\r\n", data.len()).into_bytes();
    out.extend_from_slice(data);
    out.extend_from_slice(b"\r\n");
    out
}

/// The saved key for a request to `base_url`: refused if it was saved for another
/// server. An unbound key is bound to this one.
fn saved_key(state: &State, provider_id: &str, base_url: &str) -> Result<Option<String>, String> {
    let mut saved = state.keys.lock().unwrap();
    let Some(stored) = saved.get(provider_id).cloned() else {
        return Ok(None);
    };
    let found = keys::unlock(&stored, base_url)?;
    if let Some(bound) = found.rebind {
        saved.insert(provider_id.to_string(), bound);
    }
    Ok(Some(found.key))
}

#[derive(Deserialize)]
struct IdArgs {
    id: String,
}

#[derive(Deserialize)]
struct SaveArgs {
    conversation: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyArgs {
    provider_id: String,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    base_url: Option<String>,
}

#[derive(Deserialize)]
struct ChatArgs {
    id: String,
    args: AskArgs,
}

fn parse<T: for<'de> Deserialize<'de>>(args: Value) -> Result<T, String> {
    serde_json::from_value(args).map_err(|e| format!("bad arguments: {}", e))
}

fn send_line(tx: &Sender<Vec<u8>>, value: Value) {
    let mut bytes = value.to_string().into_bytes();
    bytes.push(b'\n');
    let _ = tx.send(bytes);
}

/// Events as newline-delimited JSON; a failure ends with `{"type":"error"}`. The
/// receiver ends when the answer does. The `Notify` stops the answer early.
fn chat(state: &State, args: Value) -> (Receiver<Vec<u8>>, Option<Arc<Notify>>) {
    let (tx, reply) = channel::<Vec<u8>>();
    let a: ChatArgs = match parse(args) {
        Ok(a) => a,
        Err(e) => {
            send_line(&tx, json!({"type": "error", "message": e}));
            return (reply, None);
        }
    };
    let key = match saved_key(state, &a.args.provider_id, &a.args.base_url) {
        Ok(key) => key,
        Err(e) => {
            send_line(&tx, json!({"type": "error", "message": e}));
            return (reply, None);
        }
    };
    let stop = Arc::new(Notify::new());
    state.running.lock().unwrap().insert(a.id.clone(), stop.clone());
    let (data, client, running) = (state.data.clone(), state.client.clone(), state.running.clone());
    let stopped = stop.clone();
    state.runtime.spawn(async move {
        let result = async {
            let request = tokio::task::spawn_blocking(move || kjv_ai::assistant::prepare(&data, &a.args, key))
                .await
                .map_err(|e| e.to_string())??;
            tokio::select! {
                r = kjv_ai::chat(&client, &request, |event: Event| {
                    send_line(&tx, serde_json::to_value(event).unwrap());
                }) => r,
                // Dropping the request closes the connection, so the server stops generating
                () = stopped.notified() => {
                    send_line(&tx, json!({"type": "done", "reason": "cancelled"}));
                    Ok(())
                }
            }
        }
        .await;
        if let Err(message) = result {
            send_line(&tx, json!({"type": "error", "message": message}));
        }
        running.lock().unwrap().remove(&a.id);
        // Dropping `tx` here ends the response
    });
    (reply, Some(stop))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PORT: u16 = 1420;

    fn check(method: Method, headers: &[(&str, &str)]) -> Result<(), u16> {
        let headers: Vec<Header> = headers.iter().map(|(n, v)| header(n, v)).collect();
        check_api_request(&method, &headers, PORT).map_err(|(status, _)| status)
    }

    const JSON: (&str, &str) = ("Content-Type", "application/json");

    #[test]
    fn own_pages_and_local_tools_are_answered() {
        // The page, in a browser
        assert_eq!(
            check(Method::Post, &[("Host", "localhost:1420"), ("Origin", "http://localhost:1420"), JSON]),
            Ok(())
        );
        assert_eq!(
            check(Method::Post, &[("Host", "127.0.0.1:1420"), ("Origin", "http://127.0.0.1:1420"), JSON]),
            Ok(())
        );
        // Node's fetch sends no Origin
        assert_eq!(check(Method::Post, &[("Host", "localhost:1420"), JSON]), Ok(()));
        assert_eq!(
            check(Method::Post, &[("host", "LOCALHOST:1420"), ("content-type", "Application/JSON; charset=utf-8")]),
            Ok(())
        );
    }

    #[test]
    fn other_hosts_origins_methods_and_bodies_are_refused() {
        // DNS rebinding: the page's own name in Host
        assert_eq!(check(Method::Post, &[("Host", "evil.example:1420"), JSON]), Err(403));
        assert_eq!(check(Method::Post, &[("Host", "localhost:9999"), JSON]), Err(403));
        assert_eq!(check(Method::Post, &[("Host", "localhost"), JSON]), Err(403));
        assert_eq!(check(Method::Post, &[("Host", "localhost:14201"), JSON]), Err(403));
        // A page on another site
        assert_eq!(
            check(Method::Post, &[("Host", "localhost:1420"), ("Origin", "https://evil.example"), JSON]),
            Err(403)
        );
        assert_eq!(
            check(Method::Post, &[("Host", "localhost:1420"), ("Origin", "http://localhost:9999"), JSON]),
            Err(403)
        );
        assert_eq!(check(Method::Post, &[("Host", "localhost:1420"), ("Origin", "null"), JSON]), Err(403));
        assert_eq!(
            check(Method::Post, &[("Host", "localhost:1420"), ("Origin", "https://localhost:1420"), JSON]),
            Err(403)
        );
        // What a form or a no-preflight fetch can send
        assert_eq!(
            check(Method::Post, &[("Host", "localhost:1420"), ("Content-Type", "text/plain;charset=UTF-8")]),
            Err(415)
        );
        assert_eq!(check(Method::Post, &[("Host", "localhost:1420")]), Err(415));
        assert_eq!(check(Method::Get, &[("Host", "localhost:1420"), JSON]), Err(405));
    }

    #[test]
    fn port_80_may_be_left_out() {
        assert!(own_address("localhost", 80, ""));
        assert!(own_address("http://127.0.0.1", 80, "http://"));
        assert!(!own_address("localhost", 1420, ""));
    }

    #[test]
    fn chunks_and_head_use_crlf() {
        assert_eq!(chunk(b"{\"type\":\"done\"}\n"), b"10\r\n{\"type\":\"done\"}\n\r\n".to_vec());
        assert!(STREAM_HEAD.ends_with("\r\n\r\n"));
        assert_eq!(STREAM_HEAD.matches('\n').count(), STREAM_HEAD.matches("\r\n").count(), "no bare LF");
        assert!(STREAM_HEAD.contains("Transfer-Encoding: chunked\r\n"));
    }
}
