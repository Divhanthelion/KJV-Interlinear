//! Google Gemini API (`generativelanguage.googleapis.com`).

use serde_json::{Value, json};

use crate::{ChatRequest, Endpoint, Event, ModelInfo, Role, Usage, secret_header, url};

pub fn request(client: &reqwest::Client, req: &ChatRequest) -> reqwest::RequestBuilder {
    let mut system = req.instructions.clone();
    if !req.context.is_empty() {
        system.push_str("\n\n");
        system.push_str(&req.context);
    }
    let contents: Vec<Value> = req
        .messages
        .iter()
        .map(|m| {
            json!({
                "role": match m.role { Role::User => "user", Role::Assistant => "model" },
                "parts": [{"text": m.content}],
            })
        })
        .collect();
    let mut config = json!({});
    if let Some(max) = req.max_tokens {
        config["maxOutputTokens"] = json!(max);
    }
    if req.thinking {
        config["thinkingConfig"] = json!({"includeThoughts": true});
    }
    let body = json!({
        "systemInstruction": {"parts": [{"text": system}]},
        "contents": contents,
        "generationConfig": config,
    });
    let model = req.model.trim_start_matches("models/");
    key(
        client.post(url(&req.endpoint.base_url, &format!("models/{}:streamGenerateContent?alt=sse", model))),
        &req.endpoint,
    )
    .json(&body)
}

pub fn models_request(client: &reqwest::Client, endpoint: &Endpoint) -> reqwest::RequestBuilder {
    key(client.get(url(&endpoint.base_url, "models?pageSize=1000")), endpoint)
}

fn key(request: reqwest::RequestBuilder, endpoint: &Endpoint) -> reqwest::RequestBuilder {
    request.header("x-goog-api-key", secret_header(endpoint.api_key.as_deref().unwrap_or("").trim()))
}

/// Chat models only (the list also has embedding and image models).
pub fn parse_models(json: &Value) -> Vec<ModelInfo> {
    json.get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|m| {
            m.get("supportedGenerationMethods")
                .and_then(Value::as_array)
                .is_some_and(|methods| methods.iter().any(|x| x == "generateContent"))
        })
        .filter_map(|m| {
            let id = m.get("name").and_then(Value::as_str)?.trim_start_matches("models/").to_string();
            Some(ModelInfo {
                name: m.get("displayName").and_then(Value::as_str).unwrap_or(&id).to_string(),
                context_window: m.get("inputTokenLimit").and_then(Value::as_u64),
                max_output: m.get("outputTokenLimit").and_then(Value::as_u64),
                adaptive_thinking: m.get("thinking").and_then(Value::as_bool).unwrap_or(false),
                effort: false,
                id,
            })
        })
        .collect()
}

/// Gemini says why it stopped (`finishReason`) in the last chunk, often alongside the
/// final usage, so `Done` waits for the end of the stream.
#[derive(Default)]
pub struct Decoder {
    reason: Option<String>,
}

impl crate::Decoder for Decoder {
    fn decode(&mut self, data: &str, emit: &mut dyn FnMut(Event)) -> Result<bool, String> {
        let Ok(chunk) = serde_json::from_str::<Value>(data) else {
            return Ok(false);
        };
        if let Some(message) = chunk.pointer("/error/message").and_then(Value::as_str) {
            return Err(format!("The model stopped with an error: {}", message));
        }
        if chunk.pointer("/promptFeedback/blockReason").is_some() {
            self.reason = Some("refusal".into());
        }
        if let Some(candidate) = chunk.pointer("/candidates/0") {
            for part in candidate.pointer("/content/parts").and_then(Value::as_array).into_iter().flatten() {
                let Some(text) = part.get("text").and_then(Value::as_str).filter(|t| !t.is_empty()) else {
                    continue;
                };
                let text = text.to_string();
                emit(if part.get("thought").and_then(Value::as_bool) == Some(true) {
                    Event::Reasoning { text }
                } else {
                    Event::Text { text }
                });
            }
            if let Some(reason) = candidate.get("finishReason").and_then(Value::as_str) {
                self.reason = Some(match reason {
                    "STOP" => "stop".into(),
                    "MAX_TOKENS" => "length".into(),
                    "SAFETY" | "RECITATION" | "PROHIBITED_CONTENT" | "BLOCKLIST" | "SPII" => "refusal".into(),
                    other => other.to_lowercase(),
                });
            }
        }
        if let Some(u) = chunk.get("usageMetadata") {
            let n = |k: &str| u.get(k).and_then(Value::as_u64);
            emit(Event::Usage {
                usage: Usage {
                    input_tokens: n("promptTokenCount"),
                    output_tokens: match (n("candidatesTokenCount"), n("thoughtsTokenCount")) {
                        (None, None) => None,
                        (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
                    },
                    cached_tokens: n("cachedContentTokenCount"),
                },
            });
        }
        Ok(false)
    }

    /// `Done` with the finish reason; true when there was one, so the caller doesn't add
    /// a second `Done`. No reason at all means the stream was cut short.
    fn flush(&mut self, emit: &mut dyn FnMut(Event)) -> bool {
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

    #[test]
    fn streams_thoughts_text_and_finish() {
        let mut d = Decoder::default();
        let mut events = Vec::new();
        for line in [
            r#"{"candidates":[{"content":{"parts":[{"text":"Weighing it.","thought":true}],"role":"model"}}]}"#,
            r#"{"candidates":[{"content":{"parts":[{"text":"Selah."}],"role":"model"},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":50,"candidatesTokenCount":3,"thoughtsTokenCount":10}}"#,
        ] {
            assert!(!d.decode(line, &mut |e| events.push(e)).unwrap());
        }
        assert!(d.flush(&mut |e| events.push(e)), "a finish reason ends the reply");
        assert_eq!(
            events,
            vec![
                Event::Reasoning { text: "Weighing it.".into() },
                Event::Text { text: "Selah.".into() },
                Event::Usage { usage: Usage { input_tokens: Some(50), output_tokens: Some(13), cached_tokens: None } },
                Event::Done { reason: Some("stop".into()) },
            ]
        );
    }

    #[test]
    fn no_finish_reason_leaves_the_ending_to_the_caller() {
        let mut d = Decoder::default();
        let mut events = Vec::new();
        d.decode(r#"{"candidates":[{"content":{"parts":[{"text":"In the"}]}}]}"#, &mut |e| events.push(e)).unwrap();
        assert!(!d.flush(&mut |e| events.push(e)));
        assert_eq!(events, vec![Event::Text { text: "In the".into() }]);
    }

    #[test]
    fn only_chat_models_are_listed() {
        let list = parse_models(&json!({"models": [
            {"name": "models/gemini-3-pro", "displayName": "Gemini 3 Pro", "inputTokenLimit": 1048576,
             "outputTokenLimit": 65536, "supportedGenerationMethods": ["generateContent", "countTokens"], "thinking": true},
            {"name": "models/text-embedding-004", "supportedGenerationMethods": ["embedContent"]}
        ]}));
        assert_eq!(list.len(), 1);
        assert_eq!((list[0].id.as_str(), list[0].context_window), ("gemini-3-pro", Some(1048576)));
    }
}
