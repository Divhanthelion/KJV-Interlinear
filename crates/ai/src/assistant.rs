//! The app's Bible-study assistant: what the page sends (a provider, a model, a
//! Scripture scope, the conversation) becomes a provider request with the Scripture
//! text and instructions attached. Shared by the app and the browser preview server.

use serde::Deserialize;

use kjv_core::bundle::DataBundle;
use kjv_core::context::{self, ContextOptions, Scope};

use crate::{ChatRequest, Endpoint, Kind, Message};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskArgs {
    /// Which saved provider (its API key is looked up by this id, never sent by the page)
    pub provider_id: String,
    pub kind: Kind,
    pub base_url: String,
    pub model: String,
    pub scope: Scope,
    #[serde(default)]
    pub context_options: ContextOptions,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub thinking: bool,
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

/// Build the provider request for `args`, attaching the Scripture for its scope.
pub fn prepare(data: &DataBundle, args: &AskArgs, api_key: Option<String>) -> Result<ChatRequest, String> {
    let scripture = context::build(data, &args.scope, &args.context_options)?;
    let context = if scripture.text.is_empty() {
        String::new()
    } else {
        format!("<scripture scope=\"{}\">\n{}</scripture>", scripture.label, scripture.text)
    };
    Ok(ChatRequest {
        endpoint: Endpoint { kind: args.kind, base_url: args.base_url.clone(), api_key },
        model: args.model.clone(),
        instructions: instructions(&scripture.label, args.context_options.original),
        context,
        messages: args.messages.clone(),
        max_tokens: args.max_tokens,
        effort: args.effort.clone(),
        thinking: args.thinking,
    })
}

/// How the assistant should behave. Kept free of anything that changes from turn to
/// turn so providers can cache it with the Scripture that follows.
pub fn instructions(scope_label: &str, original: bool) -> String {
    let mut s = String::from(
        "You are the study assistant in KJV Interlinear, a Bible app built on the King James Version \
         (1769 Oxford text) with the Hebrew, Aramaic, and Greek beneath it.\n\n",
    );
    if scope_label.is_empty() {
        s.push_str(
            "No passage is attached to this conversation. Answer from your knowledge of the Bible, and \
             give references (Book chapter:verse) the reader can check.\n\n",
        );
    } else {
        s.push_str(&format!(
            "The reader has attached {} below, inside <scripture>. It is the text under discussion. \
             Headings mark books (#) and chapters (##); each line starts with its verse number, and \
             \"(title)\" marks a psalm's title. When you quote Scripture, quote this text exactly and \
             give the reference (Book chapter:verse).",
            scope_label
        ));
        if original {
            s.push_str(
                " Each verse is followed by its original-language words, each with its Strong's number \
                 and a short English gloss; use them when the original language matters.",
            );
        }
        s.push_str(
            " If a question needs passages that aren't attached, you may draw on your wider knowledge \
             of the Bible, but say which references you are citing from memory so the reader can \
             check them.\n\n",
        );
    }
    s.push_str(
        "Guidelines:\n\
         - Distinguish what the text says from how it has been interpreted. Where Christian traditions \
         read a passage differently, say so briefly and fairly instead of presenting one view as the \
         only one.\n\
         - Be careful with the original languages: don't overstate what a word means, and say when a \
         point goes beyond the glosses and standard lexicons.\n\
         - If you are unsure of a fact, a date, or a reference, say so.\n\
         - Answer the question asked. Use short paragraphs, and lists or headings only when they help.",
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instructions_name_the_scope_and_original_language_notes() {
        let s = instructions("John 3", true);
        assert!(s.contains("attached John 3 below"));
        assert!(s.contains("Strong's number"));
        let none = instructions("", false);
        assert!(none.contains("No passage is attached"));
        assert!(!none.contains("<scripture>"));
    }
}
