//! Anthropic Messages API (raw HTTP; Anthropic has no official Rust SDK).

use serde_json::{Value, json};

use crate::{ChatRequest, Endpoint, Event, ModelInfo, Role, Usage, secret_header, url};

const VERSION: &str = "2023-06-01";
/// Models that take `fallbacks: "default"`: if a safety classifier declines a
/// harmless Bible question, the API answers with a suitable model instead.
const FALLBACK_MODELS: [&str; 4] = ["claude-fable-5-1", "claude-opus-5-5", "claude-opus-5", "claude-sonnet-5-5"];
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";

pub fn request(client: &reqwest::Client, req: &ChatRequest) -> reqwest::RequestBuilder {
    let mut system = vec![json!({"type": "text", "text": req.instructions})];
    if !req.context.is_empty() {
        // The Scripture is the big, stable part: cache it so follow-up questions
        // don't pay for it again
        system.push(json!({"type": "text", "text": req.context, "cache_control": {"type": "ephemeral"}}));
    }
    let messages: Vec<Value> = req
        .messages
        .iter()
        .map(|m| {
            json!({
                "role": match m.role { Role::User => "user", Role::Assistant => "assistant" },
                "content": m.content,
            })
        })
        .collect();
    let mut body = json!({
        "model": req.model,
        "max_tokens": req.max_tokens.unwrap_or(16000),
        "stream": true,
        "system": system,
        "messages": messages,
    });
    if req.thinking {
        body["thinking"] = json!({"type": "adaptive", "display": "summarized"});
    }
    if let Some(effort) = &req.effort {
        body["output_config"] = json!({"effort": effort});
    }
    let mut request = headers(client.post(url(&req.endpoint.base_url, "messages")), &req.endpoint);
    if FALLBACK_MODELS.contains(&req.model.as_str()) {
        body["fallbacks"] = json!("default");
        request = request.header("anthropic-beta", FALLBACK_BETA);
    }
    request.json(&body)
}

pub fn models_request(client: &reqwest::Client, endpoint: &Endpoint) -> reqwest::RequestBuilder {
    headers(client.get(url(&endpoint.base_url, "models?limit=1000")), endpoint)
}

fn headers(request: reqwest::RequestBuilder, endpoint: &Endpoint) -> reqwest::RequestBuilder {
    request
        .header("x-api-key", secret_header(endpoint.api_key.as_deref().unwrap_or("").trim()))
        .header("anthropic-version", VERSION)
}

pub fn parse_models(json: &Value) -> Vec<ModelInfo> {
    json.get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let id = m.get("id").and_then(Value::as_str)?.to_string();
            let supported = |path: &str| m.pointer(path).and_then(Value::as_bool).unwrap_or(false);
            Some(ModelInfo {
                name: m.get("display_name").and_then(Value::as_str).unwrap_or(&id).to_string(),
                context_window: m.get("max_input_tokens").and_then(Value::as_u64),
                max_output: m.get("max_tokens").and_then(Value::as_u64),
                adaptive_thinking: supported("/capabilities/thinking/types/adaptive/supported"),
                effort: supported("/capabilities/effort/supported"),
                id,
            })
        })
        .collect()
}

#[derive(Default)]
pub struct Decoder {
    /// Prompt tokens from `message_start`, including cache reads and writes
    input: Option<u64>,
    cached: Option<u64>,
}

impl crate::Decoder for Decoder {
    fn decode(&mut self, data: &str, emit: &mut dyn FnMut(Event)) -> Result<bool, String> {
        let Ok(event) = serde_json::from_str::<Value>(data) else {
            return Ok(false);
        };
        match event.get("type").and_then(Value::as_str).unwrap_or("") {
            "message_start" => {
                let usage = &event["message"]["usage"];
                let n = |k: &str| usage.get(k).and_then(Value::as_u64).unwrap_or(0);
                self.input = Some(n("input_tokens") + n("cache_read_input_tokens") + n("cache_creation_input_tokens"));
                self.cached = usage.get("cache_read_input_tokens").and_then(Value::as_u64);
            }
            "content_block_delta" => {
                let delta = &event["delta"];
                match delta.get("type").and_then(Value::as_str) {
                    Some("text_delta") => {
                        if let Some(text) = delta.get("text").and_then(Value::as_str) {
                            emit(Event::Text { text: text.to_string() });
                        }
                    }
                    Some("thinking_delta") => {
                        if let Some(text) = delta.get("thinking").and_then(Value::as_str).filter(|t| !t.is_empty()) {
                            emit(Event::Reasoning { text: text.to_string() });
                        }
                    }
                    _ => {}
                }
            }
            "message_delta" => {
                emit(Event::Usage {
                    usage: Usage {
                        input_tokens: self.input,
                        output_tokens: event.pointer("/usage/output_tokens").and_then(Value::as_u64),
                        cached_tokens: self.cached,
                    },
                });
                if let Some(reason) = event.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    let reason = match reason {
                        "end_turn" | "stop_sequence" => "stop",
                        "max_tokens" | "model_context_window_exceeded" => "length",
                        other => other,
                    };
                    emit(Event::Done { reason: Some(reason.to_string()) });
                    return Ok(true);
                }
            }
            "error" => {
                let message = event.pointer("/error/message").and_then(Value::as_str).unwrap_or("unknown error");
                return Err(match event.pointer("/error/type").and_then(Value::as_str) {
                    Some("overloaded_error") => "Anthropic is overloaded right now. Try again shortly.".to_string(),
                    _ => format!("The model stopped with an error: {}", message),
                });
            }
            _ => {}
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Decoder as _;

    #[test]
    fn streams_thinking_text_cached_usage_and_refusal() {
        let mut d = Decoder::default();
        let mut events = Vec::new();
        let mut done = false;
        for line in [
            r#"{"type":"message_start","message":{"usage":{"input_tokens":20,"cache_read_input_tokens":5000,"cache_creation_input_tokens":0}}}"#,
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Checking."}}"#,
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"x"}}"#,
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Amen."}}"#,
            r#"{"type":"message_delta","delta":{"stop_reason":"refusal"},"usage":{"output_tokens":7}}"#,
        ] {
            done |= d.decode(line, &mut |e| events.push(e)).unwrap();
        }
        assert!(done);
        assert_eq!(
            events,
            vec![
                Event::Reasoning { text: "Checking.".into() },
                Event::Text { text: "Amen.".into() },
                Event::Usage {
                    usage: Usage { input_tokens: Some(5020), output_tokens: Some(7), cached_tokens: Some(5000) }
                },
                Event::Done { reason: Some("refusal".into()) },
            ]
        );
    }

    #[test]
    fn stream_errors_are_reported() {
        let mut d = Decoder::default();
        let err = d
            .decode(r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#, &mut |_| {})
            .unwrap_err();
        assert!(err.contains("overloaded"));
    }

    #[test]
    fn model_list_carries_context_and_capabilities() {
        let list = parse_models(&json!({"data": [{
            "id": "claude-opus-5-5", "display_name": "Claude Opus 5.5",
            "max_input_tokens": 1000000, "max_tokens": 128000,
            "capabilities": {"thinking": {"supported": true, "types": {"adaptive": {"supported": true}}},
                             "effort": {"supported": true}}
        }]}));
        let m = &list[0];
        assert_eq!(
            (m.context_window, m.max_output, m.adaptive_thinking, m.effort),
            (Some(1000000), Some(128000), true, true)
        );
    }
}
