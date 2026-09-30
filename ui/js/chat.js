// The study assistant: chat with a language model about a verse, a chapter, some
// books, or the whole Bible, using the reader's own AI provider (a local server or
// their API key). Also the provider setup shown in Settings.

import { aiCancel, aiChat, aiKeySet, aiKeyStatus, aiModels, call, copyText, openExternal } from "./backend.js";
import { h, icon, replace } from "./dom.js";
import { referenceFinder, renderMarkdown } from "./markdown.js";

export const PRESETS = [
  {
    id: "local",
    label: "My own server",
    kind: "openai",
    baseUrl: "",
    keyOptional: true,
    placeholder: "http://192.168.1.20:8000/v1",
    hint: "vLLM, Ollama, LM Studio, llama.cpp, or any server with an OpenAI-style API. The address usually ends in /v1.",
  },
  { id: "anthropic", label: "Anthropic (Claude)", kind: "anthropic", baseUrl: "https://api.anthropic.com/v1", model: "claude-opus-5-5" },
  { id: "openai", label: "OpenAI", kind: "openai", baseUrl: "https://api.openai.com/v1" },
  { id: "gemini", label: "Google Gemini", kind: "gemini", baseUrl: "https://generativelanguage.googleapis.com/v1beta" },
  { id: "deepseek", label: "DeepSeek", kind: "openai", baseUrl: "https://api.deepseek.com/v1" },
  { id: "openrouter", label: "OpenRouter", kind: "openai", baseUrl: "https://openrouter.ai/api/v1" },
  { id: "groq", label: "Groq", kind: "openai", baseUrl: "https://api.groq.com/openai/v1" },
  {
    id: "custom",
    label: "Other (OpenAI-compatible)",
    kind: "openai",
    baseUrl: "",
    keyOptional: true,
    placeholder: "https://example.com/v1",
    hint: "Any service with an OpenAI-style chat API.",
  },
];

const presetOf = (p) => PRESETS.find((x) => x.id === p.preset) ?? PRESETS.at(-1);

/** Longest answer to ask for, and the room kept free for it in the context window. */
const ANSWER_TOKENS = 16000;
/** Instructions and wrapping around the Scripture, in tokens (see crates/ai/src/assistant.rs). */
const INSTRUCTION_TOKENS = 450;

const chat = {
  messages: [], // { role, content, reasoning, scopeLabel, usage, error, reason, streaming }
  requestId: null,
  pendingConsent: null, // text waiting for the reader to allow a provider
  models: new Map(), // providerId -> { list, error, loading }
  size: null, // { key, label, tokens, verses } for the current scope
  showScope: false,
  draft: "",
  view: null, // current DOM references
};

let finder = null;

// ------------------------------------------------------------------ helpers

const estimateTokens = (text) => Math.ceil(text.length / 3.8);

function provider(ctx) {
  const ai = ctx.settings.ai;
  return ai.providers.find((p) => p.id === ai.providerId) ?? null;
}

function modelInfo(ctx) {
  const p = provider(ctx);
  const list = p ? chat.models.get(p.id)?.list : null;
  return list?.find((m) => m.id === ctx.settings.ai.model) ?? null;
}

function contextWindow(ctx) {
  const p = provider(ctx);
  return p?.contextWindow ?? modelInfo(ctx)?.contextWindow ?? null;
}

function calibrationKey(ctx) {
  return `${ctx.settings.ai.providerId}|${ctx.settings.ai.model}`;
}

/** The scope as the Rust side takes it, following the reader's place. */
function scopeArgs(ctx) {
  const ai = ctx.settings.ai;
  const ch = ctx.state.chapter;
  if (!ch && ai.scope !== "bible" && ai.scope !== "books") return { kind: "none" };
  switch (ai.scope) {
    case "verse": {
      const verse = ctx.state.selectedVerse ?? ctx.settings.position.verse ?? 1;
      return { kind: "verse", book: ch.book, chapter: ch.chapter, verse };
    }
    case "chapter":
      return { kind: "chapter", book: ch.book, chapter: ch.chapter };
    case "book":
      return { kind: "book", book: ch.book };
    case "books":
      return ai.books.length ? { kind: "books", books: ai.books } : { kind: "none" };
    case "bible":
      return { kind: "bible" };
    default:
      return { kind: "none" };
  }
}

