//! The app's Bible-study assistant: what the page sends (a provider, a model, the
//! context the reader chose, the conversation) becomes a provider request with the
//! context and instructions attached. Shared by the app and the browser preview server.

use serde::Deserialize;

use kjv_core::bundle::DataBundle;
use kjv_core::context::{self, Spec};
use kjv_library::Library;

use crate::{ChatRequest, Endpoint, Kind, Message};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskArgs {
    /// Which saved provider (its API key is looked up by this id, never sent by the page)
    pub provider_id: String,
    pub kind: Kind,
    pub base_url: String,
    pub model: String,
    /// What to attach: passages, translations, commentaries, cross-references
    #[serde(default)]
    pub context: Spec,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub thinking: bool,
    #[serde(default)]
    pub enable_thinking: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsArgs {
    pub provider_id: String,
    pub kind: Kind,
    pub base_url: String,
}

impl ModelsArgs {
    pub fn endpoint(&self, api_key: Option<String>) -> Endpoint {
        Endpoint { kind: self.kind, base_url: self.base_url.clone(), api_key }
    }
}

/// Build the provider request for `args`, attaching the context it asks for.
pub fn prepare(data: &DataBundle, lib: &Library, args: &AskArgs, api_key: Option<String>) -> Result<ChatRequest, String> {
    let built = context::build(data, lib, &args.context, None)?;
    Ok(ChatRequest {
        endpoint: Endpoint { kind: args.kind, base_url: args.base_url.clone(), api_key },
        model: args.model.clone(),
        instructions: context::instructions(lib, &built),
        context: built.text,
        messages: args.messages.clone(),
        max_tokens: args.max_tokens,
        effort: args.effort.clone(),
        thinking: args.thinking,
        enable_thinking: args.enable_thinking,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_carry_the_context() {
        let a: AskArgs = serde_json::from_value(serde_json::json!({
            "providerId": "p", "kind": "anthropic", "baseUrl": "https://x", "model": "m",
            "context": {
                "passages": [{ "bible": "web", "refs": "ROM.14", "commentaries": ["mhc"] }],
                "translations": ["web", "kjv"], "crossrefs": ["openbible"], "crossrefLimit": 5,
                "crossrefText": true, "original": true
            },
            "messages": [{ "role": "user", "content": "Why?" }]
        }))
        .unwrap();
        assert_eq!(a.context.passages[0].refs, "ROM.14");
        assert_eq!(a.context.passages[0].commentaries.as_deref(), Some(&["mhc".to_string()][..]));
        assert_eq!(a.context.passages[0].translations, None);
        assert_eq!((a.context.crossref_limit, a.context.crossref_text, a.context.original), (5, true, true));
        // No context: nothing attached
        let none: AskArgs = serde_json::from_value(serde_json::json!({
            "providerId": "p", "kind": "openai", "baseUrl": "x", "model": "m", "messages": []
        }))
        .unwrap();
        assert!(none.context.passages.is_empty());
    }
}
