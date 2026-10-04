//! OpenAI-compatible chat completions: OpenAI, DeepSeek, OpenRouter, Groq, and local
//! servers (vLLM, Ollama, LM Studio, llama.cpp).

use serde_json::{Value, json};

use crate::think::{Piece, ThinkSplitter};
use crate::{ChatRequest, Endpoint, Event, ModelInfo, Role, Usage, url};

/// Which name the length limit goes by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    /// What most OpenAI-compatible servers know
    MaxTokens,
    /// What OpenAI wants (its reasoning models refuse `max_tokens`)
    MaxCompletionTokens,
    /// Neither: the server refused both
    Omit,
}

/// How one try at a request is worded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attempt {
    /// Ask for token usage at the end (`stream_options`)
    pub usage: bool,
    pub limit: Limit,
}

impl Attempt {
    pub fn first(endpoint: &Endpoint) -> Self {
        let limit = if is_openai(&endpoint.base_url) { Limit::MaxCompletionTokens } else { Limit::MaxTokens };
        Self { usage: true, limit }
    }

    /// What to try after the server answered this attempt with 400 and `body`, if the
    /// body names something that can be left out or renamed. The length limit goes
    /// from `first`'s name to the other one, then is dropped.
    pub fn after_rejection(self, first: Attempt, body: &str, has_limit: bool) -> Option<Attempt> {
        if self.usage && body.contains("stream_options") {
            return Some(Attempt { usage: false, ..self });
        }
        if has_limit && (body.contains("max_tokens") || body.contains("max_completion_tokens")) {
            let other = match first.limit {
                Limit::MaxTokens => Limit::MaxCompletionTokens,
                Limit::MaxCompletionTokens => Limit::MaxTokens,
                Limit::Omit => return None,
            };
            let limit = if self.limit == first.limit {
                other
            } else if self.limit == other {
                Limit::Omit
            } else {
                return None;
            };
            return Some(Attempt { limit, ..self });
        }
        None
    }
}

fn is_openai(base_url: &str) -> bool {
    crate::host_is(base_url, "api.openai.com")
}

pub fn body(req: &ChatRequest, attempt: Attempt) -> Value {
    let mut system = req.instructions.clone();
    if !req.context.is_empty() {
        // Stable text first so servers with prefix caching reuse it across turns
        system.push_str("\n\n");
        system.push_str(&req.context);
    }
    let mut messages = vec![json!({"role": "system", "content": system})];
    messages.extend(req.messages.iter().map(|m| {
        json!({
            "role": match m.role { Role::User => "user", Role::Assistant => "assistant" },
            "content": m.content,
        })
    }));
    let mut body = json!({"model": req.model, "messages": messages, "stream": true});
    if attempt.usage {
        body["stream_options"] = json!({"include_usage": true});
    }
    if let Some(max) = req.max_tokens {
        match attempt.limit {
            Limit::MaxTokens => body["max_tokens"] = json!(max),
            Limit::MaxCompletionTokens => body["max_completion_tokens"] = json!(max),
            Limit::Omit => {}
        }
    }
    if let Some(on) = req.enable_thinking {
        body["chat_template_kwargs"] = json!({"enable_thinking": on});
    }
    if let Some(effort) = &req.effort {
        // OpenAI reasoning models; other servers ignore unknown fields or say so
        if is_openai(&req.endpoint.base_url) {
            body["reasoning_effort"] = json!(effort);
        }
    }
    body
}

pub fn request(client: &reqwest::Client, req: &ChatRequest, attempt: Attempt) -> reqwest::RequestBuilder {
    auth(client.post(url(&req.endpoint.base_url, "chat/completions")), &req.endpoint).json(&body(req, attempt))
}

pub fn models_request(client: &reqwest::Client, endpoint: &Endpoint) -> reqwest::RequestBuilder {
    auth(client.get(url(&endpoint.base_url, "models")), endpoint)
}

/// `bearer_auth` marks the header sensitive.
fn auth(request: reqwest::RequestBuilder, endpoint: &Endpoint) -> reqwest::RequestBuilder {
    match endpoint.api_key.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
        Some(key) => request.bearer_auth(key),
        None => request,
    }
}

/// Context sizes go by different names: `max_model_len` (vLLM), `context_length`
/// (OpenRouter, LM Studio), `context_window` (Groq).
pub fn parse_models(json: &Value) -> Vec<ModelInfo> {
    let list = json.get("data").or_else(|| json.get("models")).and_then(Value::as_array);
    list.into_iter()
        .flatten()
        .filter_map(|m| {
            let id = m.get("id").and_then(Value::as_str)?.to_string();
            let context_window = ["max_model_len", "context_length", "context_window"]
                .iter()
                .find_map(|k| m.get(*k).and_then(Value::as_u64));
            let max_output = m
                .pointer("/top_provider/max_completion_tokens")
                .or_else(|| m.get("max_completion_tokens"))
                .and_then(Value::as_u64);
            let name = m.get("name").and_then(Value::as_str).unwrap_or(&id).to_string();
            Some(ModelInfo { id, name, context_window, max_output, adaptive_thinking: false, effort: false })
        })
        .collect()
}