async function refreshSize(ctx) {
  const scope = scopeArgs(ctx);
  const key = JSON.stringify([scope, ctx.settings.ai.original]);
  if (chat.size?.key === key) return;
  if (scope.kind === "none") {
    chat.size = { key, label: "", tokens: 0, verses: 0 };
  } else {
    try {
      const size = await call("context_size", { scope, options: { original: ctx.settings.ai.original } });
      chat.size = { key, ...size };
    } catch (error) {
      chat.size = { key, label: "", tokens: 0, verses: 0, error: String(error.message ?? error) };
    }
  }
  drawBudget(ctx);
}

/** Tokens the next request will use before the answer, corrected for this model. */
function promptTokens(ctx, extraText = "") {
  const factor = ctx.settings.ai.calibration[calibrationKey(ctx)] ?? 1;
  const history = chat.messages.reduce((n, m) => n + estimateTokens(m.content ?? ""), 0);
  return Math.ceil(((chat.size?.tokens ?? 0) + INSTRUCTION_TOKENS + history + estimateTokens(extraText)) * factor);
}

function answerTokens(ctx) {
  const max = modelInfo(ctx)?.maxOutput;
  return max ? Math.min(max, ANSWER_TOKENS) : ANSWER_TOKENS;
}

/** 1,085,845 -> "1.09M", 13,939 -> "14k". Limits round down (never overstate the room). */
const compact = (n, round = Math.round) =>
  n >= 1e6 ? `${(round(n / 1e4) / 100).toFixed(2).replace(/\.?0+$/, "")}M` : n >= 1e3 ? `${round(n / 1e3)}k` : String(n);
const compactLimit = (n) => compact(n, Math.floor);

// ------------------------------------------------------------------ chat panel

export function renderChat(body, ctx) {
  finder ??= referenceFinder(ctx.state.books);
  const ai = ctx.settings.ai;
  if (!ai.providers.length) {
    chat.view = null;
    replace(
      body,
      h(
        "div",
        { class: "chat-empty" },
        h("p", {}, "Ask questions about a verse, a chapter, whole books, or the entire Bible, with the text attached for the model to read."),
        h("p", {}, "Use your own server (such as vLLM or Ollama on your network) or an API key from Anthropic, OpenAI, Google, DeepSeek, OpenRouter, or Groq. The app has no AI service of its own and never sees your questions."),
        h("button", { type: "button", class: "button primary", onclick: () => ctx.openPanel("settings", { section: "ai" }) }, "Set up an AI provider"),
      ),
    );
    return null;
  }

  const input = h("textarea", {
    id: "chat-input",
    class: "chat-input",
    rows: "1",
    placeholder: "Ask about the text…",
    "aria-label": "Your question",
    enterkeyhint: "send",
  });
  input.value = chat.draft;
  input.addEventListener("input", () => {
    chat.draft = input.value;
    autosize(input);
    drawBudget(ctx);
  });
  input.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
      event.preventDefault();
      send(ctx);
    }
  });

  const sendButton = h("button", { type: "button", class: "icon-btn chat-send", onclick: () => (chat.requestId ? stop() : send(ctx)) });
  const messages = h("div", { class: "chat-messages", role: "log", "aria-live": "polite", "aria-relevant": "additions" });
  const budget = h("div", { class: "chat-budget" });
  const scopeEditor = h("div", { class: "chat-scope", hidden: !chat.showScope });
  const consent = h("div", { class: "chat-consent", hidden: true });

  chat.view = { body, input, sendButton, messages, budget, scopeEditor, consent };

  replace(
    body,
    h(
      "div",
      { class: "chat" },
      h("div", { class: "chat-top" }, modelPicker(ctx), budget, scopeEditor),
      messages,
      consent,
      h("div", { class: "chat-compose" }, input, sendButton),
    ),
  );
  drawScopeEditor(ctx);
  drawMessages(ctx);
  drawSend();
  drawConsent(ctx);
  refreshSize(ctx);
  loadModels(ctx);
  autosize(input);
  drawBudget(ctx);
  return input;
}

/** The reader moved (new chapter or verse): the attached scope may have changed. */
export function chatScopeChanged(ctx) {
  if (chat.view) refreshSize(ctx);
}

function autosize(input) {
  input.style.height = "auto";
  input.style.height = `${Math.min(input.scrollHeight, 180)}px`;
}

