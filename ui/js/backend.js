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
