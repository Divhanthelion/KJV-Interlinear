//! Try a real server: `cargo run -p kjv-ai --example live -- openai http://192.168.1.20:8000/v1 [model]`
//! The API key comes from KJV_AI_KEY, which must be set (to nothing for a server that
//! needs no key).

use std::process::exit;

use kjv_ai::{ChatRequest, Endpoint, Event, Kind, Message, Role};

const USAGE: &str = "usage: live <openai|anthropic|gemini> <base url> [model]";

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let kind = match args.get(1).map(String::as_str) {
        Some("openai") => Kind::OpenAi,
        Some("anthropic") => Kind::Anthropic,
        Some("gemini") => Kind::Gemini,
        _ => fail(USAGE),
    };
    let Some(base_url) = args.get(2).cloned() else { fail(USAGE) };
    let Ok(key) = std::env::var("KJV_AI_KEY") else {
        fail("Set KJV_AI_KEY to the service's API key (set it empty for a server that needs no key).")
    };
    let endpoint = Endpoint { kind, base_url, api_key: Some(key).filter(|k| !k.trim().is_empty()) };
    let client = kjv_ai::client();
    let models = match kjv_ai::models(&client, &endpoint).await {
        Ok(models) => models,
        Err(e) => fail(&format!("Couldn't list the models: {}", e)),
    };
    for m in &models {
        println!("model {} ({}) context={:?} max_output={:?}", m.id, m.name, m.context_window, m.max_output);
    }
    let Some(model) = args.get(3).cloned().or_else(|| models.first().map(|m| m.id.clone())) else {
        fail(&format!("The server listed no models; name one.\n{}", USAGE))
    };
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
    let result = kjv_ai::chat(&client, &req, |e| {
        if first.is_none() {
            first = Some(started.elapsed());
        }
        match e {
            Event::Text { text } => print!("{}", text),
            Event::Reasoning { text } => print!("\x1b[2m{}\x1b[0m", text),
            other => println!("\n{:?}", other),
        }
    })
    .await;
    if let Err(e) = result {
        fail(&format!("\nThe chat failed: {}", e));
    }
    println!("\nfirst event after {:?}, total {:?}", first.unwrap_or_default(), started.elapsed());
}

fn fail(message: &str) -> ! {
    eprintln!("{}", message);
    exit(1)
}