function modelPicker(ctx) {
  const ai = ctx.settings.ai;
  const select = h("select", { class: "chat-model", "aria-label": "Model" });
  for (const p of ai.providers) {
    const state = chat.models.get(p.id);
    const group = h("optgroup", { label: p.name });
    const ids = new Set();
    for (const m of state?.list ?? []) {
      ids.add(m.id);
      group.append(h("option", { value: `${p.id}\n${m.id}` }, m.name === m.id ? m.id : `${m.name}`));
    }
    // Keep the saved choice visible while the list loads (or if it failed)
    if (p.id === ai.providerId && ai.model && !ids.has(ai.model)) {
      group.append(h("option", { value: `${p.id}\n${ai.model}` }, ai.model));
    }
    if (!group.children.length) {
      group.append(h("option", { value: `${p.id}\n`, disabled: true }, state?.loading ? "Loading models…" : state?.error ? "Couldn't load models" : "No models"));
    }
    select.append(group);
  }
  select.value = `${ai.providerId}\n${ai.model ?? ""}`;
  select.addEventListener("change", () => {
    const [providerId, model] = select.value.split("\n");
    ctx.changeSettings((s) => {
      s.ai.providerId = providerId;
      s.ai.model = model || null;
    });
    loadModels(ctx);
    drawBudget(ctx);
  });
  const status = chat.models.get(ai.providerId);
  return h(
    "div",
    { class: "chat-model-row" },
    select,
    h(
      "button",
      {
        type: "button",
        class: "icon-btn",
        "aria-label": "New conversation",
        title: "New conversation",
        disabled: !chat.messages.length || !!chat.requestId,
        onclick: () => {
          chat.messages = [];
          ctx.refreshPanel();
          chat.view?.input.focus();
        },
      },
      icon("plus"),
    ),
    status?.error
      ? h(
          "p",
          { class: "chat-error small" },
          status.error,
          " ",
          h("button", { type: "button", class: "text-button", onclick: () => loadModels(ctx, { force: true }) }, "Retry"),
        )
      : null,
  );
}

function loadModels(ctx, { force = false } = {}) {
  const p = provider(ctx);
  if (!p) return Promise.resolve();
  const existing = chat.models.get(p.id);
  if (existing?.loading) return existing.loading;
  if (existing?.list && !force) return Promise.resolve();
  const loading = (async () => {
    try {
      const list = await aiModels({ providerId: p.id, kind: p.kind, baseUrl: p.baseUrl });
      chat.models.set(p.id, { list });
      // First use: pick the preset's default, or the first model
      const ai = ctx.settings.ai;
      if (ai.providerId === p.id && (!ai.model || !list.some((m) => m.id === ai.model))) {
        const preferred = list.find((m) => m.id === presetOf(p).model) ?? list[0];
        if (preferred) ctx.changeSettings((s) => { s.ai.model = preferred.id; });
      }
    } catch (error) {
      chat.models.set(p.id, { error: String(error.message ?? error) });
    }
    if (ctx.state.panel === "chat") ctx.refreshPanel();
  })();
  chat.models.set(p.id, { loading });
  return loading;
}

// ------------------------------------------------------------------ scope

const SCOPES = [
  ["none", "Nothing"],
  ["verse", "This verse"],
  ["chapter", "This chapter"],
  ["book", "This book"],
  ["books", "Choose books"],
  ["bible", "Whole Bible"],
];

function scopeName(ctx) {
  const ai = ctx.settings.ai;
  if (ai.scope === "none") return "No passage attached";
  if (chat.size?.label) return chat.size.label;
  return SCOPES.find(([id]) => id === ai.scope)[1];
}

function drawBudget(ctx) {
  const v = chat.view;
  if (!v) return;
  const limit = contextWindow(ctx);
  const used = promptTokens(ctx, v.input.value);
  const fits = !limit || used + answerTokens(ctx) <= limit;
  // Share of the room left after the answer's reserve
  const fraction = limit ? Math.min(1, used / Math.max(1, limit - answerTokens(ctx))) : 0;
  const detail =
    ctx.settings.ai.scope === "none"
      ? ""
      : limit
        ? `≈${compact(used)} of ${compactLimit(limit)} tokens`
        : `≈${compact(used)} tokens`;
  replace(
    v.budget,
    h(
      "button",
      {
        type: "button",
        class: "chat-scope-button",
        "aria-expanded": String(chat.showScope),
        onclick: () => {
          chat.showScope = !chat.showScope;
          v.scopeEditor.hidden = !chat.showScope;
          drawBudget(ctx);
        },
      },
      h("span", { class: "chat-scope-label" }, h("span", { class: "muted" }, "Attached: "), scopeName(ctx)),
      h("span", { class: `chat-scope-size${fits ? "" : " over"}` }, detail),
      h("span", { class: "ref-caret", "aria-hidden": "true" }),
    ),
    limit && ctx.settings.ai.scope !== "none" ? meter(fraction, fits) : null,
    fits
      ? null
      : h(
          "p",
          { class: "chat-error small" },
          `Too large for this model: it reads ${compactLimit(limit)} tokens, and ${compact(answerTokens(ctx))} are kept free for the answer. Attach less, or choose a model with a larger context window.`,
        ),
  );
  drawSend();
}

