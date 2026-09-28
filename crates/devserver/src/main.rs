//! Browser preview of the app: serves `ui/` and answers `POST /api/<command>`
//! with the same dispatcher the app uses. Settings are kept in memory.
//!
//! Run from the repository root: `cargo run -p kjv-devserver -- [port]`

use std::path::{Component, Path, PathBuf};

use kjv_core::bundle::DataBundle;
use kjv_core::dispatch::dispatch;
use tiny_http::{Header, Method, Response, Server};

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
    let ui = root.join("ui");
    let port: u16 = std::env::args().nth(1).and_then(|p| p.parse().ok()).unwrap_or(1420);

    eprintln!("Loading data…");
    let data = DataBundle::from_sources(&root).expect("run from the repository root");
    let mut settings = serde_json::Value::Null;

    let server = Server::http(("127.0.0.1", port)).expect("port is free");
    eprintln!("Preview at http://localhost:{}", port);

    for mut request in server.incoming_requests() {
        let url = request.url().to_string();
        let response = if let Some(name) = url.strip_prefix("/api/") {
            let mut body = String::new();
            let _ = request.as_reader().read_to_string(&mut body);
            let args: serde_json::Value = serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
            let result = match name {
                "settings_load" => Ok(settings.clone()),
                "settings_save" => {
                    settings = args;
                    Ok(serde_json::Value::Null)
                }
                _ => dispatch(&data, name, args),
            };
            match result {
                Ok(value) => Response::from_string(value.to_string())
                    .with_header(header("Content-Type", "application/json")),
                Err(message) => Response::from_string(message).with_status_code(400),
            }
        } else if request.method() == &Method::Get {
            match static_path(&ui, &url) {
                Some(path) => match std::fs::read(&path) {
                    Ok(bytes) => Response::from_data(bytes)
                        .with_header(header("Content-Type", content_type(&path)))
                        .with_header(header("Cache-Control", "no-store")),
                    Err(_) => Response::from_string("unreadable").with_status_code(500),
                },
                None => Response::from_string("not found").with_status_code(404),
            }
        } else {
            Response::from_string("method not allowed").with_status_code(405)
        };
        let _ = request.respond(response);
    }
}
