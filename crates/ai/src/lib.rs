//! Streaming chat for the app's Bible-study assistant. Three wire formats cover the
//! services people use: OpenAI-compatible (OpenAI, DeepSeek, OpenRouter, Groq, and
//! local servers such as vLLM, Ollama, LM Studio, llama.cpp), Anthropic, and Gemini.
//!
//! Requests run in Rust, not the web page, so API keys never reach page scripts and
//! local servers on the user's network are reachable from every platform.

mod anthropic;
pub mod assistant;
pub mod conversations;
mod gemini;
pub mod keys;
mod openai;
mod sse;
mod think;

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::StatusCode;
use reqwest::header::{CONTENT_TYPE, HeaderValue, LOCATION};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use sse::SseParser;
pub use think::ThinkSplitter;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// `POST {base}/chat/completions`
    OpenAi,
    /// `POST {base}/messages`
    Anthropic,
    /// `POST {base}/models/{model}:streamGenerateContent`
    Gemini,
}

/// Where to send requests.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub kind: Kind,
    /// "https://api.openai.com/v1", "http://192.168.1.20:8000/v1", …
    pub base_url: String,
    /// Never read from page input: keys come from the keychain (or a key typed into the
    /// provider form, passed separately), not from anything the page puts in a request
    #[serde(skip_deserializing)]
    pub api_key: Option<String>,
}

impl std::fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Endpoint")
            .field("kind", &self.kind)
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| keys::MASK))
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequest {
    pub endpoint: Endpoint,
    pub model: String,
    /// How the assistant should behave
    pub instructions: String,
    /// Scripture attached to the conversation; cached where the provider allows
    #[serde(default)]
    pub context: String,
    pub messages: Vec<Message>,
    /// Longest answer, in tokens
    #[serde(default)]
    pub max_tokens: Option<u32>,
    /// "low" | "medium" | "high": reasoning effort for models that take one
    #[serde(default)]
    pub effort: Option<String>,
    /// Ask for visible reasoning summaries (models whose `ModelInfo` says they can)
    #[serde(default)]
    pub thinking: bool,
    /// Turn a local reasoning model's thinking on or off (`chat_template_kwargs`,
    /// understood by vLLM and llama.cpp); None leaves the server's default
    #[serde(default)]
    pub enable_thinking: Option<bool>,
}

/// Something the model produced, in order.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Event {
    Text {
        text: String,
    },
    /// Reasoning the model shows while it thinks (summaries on some services)
    Reasoning {
        text: String,
    },
    /// Everything sent as `Text` so far was reasoning: move it there. A local model
    /// whose chat template opens the think block in the prompt (DeepSeek-R1, QwQ)
    /// only shows where its reasoning ends, with a `</think>` partway through.
    TextWasReasoning,
    Usage {
        usage: Usage,
    },
    /// The answer is complete. `reason` is normalized: "stop", "length", "refusal",
    /// "cancelled" (Stop was pressed), "incomplete" ([`INCOMPLETE`]: the stream ended
    /// without the service saying the answer was finished), or the provider's own word
    /// for anything else. Sent exactly once, last.
    Done {
        reason: Option<String>,
    },
}

/// `Done` reason for a stream that ended without the service saying the answer was
/// finished (no `[DONE]`, `stop_reason`, or `finishReason`): the connection was
/// probably cut, so the answer may be missing its end.
pub const INCOMPLETE: &str = "incomplete";

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    /// Input tokens read from the provider's prompt cache
    pub cached_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    /// Context window in tokens, when the service says
    pub context_window: Option<u64>,
    /// Longest possible answer, when the service says
    pub max_output: Option<u64>,
    /// Takes `thinking: adaptive` (Anthropic)
    pub adaptive_thinking: bool,
    /// Takes a reasoning effort level (Anthropic)
    pub effort: bool,
}

/// The HTTP client type, so callers needn't depend on reqwest.
pub type Client = reqwest::Client;

/// Most of an error response worth reading
const ERROR_BODY_LIMIT: usize = 64 * 1024;
/// OpenRouter's list of every model is a couple of megabytes
const MODELS_BODY_LIMIT: usize = 16 * 1024 * 1024;

