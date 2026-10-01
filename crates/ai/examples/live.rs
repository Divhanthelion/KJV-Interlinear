//! Try a real server: `cargo run -p kjv-ai --example live -- openai http://192.168.1.20:8000/v1 [model]`
//! The API key, if any, comes from KJV_AI_KEY.

use kjv_ai::{ChatRequest, Endpoint, Event, Kind, Message, Role};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let kind = match args.get(1).map(String::as_str) {
        Some("anthropic") => Kind::Anthropic,
        Some("gemini") => Kind::Gemini,
        _ => Kind::OpenAi,
    };
    let endpoint = Endpoint {
        kind,
        base_url: args.get(2).cloned().expect("usage: live <openai|anthropic|gemini> <base url> [model]"),
        api_key: std::env::var("KJV_AI_KEY").ok(),
    };
    let client = kjv_ai::client();
    let models = kjv_ai::models(&client, &endpoint).await.expect("model list");
    for m in &models {
        println!("model {} ({}) context={:?} max_output={:?}", m.id, m.name, m.context_window, m.max_output);
    }
    let model = args.get(3).cloned().unwrap_or_else(|| models[0].id.clone());
    let req = ChatRequest {
        endpoint,
        model,
        instructions: "You are a Bible study assistant. Answer in two sentences.".into(),
        context: "# John\n## John 11\n35 Jesus wept.\n".into(),
        messages: vec![Message { role: Role::User, content: "Why did Jesus weep here?".into() }],
        max_tokens: Some(2000),
        effort: None,
        thinking: false,
        enable_thinking: None,
    };
    let started = std::time::Instant::now();
    let mut first = None;
    kjv_ai::chat(&client, &req, |e| {
        if first.is_none() {
            first = Some(started.elapsed());
        }
        match e {
            Event::Text { text } => print!("{}", text),
            Event::Reasoning { text } => print!("\x1b[2m{}\x1b[0m", text),
            other => println!("\n{:?}", other),
        }
    })
    .await
    .expect("chat");
    println!("\nfirst event after {:?}, total {:?}", first.unwrap_or_default(), started.elapsed());
}
