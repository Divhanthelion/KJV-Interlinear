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
mod openai;
mod sse;
mod think;

use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

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
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub kind: Kind,
    /// "https://api.openai.com/v1", "http://192.168.1.20:8000/v1", …
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
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
    Text { text: String },
    /// Reasoning the model shows while it thinks (summaries on some services)
    Reasoning { text: String },
    Usage { usage: Usage },
    /// The answer is complete. `reason` is normalized: "stop", "length", "refusal", or
    /// the provider's own word for anything else.
    Done { reason: Option<String> },
}

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

/// One client for the whole app: connection reuse, sane timeouts.
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        // Between chunks, not the whole answer: local models can think for minutes
        .read_timeout(Duration::from_secs(300))
        .user_agent(concat!("KJV-Interlinear/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("HTTP client builds")
}

/// Stream a reply, calling `emit` for each piece. Dropping the returned future
/// closes the connection, which stops generation on the server.
pub async fn chat(client: &reqwest::Client, req: &ChatRequest, mut emit: impl FnMut(Event)) -> Result<(), String> {
    if req.model.trim().is_empty() {
        return Err("Choose a model first.".into());
    }
    let (request, mut decoder): (reqwest::RequestBuilder, Box<dyn Decoder>) = match req.endpoint.kind {
        Kind::OpenAi => (openai::request(client, req, true), Box::new(openai::Decoder::default())),
        Kind::Anthropic => (anthropic::request(client, req), Box::new(anthropic::Decoder::default())),
        Kind::Gemini => (gemini::request(client, req), Box::new(gemini::Decoder::default())),
    };
    let mut response = send(request).await?;
    // Some OpenAI-compatible servers reject the usage option; ask again without it
    if req.endpoint.kind == Kind::OpenAi && response.status() == reqwest::StatusCode::BAD_REQUEST {
        let body = response.text().await.unwrap_or_default();
        if !body.contains("stream_options") {
            return Err(http_error(reqwest::StatusCode::BAD_REQUEST, &body));
        }
        response = send(openai::request(client, req, false)).await?;
    }
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(http_error(status, &body));
    }

    let mut parser = SseParser::default();
    let mut stream = response.bytes_stream();
    let mut finished = false;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("The connection dropped: {}", plain_error(&e)))?;
        for data in parser.push(&chunk) {
            finished |= decoder.decode(&data, &mut emit)?;
        }
    }
    for data in parser.finish() {
        finished |= decoder.decode(&data, &mut emit)?;
    }
    decoder.flush(&mut emit);
    if !finished {
        emit(Event::Done { reason: None });
    }
    Ok(())
}

/// Models the service offers, with context sizes where it reports them.
pub async fn models(client: &reqwest::Client, endpoint: &Endpoint) -> Result<Vec<ModelInfo>, String> {
    let request = match endpoint.kind {
        Kind::OpenAi => openai::models_request(client, endpoint),
        Kind::Anthropic => anthropic::models_request(client, endpoint),
        Kind::Gemini => gemini::models_request(client, endpoint),
    };
    let response = send(request.timeout(Duration::from_secs(30))).await?;
    let status = response.status();
    let body = response.text().await.map_err(|e| plain_error(&e))?;
    if !status.is_success() {
        return Err(http_error(status, &body));
    }
    let json: serde_json::Value =
        serde_json::from_str(&body).map_err(|_| "The server's model list isn't JSON. Check the address.".to_string())?;
    let mut list = match endpoint.kind {
        Kind::OpenAi => openai::parse_models(&json),
        Kind::Anthropic => anthropic::parse_models(&json),
        Kind::Gemini => gemini::parse_models(&json),
    };
    list.sort_by_key(|m| m.name.to_lowercase());
    Ok(list)
}

/// Turns provider stream payloads into events. Returns true once the reply is done.
trait Decoder: Send {
    fn decode(&mut self, data: &str, emit: &mut dyn FnMut(Event)) -> Result<bool, String>;
    fn flush(&mut self, _emit: &mut dyn FnMut(Event)) {}
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

/// The innermost cause, which is the useful part ("connection refused").
fn plain_error(e: &(dyn std::error::Error + 'static)) -> String {
    let mut source = e;
    while let Some(next) = source.source() {
        source = next;
    }
    source.to_string()
}

/// A readable message for a failed request, using the service's own message when it
/// sends one (every supported format nests it at `error.message`).
fn http_error(status: reqwest::StatusCode, body: &str) -> String {
    let detail = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|j| {
            j.pointer("/error/message")
                .or_else(|| j.pointer("/message"))
                .or_else(|| j.get("detail"))
                .and_then(|m| m.as_str().map(str::to_string))
        })
        .unwrap_or_else(|| body.chars().take(300).collect::<String>().trim().to_string());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_join_cleanly() {
        assert_eq!(url("http://192.168.1.20:8000/v1/", "/chat/completions"), "http://192.168.1.20:8000/v1/chat/completions");
        assert_eq!(url(" https://api.anthropic.com/v1", "messages"), "https://api.anthropic.com/v1/messages");
    }

    #[test]
    fn errors_use_the_services_message() {
        let body = r#"{"error":{"message":"invalid x-api-key","type":"authentication_error"}}"#;
        assert_eq!(
            http_error(reqwest::StatusCode::UNAUTHORIZED, body),
            "The service rejected the API key: invalid x-api-key"
        );
        assert_eq!(http_error(reqwest::StatusCode::BAD_GATEWAY, ""), "The service had an error (502).");
    }
}