/// One client for the whole app: connection reuse, sane timeouts.
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        // Between chunks, not the whole answer: local models can think for minutes
        .read_timeout(Duration::from_secs(300))
        // A redirect would carry `x-api-key` / `x-goog-api-key` to wherever it points;
        // say where it pointed instead
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("KJV-Interlinear/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("HTTP client builds")
}

/// Stream a reply, calling `emit` for each piece. Dropping the returned future
/// closes the connection, which stops generation on the server. Errors never contain
/// the API key.
pub async fn chat(client: &reqwest::Client, req: &ChatRequest, emit: impl FnMut(Event)) -> Result<(), String> {
    stream_reply(client, req, emit).await.map_err(|e| keys::redact(&e, req.endpoint.api_key.as_deref()))
}

async fn stream_reply(client: &reqwest::Client, req: &ChatRequest, mut emit: impl FnMut(Event)) -> Result<(), String> {
    if req.model.trim().is_empty() {
        return Err("Choose a model first.".into());
    }
    check_endpoint(&req.endpoint)?;
    let (response, mut decoder): (reqwest::Response, Box<dyn Decoder>) = match req.endpoint.kind {
        Kind::OpenAi => (send_openai(client, req).await?, Box::new(openai::Decoder::default())),
        Kind::Anthropic => (send(anthropic::request(client, req)).await?, Box::new(anthropic::Decoder::default())),
        Kind::Gemini => (send(gemini::request(client, req)).await?, Box::new(gemini::Decoder::default())),
    };
    if !response.status().is_success() {
        return Err(failure(response).await);
    }
    if let Some(content_type) = not_event_stream(&response) {
        // An error page, or a server that ignored `stream`: say what it sent rather
        // than finishing with an empty answer
        let detail = detail(&read_text(response, ERROR_BODY_LIMIT).await);
        return Err(if detail.is_empty() {
            format!("The server answered without streaming ({}) and sent nothing.", content_type)
        } else {
            format!("The server answered without streaming ({}): {}", content_type, detail)
        });
    }

    let mut parser = SseParser::default();
    let mut stream = response.bytes_stream();
    let mut finished = false;
    'read: while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("The connection dropped: {}", plain_error(&e)))?;
        for data in parser.push(&chunk)? {
            if decoder.decode(&data, &mut emit)? {
                // Done was sent; anything after it isn't part of the answer
                finished = true;
                break 'read;
            }
        }
    }
    if !finished {
        finished = decoder.flush(&mut emit);
    }
    if !finished {
        emit(Event::Done { reason: Some(INCOMPLETE.into()) });
    }
    Ok(())
}

/// Send an OpenAI-compatible request, rewording it when the server rejects part of
/// it: without the usage option (some servers don't know it), then with the other
/// name for the length limit (`max_tokens` / `max_completion_tokens`), then with no
/// limit. At most four tries.
async fn send_openai(client: &reqwest::Client, req: &ChatRequest) -> Result<reqwest::Response, String> {
    let first = openai::Attempt::first(&req.endpoint);
    let mut attempt = first;
    loop {
        let response = send(openai::request(client, req, attempt)).await?;
        if response.status() != StatusCode::BAD_REQUEST {
            return Ok(response);
        }
        let body = read_text(response, ERROR_BODY_LIMIT).await;
        match attempt.after_rejection(first, &body, req.max_tokens.is_some()) {
            Some(next) => attempt = next,
            None => return Err(http_error(StatusCode::BAD_REQUEST, &body)),
        }
    }
}

/// Models the service offers, with context sizes where it reports them. Errors never
/// contain the API key.
pub async fn models(client: &reqwest::Client, endpoint: &Endpoint) -> Result<Vec<ModelInfo>, String> {
    list_models(client, endpoint).await.map_err(|e| keys::redact(&e, endpoint.api_key.as_deref()))
}

