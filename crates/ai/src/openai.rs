//! OpenAI-compatible chat completions: OpenAI, DeepSeek, OpenRouter, Groq, and local
//! servers (vLLM, Ollama, LM Studio, llama.cpp).

use serde_json::{Value, json};

use crate::think::ThinkSplitter;
use crate::{ChatRequest, Endpoint, Event, ModelInfo, Role, Usage, url};

pub fn request(client: &reqwest::Client, req: &ChatRequest, usage_option: bool) -> reqwest::RequestBuilder {
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
    if usage_option {
        body["stream_options"] = json!({"include_usage": true});
    }
    if let Some(max) = req.max_tokens {
        body["max_tokens"] = json!(max);
    }
    if let Some(on) = req.enable_thinking {
        body["chat_template_kwargs"] = json!({"enable_thinking": on});
    }
    if let Some(effort) = &req.effort {
        // OpenAI reasoning models; other servers ignore unknown fields or say so
        if req.endpoint.base_url.contains("api.openai.com") {
            body["reasoning_effort"] = json!(effort);
        }
    }
    auth(client.post(url(&req.endpoint.base_url, "chat/completions")), &req.endpoint).json(&body)
}

pub fn models_request(client: &reqwest::Client, endpoint: &Endpoint) -> reqwest::RequestBuilder {
    auth(client.get(url(&endpoint.base_url, "models")), endpoint)
}

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
    reason: Option<String>,
}

impl crate::Decoder for Decoder {
    fn decode(&mut self, data: &str, emit: &mut dyn FnMut(Event)) -> Result<bool, String> {
        if data.trim() == "[DONE]" {
            self.flush(emit);
            emit(Event::Done { reason: self.reason.take().or(Some("stop".into())) });
            return Ok(true);
        }
        let Ok(chunk) = serde_json::from_str::<Value>(data) else {
            return Ok(false);
        };
        if let Some(message) = chunk.pointer("/error/message").and_then(Value::as_str) {
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
                for (reasoning, text) in self.think.push(text) {
                    emit(if reasoning { Event::Reasoning { text } } else { Event::Text { text } });
                }
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

    fn flush(&mut self, emit: &mut dyn FnMut(Event)) {
        for (reasoning, text) in self.think.finish() {
            emit(if reasoning { Event::Reasoning { text } } else { Event::Text { text } });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Decoder as _;

    fn decode(lines: &[&str]) -> Vec<Event> {
        let mut d = Decoder::default();
        let mut events = Vec::new();
        for l in lines {
            d.decode(l, &mut |e| events.push(e)).unwrap();
        }
        events
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