/** The app's content security policy blocks inline style attributes; set it via the DOM. */
function meter(fraction, fits) {
  const fill = h("span", {});
  fill.style.width = `${(fraction * 100).toFixed(1)}%`;
  return h("div", { class: `meter${fits ? "" : " over"}`, role: "presentation" }, fill);
}

function drawScopeEditor(ctx) {
  const v = chat.view;
  if (!v) return;
  const ai = ctx.settings.ai;
  const set = (mutator) => {
    ctx.changeSettings(mutator);
    drawScopeEditor(ctx);
    refreshSize(ctx);
    drawBudget(ctx);
  };
  const books = ctx.state.books;
  const old = books.filter((b) => b.testament === "old").map((b) => b.name);
  const nt = books.filter((b) => b.testament === "new").map((b) => b.name);
  const chosen = new Set(ai.books);
  const setBooks = (names) => set((s) => { s.ai.books = books.map((b) => b.name).filter((n) => names.has(n)); });
  const all = (names) => names.every((n) => chosen.has(n));

  replace(
    v.scopeEditor,
    h(
      "div",
      { class: "segmented wrap", role: "radiogroup", "aria-label": "Attach" },
      SCOPES.map(([id, label]) =>
        h("button", { type: "button", role: "radio", "aria-checked": String(ai.scope === id), onclick: () => set((s) => { s.ai.scope = id; }) }, label),
      ),
    ),
    ai.scope === "books"
      ? h(
          "div",
          { class: "book-choices" },
          h(
            "div",
            { class: "book-choice-actions" },
            h("button", { type: "button", class: "chip", "aria-pressed": String(all(old)), onclick: () => {
              const next = new Set(chosen);
              for (const n of old) all(old) ? next.delete(n) : next.add(n);
              setBooks(next);
            } }, "Old Testament"),
            h("button", { type: "button", class: "chip", "aria-pressed": String(all(nt)), onclick: () => {
              const next = new Set(chosen);
              for (const n of nt) all(nt) ? next.delete(n) : next.add(n);
              setBooks(next);
            } }, "New Testament"),
            h("button", { type: "button", class: "chip", disabled: !chosen.size, onclick: () => setBooks(new Set()) }, "Clear"),
          ),
          h(
            "div",
            { class: "book-checks" },
            books.map((b) =>
              h(
                "button",
                {
                  type: "button",
                  class: "chip",
                  "aria-pressed": String(chosen.has(b.name)),
                  onclick: () => {
                    const next = new Set(chosen);
                    next.has(b.name) ? next.delete(b.name) : next.add(b.name);
                    setBooks(next);
                  },
                },
                b.display,
              ),
            ),
          ),
        )
      : null,
    h(
      "div",
      { class: "setting" },
      h("span", { class: "setting-label", id: "chat-original-label" }, "Include Hebrew & Greek", h("span", { class: "setting-hint" }, "Each verse's original words with Strong's numbers. About 3× larger.")),
      h("button", {
        class: "switch",
        type: "button",
        role: "switch",
        "aria-checked": String(ai.original),
        "aria-labelledby": "chat-original-label",
        onclick: () => set((s) => { s.ai.original = !s.ai.original; }),
      }),
    ),
  );
}

// ------------------------------------------------------------------ messages

function drawMessages(ctx) {
  const v = chat.view;
  if (!v) return;
  replace(v.messages, chat.messages.length ? chat.messages.map((m, i) => messageNode(ctx, m, i)) : hints(ctx));
  v.messages.scrollTop = v.messages.scrollHeight;
}

function hints(ctx) {
  const ask = (text) => h("button", { type: "button", class: "chip", onclick: () => {
    chat.view.input.value = text;
    chat.draft = text;
    send(ctx);
  } }, text);
  return h(
    "div",
    { class: "chat-hints" },
    h("p", { class: "muted" }, "Try asking:"),
    ask("What is this passage about?"),
    ask("Explain the key Hebrew or Greek words here."),
    ask("Where else does the Bible speak to this?"),
    ask("How have Christians interpreted this differently?"),
  );
}