async fn list_models(client: &reqwest::Client, endpoint: &Endpoint) -> Result<Vec<ModelInfo>, String> {
    check_endpoint(endpoint)?;
    let request = match endpoint.kind {
        Kind::OpenAi => openai::models_request(client, endpoint),
        Kind::Anthropic => anthropic::models_request(client, endpoint),
        Kind::Gemini => gemini::models_request(client, endpoint),
    };
    let response = send(request.timeout(Duration::from_secs(30))).await?;
    if !response.status().is_success() {
        return Err(failure(response).await);
    }
    let (body, cut) = read_limited(response, MODELS_BODY_LIMIT).await.map_err(|e| read_error(&e))?;
    if cut {
        return Err("The server's model list is too large to read.".into());
    }
    let json: Value = serde_json::from_slice(&body)
        .map_err(|_| "The server's model list isn't JSON. Check the address.".to_string())?;
    let mut list = match endpoint.kind {
        Kind::OpenAi => openai::parse_models(&json),
        Kind::Anthropic => anthropic::parse_models(&json),
        Kind::Gemini => gemini::parse_models(&json),
    };
    list.sort_by_key(|m| m.name.to_lowercase());
    Ok(list)
}

/// Check a server address before anything is sent to it. Plain `http://` is allowed
/// only for servers on this computer or the local network (loopback, the private
/// ranges 10/8, 172.16/12, 192.168/16, IPv6 unique-local, `localhost`, `*.localhost`,
/// `*.local`), where people run their own models; anywhere else the key and the
/// conversation would cross the internet in the clear.
pub fn check_base_url(base_url: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(base_url.trim()).map_err(|_| {
        "That server address isn't a valid URL. It should look like https://api.example.com/v1.".to_string()
    })?;
    if !matches!(url.scheme(), "https" | "http") {
        return Err("The server address must start with https:// (or http:// for a server on your own network).".into());
    }
    let Some(host) = url.host_str().filter(|h| !h.is_empty()) else {
        return Err("The server address has no host name.".into());
    };
    if !url.username().is_empty() || url.password().is_some() {
        return Err(
            "The server address can't contain a user name or password. Put the API key in the key field.".into()
        );
    }
    if url.query().is_some() {
        return Err("The server address can't contain a query (the part from ? on).".into());
    }
    if url.fragment().is_some() {
        return Err("The server address can't contain a # part.".into());
    }
    if url.scheme() == "http" && !is_local_host(host) {
        return Err("Use https:// for this server. Plain http:// is only for a server on this computer or your \
                    local network (localhost, *.local, 10.x.x.x, 172.16–31.x.x, 192.168.x.x)."
            .into());
    }
    Ok(url)
}

/// This computer or the local network (`host` as `Url::host_str` gives it).
fn is_local_host(host: &str) -> bool {
    let local_v4 = |ip: Ipv4Addr| ip.is_loopback() || ip.is_private();
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = bare.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(v4) => local_v4(v4),
            // Loopback, unique-local fc00::/7, or an IPv4 address written as IPv6
            IpAddr::V6(v6) => {
                v6.is_loopback() || (v6.segments()[0] & 0xfe00) == 0xfc00 || v6.to_ipv4_mapped().is_some_and(local_v4)
            }
        };
    }
    let name = bare.trim_end_matches('.').to_ascii_lowercase();
    name == "localhost" || name.ends_with(".localhost") || name.ends_with(".local")
}

/// Whether `base_url` points at exactly `host`.
fn host_is(base_url: &str, host: &str) -> bool {
    reqwest::Url::parse(base_url.trim())
        .ok()
        .and_then(|u| u.host_str().map(|h| h.eq_ignore_ascii_case(host)))
        .unwrap_or(false)
}

/// A request can go out: the address is acceptable and the key can be sent as a header.
fn check_endpoint(endpoint: &Endpoint) -> Result<(), String> {
    check_base_url(&endpoint.base_url)?;
    if let Some(key) = endpoint.api_key.as_deref()
        && HeaderValue::from_str(key.trim()).is_err()
    {
        return Err("The API key has characters an API key doesn't contain (a line break, or a letter outside \
                    plain English?). Paste it again."
            .into());
    }
    Ok(())
}

/// A header value holding a secret: never shown in debug output or logs.
fn secret_header(value: &str) -> HeaderValue {
    // check_endpoint has already refused keys that can't be header text
    let mut value = HeaderValue::from_str(value).unwrap_or_else(|_| HeaderValue::from_static(""));
    value.set_sensitive(true);
    value
}

