// Talks to the Rust side: Tauri IPC inside the app, HTTP in the browser preview.

const tauri = window.__TAURI__;

async function post(name, args) {
  const response = await fetch(`/api/${name}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(args ?? null),
  });
  if (!response.ok) throw new Error(await response.text());
  return response.json();
}

/** Run a data command (books, chapter, search, strongs, lexicon, copy_text). */
export function call(name, args = {}) {
  return tauri ? tauri.core.invoke("call", { name, args }) : post(name, args);
}

export function loadSettings() {
  return tauri ? tauri.core.invoke("settings_load") : post("settings_load");
}

export function saveSettings(settings) {
  return tauri ? tauri.core.invoke("settings_save", { settings }) : post("settings_save", settings);
}

export async function copyText(text) {
  if (tauri) {
    await tauri.core.invoke("copy_to_clipboard", { text });
  } else {
    await navigator.clipboard.writeText(text);
  }
}

/** Open a link in the system browser. */
export function openExternal(url) {
  if (tauri) {
    tauri.core.invoke("open_url", { url });
  } else {
    window.open(url, "_blank", "noopener");
  }
}

// ------------------------------------------------------------------ AI assistant
// API keys go to the Rust side and stay there (the system keychain); the page only
// learns whether one is stored.

export function aiModels(args) {
  return tauri ? tauri.core.invoke("ai_models", { args }) : post("ai_models", args);
}

export function aiKeyStatus(providerId) {
  return tauri ? tauri.core.invoke("ai_key_status", { providerId }) : post("ai_key_status", { providerId });
}

/** Save (or with an empty key, remove) a provider's key. Resolves to "keychain" or "file". */
export function aiKeySet(providerId, key) {
  return tauri ? tauri.core.invoke("ai_key_set", { providerId, key }) : post("ai_key_set", { providerId, key });
}

export function aiKeyDelete(providerId) {
  return tauri ? tauri.core.invoke("ai_key_delete", { providerId }) : post("ai_key_delete", { providerId });
}

export function aiCancel(id) {
  return tauri ? tauri.core.invoke("ai_cancel", { id }) : post("ai_cancel", { id });
}

/**
 * Ask for an answer. `onEvent` receives {type: "text" | "reasoning" | "usage" | "done"}
 * as they arrive; the promise settles when the answer ends (or rejects with the error).
 */
export async function aiChat(id, args, onEvent) {
  if (tauri) {
    const channel = new tauri.core.Channel();
    channel.onmessage = onEvent;
    return tauri.core.invoke("ai_chat", { id, args, onEvent: channel });
  }
  const response = await fetch("/api/ai_chat", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ id, args }),
  });
  if (!response.ok) throw new Error(await response.text());
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  for (;;) {
    const { value, done } = await reader.read();
    buffer += decoder.decode(value ?? new Uint8Array(), { stream: !done });
    let newline;
    while ((newline = buffer.indexOf("\n")) >= 0) {
      const line = buffer.slice(0, newline).trim();
      buffer = buffer.slice(newline + 1);
      if (!line) continue;
      const event = JSON.parse(line);
      if (event.type === "error") throw new Error(event.message);
      onEvent(event);
    }
    if (done) return;
  }
}

// ------------------------------------------------------------------ saved conversations
// Kept on this device, in the app's private folder (conversations/).

export function conversationsList() {
  return tauri ? tauri.core.invoke("conversations_list") : post("conversations_list", {});
}

export function conversationLoad(id) {
  return tauri ? tauri.core.invoke("conversation_load", { id }) : post("conversation_load", { id });
}

export function conversationSave(conversation) {
  return tauri ? tauri.core.invoke("conversation_save", { conversation }) : post("conversation_save", { conversation });
}

export function conversationDelete(id) {
  return tauri ? tauri.core.invoke("conversation_delete", { id }) : post("conversation_delete", { id });
}