function messageNode(ctx, m, index) {
  const node = h("div", { class: `msg ${m.role}`, "data-index": String(index) });
  fillMessage(ctx, node, m);
  return node;
}

function fillMessage(ctx, node, m) {
  if (m.role === "user") {
    replace(
      node,
      m.scopeLabel ? h("p", { class: "msg-scope" }, `With ${m.scopeLabel}`) : null,
      h("div", { class: "msg-body" }, m.content),
    );
    return;
  }
  const onReference = (ref) => ctx.goTo(ref.book, ref.chapter, ref.verse, { fromPanel: true });
  const body = h("div", { class: "msg-body" }, renderMarkdown(m.content, { findReferences: finder, onReference }));
  const thinking = m.reasoning
    ? h(
        "details",
        { class: "msg-reasoning", open: m.streaming && !m.content ? true : null },
        h("summary", {}, m.streaming && !m.content ? "Thinking…" : "Reasoning"),
        h("div", { class: "msg-reasoning-body" }, m.reasoning),
      )
    : null;
  const status =
    m.error ? h("p", { class: "chat-error" }, m.error)
    : m.reason === "length" ? h("p", { class: "chat-note" }, "The answer reached its length limit.")
    : m.reason === "refusal" ? h("p", { class: "chat-note" }, "The model declined to answer this.")
    : m.reason === "cancelled" ? h("p", { class: "chat-note" }, "Stopped.")
    : null;
  const waiting = m.streaming && !m.content && !m.reasoning ? h("p", { class: "chat-note typing" }, "Waiting for the model…") : null;
  const tools =
    !m.streaming && (m.content || m.error)
      ? h(
          "div",
          { class: "msg-tools" },
          m.content
            ? h("button", { type: "button", class: "text-button", onclick: () => copyAnswer(ctx, m) }, icon("copy"), "Copy")
            : null,
          m.content
            ? h("button", { type: "button", class: "text-button", onclick: () => report(ctx, m) }, icon("flag"), "Report")
            : null,
          m.usage?.inputTokens
            ? h("span", { class: "msg-usage" }, `${compact(m.usage.inputTokens)} in · ${compact(m.usage.outputTokens ?? 0)} out${m.usage.cachedTokens ? ` · ${compact(m.usage.cachedTokens)} cached` : ""}`)
            : null,
        )
      : null;
  replace(node, thinking, waiting, body, status, tools);
}

let drawQueued = false;
/** Redraw the answer being streamed, at most once per frame. */
function drawStreaming(ctx) {
  if (drawQueued) return;
  drawQueued = true;
  requestAnimationFrame(() => {
    drawQueued = false;
    const v = chat.view;
    if (!v) return;
    const index = chat.messages.length - 1;
    const node = v.messages.querySelector(`[data-index="${index}"]`);
    if (!node) return drawMessages(ctx);
    const nearBottom = v.messages.scrollHeight - v.messages.scrollTop - v.messages.clientHeight < 80;
    // Keep an open reasoning box open
    const wasOpen = node.querySelector(".msg-reasoning")?.open;
    fillMessage(ctx, node, chat.messages[index]);
    const details = node.querySelector(".msg-reasoning");
    if (details && wasOpen !== undefined) details.open = wasOpen;
    if (nearBottom) v.messages.scrollTop = v.messages.scrollHeight;
  });
}

function drawSend() {
  const v = chat.view;
  if (!v) return;
  const busy = !!chat.requestId;
  replace(v.sendButton, icon(busy ? "stop" : "send"));
  v.sendButton.setAttribute("aria-label", busy ? "Stop" : "Send");
  v.sendButton.title = busy ? "Stop" : "Send (Enter)";
  v.sendButton.classList.toggle("busy", busy);
}

function drawConsent(ctx) {
  const v = chat.view;
  if (!v) return;
  const p = provider(ctx);
  if (!chat.pendingConsent || !p) {
    v.consent.hidden = true;
    return;
  }
  let host = p.baseUrl;
  try {
    host = new URL(p.baseUrl).host;
  } catch {}
  v.consent.hidden = false;
  replace(
    v.consent,
    h("p", {}, h("strong", {}, `Send to ${p.name}?`)),
    h(
      "p",
      {},
      `Your question, this conversation, and the attached Scripture will be sent to ${host}. `,
      p.preset === "local"
        ? "That's your own server."
        : `${p.name}'s terms and privacy policy apply. KJV Interlinear doesn't see or keep any of it.`,
    ),
    h(
      "div",
      { class: "chat-consent-actions" },
      h("button", { type: "button", class: "button", onclick: () => {
        chat.pendingConsent = null;
        drawConsent(ctx);
      } }, "Cancel"),
      h("button", { type: "button", class: "button primary", onclick: () => {
        ctx.changeSettings((s) => { s.ai.consent[p.id] = true; });
        chat.pendingConsent = null;
        drawConsent(ctx);
        send(ctx);
      } }, "Allow and send"),
    ),
  );
}