/// Turns provider stream payloads into events.
trait Decoder: Send {
    /// Returns true once the reply is done (`Done` was emitted); nothing more is read.
    fn decode(&mut self, data: &str, emit: &mut dyn FnMut(Event)) -> Result<bool, String>;
    /// The stream ended before `decode` returned true: emit anything held back, and
    /// `Done` if the service said why it stopped. Returns true if it emitted `Done`.
    fn flush(&mut self, _emit: &mut dyn FnMut(Event)) -> bool {
        false
    }
}

fn url(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim().trim_end_matches('/'), path.trim_start_matches('/'))
}

async fn send(request: reqwest::RequestBuilder) -> Result<reqwest::Response, String> {
    request.send().await.map_err(|e| {
        if e.is_connect() {
            format!("Couldn't reach the server: {}", plain_error(&e))
        } else if e.is_timeout() {
            "The server took too long to answer.".to_string()
        } else if e.is_builder() {
            "That server address isn't a valid URL.".to_string()
        } else {
            plain_error(&e)
        }
    })
}

fn read_error(e: &reqwest::Error) -> String {
    if e.is_timeout() { "The server took too long to answer.".to_string() } else { plain_error(e) }
}

/// Up to `limit` bytes of the body, and whether there was more.
async fn read_limited(response: reqwest::Response, limit: usize) -> Result<(Vec<u8>, bool), reqwest::Error> {
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let room = limit - body.len();
        if chunk.len() > room {
            body.extend_from_slice(&chunk[..room]);
            return Ok((body, true));
        }
        body.extend_from_slice(&chunk);
    }
    Ok((body, false))
}

/// The start of a body as text, for error messages ("" if it can't be read).
async fn read_text(response: reqwest::Response, limit: usize) -> String {
    match read_limited(response, limit).await {
        Ok((bytes, _)) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(_) => String::new(),
    }
}

/// The Content-Type of a successful reply that isn't an event stream. A reply with
/// no Content-Type at all is read as a stream (it can't be told apart).
fn not_event_stream(response: &reqwest::Response) -> Option<String> {
    let value = response.headers().get(CONTENT_TYPE)?;
    let text = String::from_utf8_lossy(value.as_bytes()).trim().to_string();
    let media = text.split(';').next().unwrap_or("").trim();
    (!media.eq_ignore_ascii_case("text/event-stream")).then_some(text)
}

/// The message for a failed request.
async fn failure(response: reqwest::Response) -> String {
    let status = response.status();
    if status.is_redirection() {
        let to = response.headers().get(LOCATION).map(|v| String::from_utf8_lossy(v.as_bytes()).trim().to_string());
        return match to.filter(|t| !t.is_empty()) {
            Some(to) => format!(
                "The server redirected the request to {}. Check the server address (https:// or http://, and the \
                 path such as /v1).",
                to.chars().take(200).collect::<String>()
            ),
            None => format!("The server redirected the request ({}). Check the server address.", status.as_u16()),
        };
    }
    http_error(status, &read_text(response, ERROR_BODY_LIMIT).await)
}