#[derive(Default)]
pub struct Decoder {
    think: ThinkSplitter,
    /// The finish reason, once a chunk gives one (`[DONE]` follows it, usually)
    reason: Option<String>,
}

fn send(emit: &mut dyn FnMut(Event), pieces: Vec<Piece>) {
    for piece in pieces {
        emit(match piece {
            Piece::Text(text) => Event::Text { text },
            Piece::Reasoning(text) => Event::Reasoning { text },
            Piece::TextWasReasoning => Event::TextWasReasoning,
        });
    }
}

impl crate::Decoder for Decoder {
    fn decode(&mut self, data: &str, emit: &mut dyn FnMut(Event)) -> Result<bool, String> {
        if data.trim() == "[DONE]" {
            send(emit, self.think.finish());
            emit(Event::Done { reason: self.reason.take().or(Some("stop".into())) });
            return Ok(true);
        }
        let Ok(chunk) = serde_json::from_str::<Value>(data) else {
            return Ok(false);
        };
        if let Some(message) = chunk
            .pointer("/error/message")
            .or_else(|| chunk.get("error").filter(|e| e.is_string()))
            .and_then(Value::as_str)
        {
            return Err(format!("The model stopped with an error: {}", message));
        }
        if let Some(choice) = chunk.pointer("/choices/0") {
            let delta = &choice["delta"];
            // `reasoning_content` (DeepSeek, vLLM) or `reasoning` (OpenRouter, newer vLLM)
            for key in ["reasoning_content", "reasoning"] {
                if let Some(text) = delta.get(key).and_then(Value::as_str).filter(|t| !t.is_empty()) {
                    emit(Event::Reasoning { text: text.to_string() });
                    break;
                }
            }
            if let Some(text) = delta.get("content").and_then(Value::as_str) {
                send(emit, self.think.push(text));
            }
            if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
                self.reason = Some(match reason {
                    "length" => "length".into(),
                    "content_filter" => "refusal".into(),
                    "stop" | "tool_calls" | "eos" => "stop".into(),
                    other => other.into(),
                });
            }
        }
        if let Some(u) = chunk.get("usage").filter(|u| u.is_object()) {
            emit(Event::Usage {
                usage: Usage {
                    input_tokens: u.get("prompt_tokens").and_then(Value::as_u64),
                    output_tokens: u.get("completion_tokens").and_then(Value::as_u64),
                    cached_tokens: u
                        .pointer("/prompt_tokens_details/cached_tokens")
                        .or_else(|| u.get("prompt_cache_hit_tokens"))
                        .and_then(Value::as_u64),
                },
            });
        }
        Ok(false)
    }

    /// No `[DONE]`: a finish reason still says the answer ended (some servers stop there).
    fn flush(&mut self, emit: &mut dyn FnMut(Event)) -> bool {
        send(emit, self.think.finish());
        match self.reason.take() {
            Some(reason) => {
                emit(Event::Done { reason: Some(reason) });
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Decoder as _;
    use crate::{Kind, Message};

    fn decode(lines: &[&str]) -> Vec<Event> {
        let mut d = Decoder::default();
        let mut events = Vec::new();
        for l in lines {
            d.decode(l, &mut |e| events.push(e)).unwrap();
        }
        events
    }

    fn chat(base_url: &str) -> ChatRequest {
        ChatRequest {
            endpoint: Endpoint { kind: Kind::OpenAi, base_url: base_url.into(), api_key: None },
            model: "m".into(),
            instructions: "Be brief.".into(),
            context: String::new(),
            messages: vec![Message { role: Role::User, content: "Hi".into() }],
            max_tokens: Some(1000),
            effort: Some("low".into()),
            thinking: false,
            enable_thinking: None,
        }
    }

    #[test]
    fn streams_reasoning_text_usage_and_finish() {
        let events = decode(&[
            r#"{"choices":[{"delta":{"role":"assistant","reasoning_content":"Hmm."}}]}"#,
            r#"{"choices":[{"delta":{"content":"Jesus "}}]}"#,
            r#"{"choices":[{"delta":{"content":"wept."},"finish_reason":"stop"}]}"#,
            r#"{"choices":[],"usage":{"prompt_tokens":120,"completion_tokens":9,"prompt_tokens_details":{"cached_tokens":100}}}"#,
            "[DONE]",
        ]);
        assert_eq!(
            events,
            vec![
                Event::Reasoning { text: "Hmm.".into() },
                Event::Text { text: "Jesus ".into() },
                Event::Text { text: "wept.".into() },
                Event::Usage {
                    usage: Usage { input_tokens: Some(120), output_tokens: Some(9), cached_tokens: Some(100) }
                },
                Event::Done { reason: Some("stop".into()) },
            ]
        );
    }

    #[test]
    fn flush_reports_a_finish_reason_given_without_done() {
        let mut d = Decoder::default();
        let mut events = Vec::new();
        d.decode(r#"{"choices":[{"delta":{"content":"Amen"},"finish_reason":"length"}]}"#, &mut |e| events.push(e))
            .unwrap();
        assert!(d.flush(&mut |e| events.push(e)));
        assert_eq!(events.last(), Some(&Event::Done { reason: Some("length".into()) }));
        // With no finish reason, flush leaves the ending to the caller
        assert!(!Decoder::default().flush(&mut |_| panic!("nothing to emit")));
    }

    #[test]
    fn inline_reasoning_closed_without_opening_moves_to_reasoning() {
        let events = decode(&[
            r#"{"choices":[{"delta":{"content":"The user asks about John 11."}}]}"#,
            r#"{"choices":[{"delta":{"content":"</think>\n\nJesus wept."}}]}"#,
            "[DONE]",
        ]);
        assert_eq!(
            events,
            vec![
                Event::Text { text: "The user asks about John 11.".into() },
                Event::TextWasReasoning,
                Event::Text { text: "Jesus wept.".into() },
                Event::Done { reason: Some("stop".into()) },
            ]
        );
    }

    #[test]
    fn stream_errors_as_plain_strings_are_reported() {
        let mut d = Decoder::default();
        let err = d.decode(r#"{"error":"out of memory"}"#, &mut |_| {}).unwrap_err();
        assert_eq!(err, "The model stopped with an error: out of memory");
    }

    #[test]
    fn openai_itself_gets_max_completion_tokens() {
        let first = Attempt::first(&chat("https://api.openai.com/v1").endpoint);
        assert_eq!(first.limit, Limit::MaxCompletionTokens);
        let b = body(&chat("https://api.openai.com/v1"), first);
        assert_eq!((b["max_completion_tokens"].as_u64(), b.get("max_tokens")), (Some(1000), None));
        assert_eq!(b["reasoning_effort"], "low");
        for other in
            ["https://api.deepseek.com/v1", "http://192.168.1.20:8000/v1", "https://api.openai.com.evil.example/v1"]
        {
            let first = Attempt::first(&chat(other).endpoint);
            assert_eq!(first.limit, Limit::MaxTokens, "{}", other);
            let b = body(&chat(other), first);
            assert_eq!((b["max_tokens"].as_u64(), b.get("max_completion_tokens")), (Some(1000), None));
            assert!(b.get("reasoning_effort").is_none(), "{}", other);
        }
        let none = body(&chat("https://api.openai.com/v1"), Attempt { usage: false, limit: Limit::Omit });
        assert!(none.get("max_tokens").is_none() && none.get("max_completion_tokens").is_none());
        assert!(none.get("stream_options").is_none());
    }

    #[test]
    fn rejections_change_one_thing_at_a_time() {
        let openai = Attempt { usage: true, limit: Limit::MaxCompletionTokens };
        let local = Attempt { usage: true, limit: Limit::MaxTokens };
        let unsupported = r#"{"error":{"message":"Unsupported parameter: 'max_tokens' is not supported with this model. Use 'max_completion_tokens' instead.","param":"max_tokens"}}"#;
        let unknown = r#"{"object":"error","message":"[{'type': 'extra_forbidden', 'loc': ('body', 'max_completion_tokens'), 'msg': 'Extra inputs are not permitted'}]"}"#;
        let usage = r#"{"error":{"message":"Unrecognized request argument supplied: stream_options"}}"#;
        let other = r#"{"error":{"message":"model not found"}}"#;

        // OpenAI: other name, then none
        let second = openai.after_rejection(openai, unknown, true).unwrap();
        assert_eq!(second, Attempt { usage: true, limit: Limit::MaxTokens });
        let third = second.after_rejection(openai, unsupported, true).unwrap();
        assert_eq!(third, Attempt { usage: true, limit: Limit::Omit });
        assert_eq!(third.after_rejection(openai, unsupported, true), None);

        // A local server: max_tokens first
        let second = local.after_rejection(local, unsupported, true).unwrap();
        assert_eq!(second.limit, Limit::MaxCompletionTokens);
        assert_eq!(second.after_rejection(local, unknown, true).unwrap().limit, Limit::Omit);

        // The usage option goes first, once
        let no_usage = local.after_rejection(local, usage, true).unwrap();
        assert_eq!(no_usage, Attempt { usage: false, limit: Limit::MaxTokens });
        assert_eq!(no_usage.after_rejection(local, usage, true), None);

        // No limit was sent, or the complaint is about something else: no retry
        assert_eq!(local.after_rejection(local, unsupported, false), None);
        assert_eq!(local.after_rejection(local, other, true), None);
    }

    #[test]
    fn reads_context_sizes_from_vllm_and_openrouter() {
        let vllm = json!({"data": [{"id": "qwen3.8-flash-next", "max_model_len": 262144}]});
        let m = &parse_models(&vllm)[0];
        assert_eq!((m.id.as_str(), m.context_window), ("qwen3.8-flash-next", Some(262144)));
        let router = json!({"data": [{"id": "x/y", "name": "Y", "context_length": 128000,
            "top_provider": {"max_completion_tokens": 8192}}]});
        let m = &parse_models(&router)[0];
        assert_eq!((m.name.as_str(), m.context_window, m.max_output), ("Y", Some(128000), Some(8192)));
    }
}