// ------------------------------------------------------------------ sending

async function send(ctx) {
  const v = chat.view;
  const p = provider(ctx);
  const ai = ctx.settings.ai;
  if (!v || !p || chat.requestId) return;
  const text = v.input.value.trim();
  if (!text) return;
  if (!ai.model) await loadModels(ctx);
  if (!ai.model) {
    ctx.toast(chat.models.get(p.id)?.error ? "Couldn't reach the model: see the message above" : "Choose a model first");
    return;
  }
  if (!ai.consent[p.id]) {
    chat.pendingConsent = text;
    drawConsent(ctx);
    return;
  }
  await refreshSize(ctx);
  const limit = contextWindow(ctx);
  if (limit && promptTokens(ctx, text) + answerTokens(ctx) > limit) {
    chat.showScope = true;
    v.scopeEditor.hidden = false;
    drawBudget(ctx);
    ctx.toast("Too large for this model: attach less");
    return;
  }

  const scope = scopeArgs(ctx);
  const history = chat.messages
    .filter((m) => m.content && !m.error)
    .map((m) => ({ role: m.role, content: m.content }));
  history.push({ role: "user", content: text });
  chat.messages.push({ role: "user", content: text, scopeLabel: scope.kind === "none" ? null : chat.size?.label });
  const answer = { role: "assistant", content: "", reasoning: "", streaming: true };
  chat.messages.push(answer);
  v.input.value = "";
  chat.draft = "";
  autosize(v.input);

  const id = `r${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
  chat.requestId = id;
  const sentScopeTokens = chat.size?.tokens ?? 0;
  const estimate = sentScopeTokens + INSTRUCTION_TOKENS + history.reduce((n, m) => n + estimateTokens(m.content), 0);
  const key = calibrationKey(ctx);
  drawMessages(ctx);
  drawSend();

  const info = modelInfo(ctx);
  try {
    await aiChat(
      id,
      {
        providerId: p.id,
        kind: p.kind,
        baseUrl: p.baseUrl,
        model: ai.model,
        scope,
        contextOptions: { original: ai.original },
        messages: history,
        maxTokens: answerTokens(ctx),
        thinking: !!info?.adaptiveThinking,
      },
      (event) => {
        if (event.type === "text") answer.content += event.text;
        else if (event.type === "reasoning") answer.reasoning += event.text;
        else if (event.type === "usage") {
          answer.usage = event.usage;
          // Learn how this model's tokenizer compares with the estimate, when
          // the Scripture is most of the prompt
          const real = event.usage.inputTokens;
          if (real && sentScopeTokens > 2000) {
            const ratio = real / estimate;
            ctx.changeSettings((s) => {
              const old = s.ai.calibration[key];
              s.ai.calibration[key] = Math.round((old ? (old + ratio) / 2 : ratio) * 1000) / 1000;
            });
          }
        } else if (event.type === "done") answer.reason = event.reason;
        drawStreaming(ctx);
      },
    );
  } catch (error) {
    answer.error = String(error.message ?? error);
  } finally {
    answer.content = answer.content.replace(/^\s+/, "");
    answer.streaming = false;
    chat.requestId = null;
    drawMessages(ctx);
    drawSend();
    drawBudget(ctx);
  }
}

function stop() {
  if (chat.requestId) aiCancel(chat.requestId).catch(() => {});
}

async function copyAnswer(ctx, m) {
  try {
    await copyText(m.content);
    ctx.toast("Copied the answer");
  } catch {
    ctx.toast("Could not copy to the clipboard");
  }
}

/** Report a harmful or wrong answer: opens a prefilled report the reader can review. */
function report(ctx, m) {
  const p = provider(ctx);
  const question = [...chat.messages].slice(0, chat.messages.indexOf(m)).reverse().find((x) => x.role === "user")?.content ?? "";
  const clip = (s, n) => (s.length > n ? `${s.slice(0, n)}…` : s);
  const body = [
    "**What's wrong with this answer?**",
    "",
    "",
    `**Model:** ${p?.name ?? "?"} / ${ctx.settings.ai.model ?? "?"}`,
    `**Question:** ${clip(question, 600)}`,
    "",
    "**Answer:**",
    "",
    clip(m.content, 2500).replace(/^/gm, "> "),
  ].join("\n");
  const url = `https://github.com/Divhanthelion/KJV-Interlinear/issues/new?labels=ai-report&title=${encodeURIComponent("AI answer report")}&body=${encodeURIComponent(body)}`;
  openExternal(url);
}