/// The innermost cause, which is the useful part ("connection refused").
fn plain_error(e: &(dyn std::error::Error + 'static)) -> String {
    let mut source = e;
    while let Some(next) = source.source() {
        source = next;
    }
    source.to_string()
}

/// A readable message for a failed request, using the service's own message when it
/// sends one.
fn http_error(status: StatusCode, body: &str) -> String {
    let detail = detail(body);
    let lead = match status.as_u16() {
        401 | 403 => "The service rejected the API key".to_string(),
        404 => "Not found. Check the server address and model name".to_string(),
        429 => "Rate limited or out of credit".to_string(),
        413 => "The request is too large for this model".to_string(),
        500..=599 => format!("The service had an error ({})", status.as_u16()),
        _ => format!("Request failed ({})", status.as_u16()),
    };
    if detail.is_empty() { format!("{}.", lead) } else { format!("{}: {}", lead, detail) }
}

/// The service's own message in an error body: `error.message` (every supported
/// service), `error` or `message` as a string, or `detail` (FastAPI servers, as a
/// string or a list of `{loc, msg}`). Otherwise the start of the body.
fn detail(body: &str) -> String {
    let found = serde_json::from_str::<Value>(body).ok().and_then(|j| {
        let text = |v: Option<&Value>| v.and_then(Value::as_str).map(str::to_string);
        text(j.pointer("/error/message")).or_else(|| text(j.get("error"))).or_else(|| text(j.get("message"))).or_else(
            || match j.get("detail") {
                Some(Value::String(s)) => Some(s.clone()),
                Some(Value::Array(items)) => fastapi_detail(items),
                _ => None,
            },
        )
    });
    match found.map(|d| d.trim().to_string()).filter(|d| !d.is_empty()) {
        Some(d) => d.chars().take(1000).collect(),
        None => body.chars().take(300).collect::<String>().trim().to_string(),
    }
}

/// `[{"loc": ["body", "messages", 0], "msg": "field required"}]` → "messages.0: field required"
fn fastapi_detail(items: &[Value]) -> Option<String> {
    let messages: Vec<String> = items
        .iter()
        .filter_map(|item| {
            let msg = item.get("msg").and_then(Value::as_str)?;
            let place: Vec<String> = item
                .get("loc")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|p| p.as_str() != Some("body"))
                .map(|p| p.as_str().map(str::to_string).unwrap_or_else(|| p.to_string()))
                .collect();
            Some(if place.is_empty() { msg.to_string() } else { format!("{}: {}", place.join("."), msg) })
        })
        .collect();
    (!messages.is_empty()).then(|| messages.join("; "))
}

#[cfg(test)]
pub(crate) mod mock {
    //! A stand-in server: answers each connection with the next canned response.

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    /// Its base URL, and a task that ends with the requests it got (head and body)
    /// once every response is used.
    pub async fn serve(responses: Vec<String>) -> (String, tokio::task::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let mut requests = Vec::new();
            for response in responses {
                let (mut socket, _) = listener.accept().await.unwrap();
                requests.push(read_request(&mut socket).await);
                socket.write_all(response.as_bytes()).await.unwrap();
                let _ = socket.shutdown().await;
            }
            requests
        });
        (base, task)
    }

    async fn read_request(socket: &mut TcpStream) -> String {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&buf[..end]).to_string();
                let length = head
                    .lines()
                    .find_map(|l| {
                        let (name, value) = l.split_once(':')?;
                        name.trim()
                            .eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())?
                    })
                    .unwrap_or(0);
                while buf.len() < end + 4 + length {
                    let n = socket.read(&mut chunk).await.unwrap();
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                }
                return String::from_utf8_lossy(&buf).to_string();
            }
            let n = socket.read(&mut chunk).await.unwrap();
            if n == 0 {
                return String::from_utf8_lossy(&buf).to_string();
            }
            buf.extend_from_slice(&chunk[..n]);
        }
    }

    pub fn response(status: &str, content_type: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            status,
            content_type,
            body.len(),
            body
        )
    }

    pub fn sse(body: &str) -> String {
        response("200 OK", "text/event-stream", body)
    }

    pub fn json(status: &str, body: &str) -> String {
        response(status, "application/json", body)
    }

    /// The body of a request `serve` collected.
    pub fn body(request: &str) -> &str {
        request.split_once("\r\n\r\n").map_or("", |(_, b)| b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const FAKE_KEY: &str = "sk-test-0000-not-a-real-key";

    fn endpoint(kind: Kind, base_url: &str) -> Endpoint {
        Endpoint { kind, base_url: base_url.into(), api_key: Some(FAKE_KEY.into()) }
    }

    fn request(endpoint: Endpoint, max_tokens: Option<u32>) -> ChatRequest {
        ChatRequest {
            endpoint,
            model: "m".into(),
            instructions: "Be brief.".into(),
            context: String::new(),
            messages: vec![Message { role: Role::User, content: "Why did Jesus weep?".into() }],
            max_tokens,
            effort: None,
            thinking: false,
            enable_thinking: None,
        }
    }

    async fn run(req: &ChatRequest) -> (Result<(), String>, Vec<Event>) {
        let mut events = Vec::new();
        let result = chat(&client(), req, |e| events.push(e)).await;
        (result, events)
    }

    fn dones(events: &[Event]) -> Vec<Option<String>> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::Done { reason } => Some(reason.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn urls_join_cleanly() {
        assert_eq!(
            url("http://192.168.1.20:8000/v1/", "/chat/completions"),
            "http://192.168.1.20:8000/v1/chat/completions"
        );
        assert_eq!(url(" https://api.anthropic.com/v1", "messages"), "https://api.anthropic.com/v1/messages");
    }

    #[test]
    fn errors_use_the_services_message() {
        let body = r#"{"error":{"message":"invalid x-api-key","type":"authentication_error"}}"#;
        assert_eq!(http_error(StatusCode::UNAUTHORIZED, body), "The service rejected the API key: invalid x-api-key");
        assert_eq!(http_error(StatusCode::BAD_GATEWAY, ""), "The service had an error (502).");
    }

    #[test]
    fn errors_read_string_and_fastapi_bodies() {
        assert_eq!(
            http_error(StatusCode::NOT_FOUND, r#"{"error":"model 'x' not found"}"#),
            "Not found. Check the server address and model name: model 'x' not found"
        );
        let fastapi = r#"{"detail":[{"loc":["body","messages",0,"content"],"msg":"Field required","type":"missing"},
                                     {"loc":["body","model"],"msg":"Input should be a valid string"}]}"#;
        assert_eq!(
            http_error(StatusCode::UNPROCESSABLE_ENTITY, fastapi),
            "Request failed (422): messages.0.content: Field required; model: Input should be a valid string"
        );
        assert_eq!(
            http_error(StatusCode::BAD_REQUEST, r#"{"detail":"Not allowed"}"#),
            "Request failed (400): Not allowed"
        );
        assert_eq!(http_error(StatusCode::BAD_REQUEST, "plain words"), "Request failed (400): plain words");
    }

    #[test]
    fn server_addresses_are_checked() {
        for ok in [
            "https://api.openai.com/v1",
            " https://generativelanguage.googleapis.com/v1beta/ ",
            "http://localhost:11434/v1",
            "http://LOCALHOST:8000/v1",
            "http://127.0.0.1:8000/v1",
            "http://127.1.2.3/v1",
            "http://10.0.0.5/v1",
            "http://172.16.0.1:8000/v1",
            "http://172.31.255.1/v1",
            "http://192.168.1.20:8000/v1",
            "http://gpu-box.local:8000/v1",
            "http://vllm.localhost/v1",
            "http://[::1]:8000/v1",
            "http://[fd12:3456::1]:8000/v1",
        ] {
            assert!(check_base_url(ok).is_ok(), "{} should be allowed", ok);
        }
        for (bad, says) in [
            ("http://api.example.com/v1", "https://"),
            ("http://172.32.0.1/v1", "https://"),
            ("http://8.8.8.8/v1", "https://"),
            ("http://local.example.com/v1", "https://"),
            ("http://[2001:db8::1]/v1", "https://"),
            ("https://user:pass@api.example.com/v1", "user name or password"),
            ("https://sk-abc@api.example.com/v1", "user name or password"),
            ("https://api.example.com/v1?key=abc", "query"),
            ("https://api.example.com/v1#x", "#"),
            ("ftp://api.example.com/v1", "must start with"),
            ("api.example.com/v1", "isn't a valid URL"),
            ("", "isn't a valid URL"),
        ] {
            let err = check_base_url(bad).unwrap_err();
            assert!(err.contains(says), "{}: {}", bad, err);
        }
    }

    #[test]
    fn keys_are_not_read_from_input_or_printed() {
        let e: Endpoint =
            serde_json::from_value(json!({"kind": "openai", "baseUrl": "https://x.example/v1", "apiKey": FAKE_KEY}))
                .unwrap();
        assert_eq!(e.api_key, None);
        let printed = format!("{:?}", request(endpoint(Kind::OpenAi, "https://x.example/v1"), None));
        assert!(!printed.contains(FAKE_KEY), "{}", printed);
        assert!(printed.contains(keys::MASK));
    }

    #[test]
    fn key_headers_are_marked_sensitive() {
        let client = client();
        let built = anthropic::models_request(&client, &endpoint(Kind::Anthropic, "https://api.anthropic.com/v1"))
            .build()
            .unwrap();
        assert!(built.headers()["x-api-key"].is_sensitive());
        let built = gemini::models_request(
            &client,
            &endpoint(Kind::Gemini, "https://generativelanguage.googleapis.com/v1beta"),
        )
        .build()
        .unwrap();
        assert!(built.headers()["x-goog-api-key"].is_sensitive());
        let built =
            openai::models_request(&client, &endpoint(Kind::OpenAi, "https://api.openai.com/v1")).build().unwrap();
        assert!(built.headers()["authorization"].is_sensitive());
    }

    #[tokio::test]
    async fn keys_that_cant_be_headers_are_refused_before_sending() {
        let mut e = endpoint(Kind::Anthropic, "https://api.anthropic.com/v1");
        e.api_key = Some("sk-ant\nX-Injected: 1".into());
        let err = models(&client(), &e).await.unwrap_err();
        assert!(err.contains("Paste it again"), "{}", err);
    }

    #[tokio::test]
    async fn length_limit_is_reworded_then_dropped_when_rejected() {
        let (base, server) = mock::serve(vec![
            mock::json("400 Bad Request", r#"{"error":{"message":"Unsupported parameter: 'max_tokens' is not supported with this model. Use 'max_completion_tokens' instead."}}"#),
            mock::json("400 Bad Request", r#"{"error":{"message":"Unrecognized request argument supplied: max_completion_tokens"}}"#),
            mock::sse("data: {\"choices\":[{\"delta\":{\"content\":\"Amen.\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n"),
        ])
        .await;
        let req = request(endpoint(Kind::OpenAi, &format!("{}/v1", base)), Some(500));
        let (result, events) = run(&req).await;
        result.unwrap();
        assert_eq!(events, vec![Event::Text { text: "Amen.".into() }, Event::Done { reason: Some("stop".into()) }]);
        let bodies: Vec<Value> =
            server.await.unwrap().iter().map(|r| serde_json::from_str(mock::body(r)).unwrap()).collect();
        assert_eq!(bodies[0]["max_tokens"], 500);
        assert!(bodies[0].get("max_completion_tokens").is_none());
        assert_eq!(bodies[1]["max_completion_tokens"], 500);
        assert!(bodies[1].get("max_tokens").is_none());
        assert!(bodies[2].get("max_tokens").is_none() && bodies[2].get("max_completion_tokens").is_none());
        assert!(bodies.iter().all(|b| b["stream_options"]["include_usage"] == true));
    }

    #[tokio::test]
    async fn usage_option_is_dropped_when_rejected_and_other_400s_are_errors() {
        let (base, server) = mock::serve(vec![
            mock::json("400 Bad Request", r#"{"error":{"message":"extra field: stream_options"}}"#),
            mock::json("400 Bad Request", r#"{"error":{"message":"messages: too many"}}"#),
        ])
        .await;
        let req = request(endpoint(Kind::OpenAi, &format!("{}/v1", base)), Some(500));
        let (result, events) = run(&req).await;
        assert_eq!(result.unwrap_err(), "Request failed (400): messages: too many");
        assert!(events.is_empty());
        let requests = server.await.unwrap();
        assert!(mock::body(&requests[0]).contains("stream_options"));
        assert!(!mock::body(&requests[1]).contains("stream_options"));
    }

    #[tokio::test]
    async fn gemini_max_tokens_finish_is_reported_once() {
        let (base, _server) = mock::serve(vec![mock::sse(concat!(
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"In the beginning\"}],\"role\":\"model\"}}]}\r\n\r\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\" was the Word\"}],\"role\":\"model\"},",
            "\"finishReason\":\"MAX_TOKENS\"}],\"usageMetadata\":{\"promptTokenCount\":9,\"candidatesTokenCount\":4}}\r\n\r\n",
        ))])
        .await;
        let req = request(endpoint(Kind::Gemini, &format!("{}/v1beta", base)), Some(4));
        let (result, events) = run(&req).await;
        result.unwrap();
        assert_eq!(dones(&events), vec![Some("length".to_string())]);
        assert_eq!(events.last(), Some(&Event::Done { reason: Some("length".into()) }));
    }

    #[tokio::test]
    async fn a_stream_that_just_stops_is_incomplete() {
        let (base, _server) =
            mock::serve(vec![mock::sse("data: {\"choices\":[{\"delta\":{\"content\":\"In the begin\"}}]}\n\n")]).await;
        let (result, events) = run(&request(endpoint(Kind::OpenAi, &format!("{}/v1", base)), None)).await;
        result.unwrap();
        assert_eq!(
            events,
            vec![Event::Text { text: "In the begin".into() }, Event::Done { reason: Some(INCOMPLETE.into()) }]
        );
    }

    #[tokio::test]
    async fn a_finish_reason_without_done_marker_still_counts() {
        let (base, _server) = mock::serve(vec![mock::sse(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Selah\"},\"finish_reason\":\"length\"}]}\n\n",
        )])
        .await;
        let (result, events) = run(&request(endpoint(Kind::OpenAi, &format!("{}/v1", base)), None)).await;
        result.unwrap();
        assert_eq!(dones(&events), vec![Some("length".to_string())]);
    }

    #[tokio::test]
    async fn nothing_is_read_after_the_answer_ends() {
        let (base, _server) = mock::serve(vec![mock::sse(concat!(
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Amen.\"}}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":2}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Stray.\"}}\n\n",
            "event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"api_error\",\"message\":\"late\"}}\n\n",
        ))])
        .await;
        let (result, events) = run(&request(endpoint(Kind::Anthropic, &format!("{}/v1", base)), None)).await;
        result.unwrap();
        assert_eq!(events.first(), Some(&Event::Text { text: "Amen.".into() }));
        assert_eq!(events.last(), Some(&Event::Done { reason: Some("stop".into()) }));
        assert!(!events.contains(&Event::Text { text: "Stray.".into() }));
    }

    #[tokio::test]
    async fn a_reply_that_isnt_a_stream_is_an_error() {
        let (base, _server) = mock::serve(vec![mock::json("200 OK", r#"{"error":"model is still loading"}"#)]).await;
        let (result, events) = run(&request(endpoint(Kind::OpenAi, &format!("{}/v1", base)), None)).await;
        let err = result.unwrap_err();
        assert!(
            err.contains("without streaming (application/json)") && err.contains("model is still loading"),
            "{}",
            err
        );
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn redirects_are_not_followed() {
        let redirect = "HTTP/1.1 301 Moved Permanently\r\nLocation: https://elsewhere.example/v1/models\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let (base, server) = mock::serve(vec![redirect.to_string()]).await;
        let err = models(&client(), &endpoint(Kind::Anthropic, &format!("{}/v1", base))).await.unwrap_err();
        assert!(err.contains("redirected the request to https://elsewhere.example/v1/models"), "{}", err);
        assert_eq!(server.await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn errors_never_show_the_key() {
        let echo = format!(r#"{{"error":{{"message":"Incorrect API key provided: {}"}}}}"#, FAKE_KEY);
        let (base, server) =
            mock::serve(vec![mock::json("401 Unauthorized", &echo), mock::json("401 Unauthorized", &echo)]).await;
        let e = endpoint(Kind::OpenAi, &format!("{}/v1", base));
        let err = models(&client(), &e).await.unwrap_err();
        assert_eq!(err, "The service rejected the API key: Incorrect API key provided: •••");
        let (result, _) = run(&request(e, None)).await;
        let err = result.unwrap_err();
        assert!(!err.contains(FAKE_KEY) && err.contains("•••"), "{}", err);
        // The key did go to the server, as a bearer token
        assert!(server.await.unwrap()[0].contains(&format!("Bearer {}", FAKE_KEY)));
    }

    #[tokio::test]
    async fn model_lists_are_read() {
        let (base, _server) = mock::serve(vec![mock::json(
            "200 OK",
            r#"{"data":[{"id":"b-model","max_model_len":8192},{"id":"a-model"}]}"#,
        )])
        .await;
        let list = models(&client(), &endpoint(Kind::OpenAi, &format!("{}/v1", base))).await.unwrap();
        assert_eq!(list.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["a-model", "b-model"]);
    }
}