// ------------------------------------------------------------------ settings: providers

const editing = { form: null }; // { id | null, preset, name, baseUrl, key, contextWindow, status }

export function renderAiSettings(ctx) {
  const ai = ctx.settings.ai;
  const rows = ai.providers.map((p) =>
    h(
      "li",
      { class: "provider-row" },
      h(
        "span",
        { class: "grow" },
        h("span", { class: "row-main" }, p.name),
        h("span", { class: "row-sub", "data-key-status": p.id }, p.baseUrl),
      ),
      h("button", { type: "button", class: "text-button", onclick: () => {
        editing.form = { id: p.id, preset: p.preset, name: p.name, baseUrl: p.baseUrl, key: "", contextWindow: p.contextWindow ?? "" };
        ctx.refreshPanel();
      } }, "Edit"),
    ),
  );
  const list = rows.length ? h("ul", { class: "result-list" }, rows) : null;
  // Fill in key status once the page is drawn
  queueMicrotask(() => {
    for (const p of ai.providers) {
      aiKeyStatus(p.id).then((st) => {
        const el = document.querySelector(`[data-key-status="${p.id}"]`);
        if (!el) return;
        const key = st.stored ? (st.storage === "file" ? "key saved in the app's files" : "key in system keychain") : presetOf(p).keyOptional ? "no key" : "no key yet";
        el.textContent = `${p.baseUrl} · ${key}`;
      }).catch(() => {});
    }
  });
  return [
    h("p", { class: "setting-note" }, "Chat about the text with your own AI: a server on your network or an API key. Keys are kept in your system's keychain and never leave this device except to the service they belong to."),
    list,
    editing.form ? providerForm(ctx) : h("button", { type: "button", class: "button", onclick: () => {
      editing.form = { id: null, preset: "local", name: "", baseUrl: "", key: "", contextWindow: "" };
      ctx.refreshPanel();
    } }, icon("plus"), "Add a provider"),
  ];
}

function providerForm(ctx) {
  const f = editing.form;
  const preset = PRESETS.find((p) => p.id === f.preset) ?? PRESETS[0];
  const fixedUrl = !!preset.baseUrl;
  const status = h("p", { class: "setting-note", "aria-live": "polite" }, f.status ?? "");
  const presetSelect = h(
    "select",
    { id: "provider-preset", "aria-label": "Service" },
    PRESETS.map((p) => h("option", { value: p.id, selected: p.id === f.preset ? true : null }, p.label)),
  );
  presetSelect.addEventListener("change", () => {
    f.preset = presetSelect.value;
    const next = PRESETS.find((p) => p.id === f.preset);
    f.baseUrl = next.baseUrl;
    f.status = "";
    ctx.refreshPanel();
  });
  const name = h("input", { id: "provider-name", type: "text", value: f.name, placeholder: preset.label, autocomplete: "off" });
  name.addEventListener("input", () => { f.name = name.value; });
  const url = h("input", {
    id: "provider-url",
    type: "url",
    value: fixedUrl ? preset.baseUrl : f.baseUrl,
    placeholder: preset.placeholder ?? "",
    readonly: fixedUrl ? true : null,
    autocomplete: "off",
    autocapitalize: "off",
    spellcheck: "false",
  });
  url.addEventListener("input", () => { f.baseUrl = url.value; });
  const key = h("input", {
    id: "provider-key",
    type: "password",
    value: f.key,
    placeholder: f.id ? "Saved key kept unless you enter a new one" : preset.keyOptional ? "Optional" : "Paste your API key",
    autocomplete: "off",
    autocapitalize: "off",
    spellcheck: "false",
  });
  key.addEventListener("input", () => { f.key = key.value; });
  const size = h("input", { id: "provider-context", type: "number", min: "1000", step: "1000", value: f.contextWindow ?? "", placeholder: "As reported by the service", inputmode: "numeric" });
  size.addEventListener("input", () => { f.contextWindow = size.value; });

  const save = async () => {
    const baseUrl = (fixedUrl ? preset.baseUrl : f.baseUrl).trim().replace(/\/+$/, "");
    if (!/^https?:\/\/[^\s/]+/.test(baseUrl)) {
      f.status = "Enter the server's address, starting with http:// or https://";
      return ctx.refreshPanel();
    }
    if (!f.id && !preset.keyOptional && !f.key.trim()) {
      f.status = "Paste an API key for this service.";
      return ctx.refreshPanel();
    }
    const id = f.id ?? `p${Date.now().toString(36)}`;
    const contextWindow = Number.parseInt(f.contextWindow, 10);
    try {
      if (f.key.trim()) {
        const where = await aiKeySet(id, f.key);
        if (where === "file") ctx.toast("No system keychain found: the key is saved in the app's private files");
      }
    } catch (error) {
      f.status = String(error.message ?? error);
      return ctx.refreshPanel();
    }
    ctx.changeSettings((s) => {
      const entry = {
        id,
        preset: preset.id,
        name: f.name.trim() || preset.label,
        kind: preset.kind,
        baseUrl,
        contextWindow: Number.isInteger(contextWindow) && contextWindow > 0 ? contextWindow : null,
      };
      const i = s.ai.providers.findIndex((p) => p.id === id);
      if (i >= 0) {
        // A new address or service needs the reader's permission again
        if (s.ai.providers[i].baseUrl !== baseUrl) delete s.ai.consent[id];
        s.ai.providers[i] = entry;
      } else {
        s.ai.providers.push(entry);
      }
      if (!s.ai.providerId || i < 0) {
        s.ai.providerId = id;
        s.ai.model = null;
      }
    });
    chat.models.delete(id);
    editing.form = null;
    ctx.refreshPanel();
    ctx.toast("Provider saved");
  };

  const test = async () => {
    f.status = "Connecting…";
    status.textContent = f.status;
    const baseUrl = (fixedUrl ? preset.baseUrl : f.baseUrl).trim().replace(/\/+$/, "");
    // A newly typed key is tried under a throwaway id; otherwise the saved one
    const typed = !!f.key.trim();
    const id = typed || !f.id ? "__test__" : f.id;
    try {
      if (typed) await aiKeySet(id, f.key);
      const list = await aiModels({ providerId: id, kind: preset.kind, baseUrl });
      const sizes = list.map((m) => m.contextWindow).filter(Boolean);
      f.status = `Connected. ${list.length} model${list.length === 1 ? "" : "s"}${sizes.length ? `, up to ${compactLimit(Math.max(...sizes))} tokens of context` : ""}.`;
    } catch (error) {
      f.status = String(error.message ?? error);
    } finally {
      if (typed) aiKeySet("__test__", "").catch(() => {});
    }
    status.textContent = f.status;
  };

  const remove = () => {
    const id = f.id;
    aiKeySet(id, "").catch(() => {});
    ctx.changeSettings((s) => {
      s.ai.providers = s.ai.providers.filter((p) => p.id !== id);
      delete s.ai.consent[id];
      for (const k of Object.keys(s.ai.calibration)) if (k.startsWith(`${id}|`)) delete s.ai.calibration[k];
      if (s.ai.providerId === id) {
        s.ai.providerId = s.ai.providers[0]?.id ?? null;
        s.ai.model = null;
      }
    });
    chat.models.delete(id);
    editing.form = null;
    ctx.refreshPanel();
  };

  const field = (label, control, hint) =>
    h("label", { class: "field" }, h("span", { class: "field-label" }, label), control, hint ? h("span", { class: "setting-hint" }, hint) : null);

  return h(
    "div",
    { class: "provider-form" },
    field("Service", presetSelect),
    field("Name", name),
    field("Address", url, preset.hint),
    field("API key", key),
    field("Context window (tokens)", size, "Only if the service doesn't report it."),
    status,
    h(
      "div",
      { class: "form-actions" },
      f.id ? h("button", { type: "button", class: "button danger", onclick: remove }, "Remove") : null,
      h("span", { class: "grow" }),
      h("button", { type: "button", class: "button", onclick: test }, "Test"),
      h("button", { type: "button", class: "button", onclick: () => { editing.form = null; ctx.refreshPanel(); } }, "Cancel"),
      h("button", { type: "button", class: "button primary", onclick: save }, "Save"),
    ),
  );
}
