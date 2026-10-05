// The study assistant: chat with a language model about a verse, a chapter, some
// books, or the whole Bible, using the reader's own AI provider (a local server or
// their API key). Also the provider setup shown in Settings.

import {
  aiCancel,
  aiChat,
  aiKeySet,
  aiKeyStatus,
  aiModels,
  call,
  conversationDelete,
  conversationLoad,
  conversationSave,
  conversationsList,
  copyText,
  openExternal,
} from "./backend.js";
import { h, icon, replace, timeAgo } from "./dom.js";
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

/** "https://api.openai.com" for "https://api.openai.com/v1"; null if it isn't a URL. */
function originOf(url) {
  try {
    return new URL(url).origin;
  } catch {
    return null;
  }
}

/** Longest answer to ask for (see answerTokens for the room kept free for it). */
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
  // The open conversation as saved: { id, title, created, starred }; null until the first question
  current: null,
  showHistory: false,
};

let finder = null;
let finderBooks = null; // the book list `finder` was built from

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

/** The size request in flight, if any: { key, done }. */
let sizing = null;

/**
 * Measure the attached scope into chat.size. Only the latest scope asked for is kept
 * (a slow answer for an older one is dropped), and the promise settles once chat.size
 * matches it.
 */
function refreshSize(ctx) {
  const scope = scopeArgs(ctx);
  const original = ctx.settings.ai.original;
  const key = JSON.stringify([scope, original]);
  if (sizing?.key === key) return sizing.done;
  if (chat.size?.key === key) {
    sizing = null; // a request for another scope still on its way mustn't land over this
    return Promise.resolve();
  }
  const request = { key };
  sizing = request;
  request.done = (async () => {
    let size = { label: "", tokens: 0, verses: 0 };
    if (scope.kind !== "none") {
      try {
        size = await call("context_size", { scope, options: { original } });
      } catch (error) {
        size = { ...size, error: String(error.message ?? error) };
      }
    }
    if (sizing !== request) return sizing?.done; // the reader moved on: wait for the newer one
    sizing = null;
    chat.size = { key, ...size };
    drawBudget(ctx);
  })();
  return request.done;
}

/** Tokens the next request will use before the answer, corrected for this model. */
function promptTokens(ctx, extraText = "") {
  const factor = ctx.settings.ai.calibration[calibrationKey(ctx)] ?? 1;
  const history = chat.messages.reduce((n, m) => n + estimateTokens(m.content ?? ""), 0);
  return Math.ceil(((chat.size?.tokens ?? 0) + INSTRUCTION_TOKENS + history + estimateTokens(extraText)) * factor);
}

/**
 * Room kept free for the answer, and the limit sent with the question: the model's own
 * maximum (at most ANSWER_TOKENS), but no more than a quarter of a known context
 * window, so a small (8k) model still has room for the question.
 */
function answerTokens(ctx) {
  const max = Math.min(modelInfo(ctx)?.maxOutput ?? ANSWER_TOKENS, ANSWER_TOKENS);
  const limit = contextWindow(ctx);
  return limit ? Math.min(max, Math.max(512, Math.floor(limit / 4))) : max;
}

/** 1,085,845 -> "1.09M", 13,939 -> "14k", 999,600 -> "1M". Limits round down (never overstate the room). */
const compact = (n, round = Math.round) =>
  n >= 1e6 || round(n / 1e3) >= 1000
    ? `${(round(n / 1e4) / 100).toFixed(2).replace(/\.?0+$/, "")}M`
    : n >= 1e3 ? `${round(n / 1e3)}k` : String(n);
const compactLimit = (n) => compact(n, Math.floor);

// ------------------------------------------------------------------ chat panel

export function renderChat(body, ctx) {
  // Rebuilt if the panel was opened before the books (with their chapter counts) arrived
  if (finderBooks !== ctx.state.books) {
    finder = referenceFinder(ctx.state.books);
    finderBooks = ctx.state.books;
  }
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
        h("button", { type: "button", class: "button primary", onclick: () => openProviderForm(ctx) }, "Set up an AI provider"),
      ),
    );
    return null;
  }

  // A redraw (say, the model list arriving) mustn't move the reader
  const previous = chat.view?.messages.isConnected
    ? { following: chat.view.follow.following, top: chat.view.messages.scrollTop }
    : null;

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
  const jump = h("button", { type: "button", class: "chat-jump", hidden: true }, icon("arrowDown"), "Latest");
  const budget = h("div", { class: "chat-budget" });
  const scopeEditor = h("div", { class: "chat-scope", hidden: !chat.showScope });
  const consent = h("div", { class: "chat-consent", hidden: true });

  chat.view = { body, input, sendButton, messages, jump, budget, scopeEditor, consent };
  chat.view.follow = follower(messages, drawJump);
  jump.addEventListener("click", () => chat.view.follow.resume());

  const history = h("div", { class: "chat-history" });
  chat.view.history = history;
  // Redrawn on its own when a model list arrives (see drawModelRow)
  chat.view.modelRow = modelPicker(ctx);

  replace(
    body,
    h(
      "div",
      { class: "chat", "data-mode": chat.showHistory ? "history" : "chat" },
      h("div", { class: "chat-top" }, chat.view.modelRow, budget, scopeEditor),
      h("div", { class: "chat-scroll" }, messages, jump),
      history,
      consent,
      h("div", { class: "chat-compose" }, input, sendButton),
    ),
  );
  if (chat.showHistory) drawHistory(ctx);
  drawScopeEditor(ctx);
  if (previous && !previous.following) chat.view.follow.following = false;
  drawMessages(ctx);
  if (previous && !previous.following) messages.scrollTop = previous.top;
  drawSend();
  drawConsent(ctx);
  refreshSize(ctx);
  // Every provider's models, so any of them can be picked from the menu (each is
  // asked once: a failed list waits for Retry)
  for (const p of ai.providers) loadModels(ctx, { p });
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
  select.append(h("option", { value: "__add__" }, "Add a provider…"));
  select.value = `${ai.providerId}\n${ai.model ?? ""}`;
  select.addEventListener("change", () => {
    if (select.value === "__add__") {
      select.value = `${ctx.settings.ai.providerId}\n${ctx.settings.ai.model ?? ""}`;
      openProviderForm(ctx);
      return;
    }
    const [providerId, model] = select.value.split("\n");
    ctx.changeSettings((s) => {
      s.ai.providerId = providerId;
      s.ai.model = model || null;
    });
    loadModels(ctx);
    // Redraw: the controls depend on the provider ("Think first" is for your own server)
    ctx.refreshPanel();
  });
  const status = chat.models.get(ai.providerId);
  // Local reasoning models (Qwen, DeepSeek-R1, …) can skip thinking for quick questions
  const think =
    provider(ctx)?.preset === "local"
      ? h(
          "button",
          {
            type: "button",
            class: "chip think-toggle",
            "aria-pressed": String(ai.think),
            title: "Let the model reason before it answers (slower, often better on hard questions)",
            onclick: (event) => {
              ctx.changeSettings((s) => { s.ai.think = !s.ai.think; });
              event.currentTarget.setAttribute("aria-pressed", String(ctx.settings.ai.think));
            },
          },
          "Think first",
        )
      : null;
  return h(
    "div",
    { class: "chat-model-row" },
    select,
    think,
    h(
      "button",
      {
        type: "button",
        class: "icon-btn",
        "aria-label": "New conversation",
        title: "New conversation",
        "data-new-conversation": "",
        onclick: () => {
          if (chat.requestId) return; // disabled while an answer streams (see drawSend)
          chat.messages = [];
          chat.current = null;
          chat.showHistory = false;
          ctx.refreshPanel();
          chat.view?.input.focus();
        },
      },
      icon("plus"),
    ),
    h(
      "button",
      {
        type: "button",
        class: "icon-btn",
        "aria-label": "Conversations",
        title: "Conversations",
        "aria-pressed": String(chat.showHistory),
        onclick: () => {
          chat.showHistory = !chat.showHistory;
          ctx.refreshPanel();
        },
      },
      icon("history"),
    ),
    status?.error
      ? h(
          "p",
          { class: "chat-error small" },
          status.error,
          " ",
          h("button", { type: "button", class: "text-button", onclick: () => retryModels(ctx) }, "Retry"),
        )
      : null,
  );
}

/** Retry: ask again every provider whose list failed, and show them loading. */
function retryModels(ctx) {
  for (const p of ctx.settings.ai.providers) {
    if (chat.models.get(p.id)?.error) loadModels(ctx, { p, force: true });
  }
  ctx.refreshPanel();
}

/**
 * Load a provider's models (the current one unless `p` is given). Each provider is
 * asked once: a list, or an error, stays until `force` (Retry) or the provider changes.
 */
function loadModels(ctx, { force = false, p = provider(ctx) } = {}) {
  if (!p) return Promise.resolve();
  const existing = chat.models.get(p.id);
  if (existing?.loading) return existing.loading;
  if (existing && !force) return Promise.resolve();
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
    drawModelRow(ctx);
  })();
  chat.models.set(p.id, { loading });
  return loading;
}

/** Redraw just the model menu and what depends on it (the size meter, the buttons). */
function drawModelRow(ctx) {
  const v = chat.view;
  if (!v?.modelRow?.isConnected) return;
  const controls = (el) => [...el.querySelectorAll("select, button")];
  const focused = controls(v.modelRow).indexOf(document.activeElement);
  const row = modelPicker(ctx);
  v.modelRow.replaceWith(row);
  v.modelRow = row;
  if (focused >= 0) controls(row)[focused]?.focus();
  drawBudget(ctx); // the model's context window and answer limit may be known now
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

// ------------------------------------------------------------------ scrolling

/**
 * Follow new content only while the reader is at the bottom of `el`. Scrolling away
 * by any means (wheel, touch, scrollbar, keys) lets go at once; coming back to the
 * bottom picks it up again. Content that streams in never moves the view otherwise.
 */
function follower(el, onChange = () => {}) {
  const f = { following: true };
  const gap = () => el.scrollHeight - el.scrollTop - el.clientHeight;
  // Our own jumps to the bottom also fire scroll events; they land at the bottom,
  // so they keep following on. Growth between frames doesn't fire scroll events.
  el.addEventListener(
    "scroll",
    () => {
      const atBottom = gap() <= 2;
      if (atBottom !== f.following) {
        f.following = atBottom;
        onChange();
      }
    },
    { passive: true },
  );
  f.stick = () => {
    if (f.following && gap() > 0) el.scrollTop = el.scrollHeight;
    onChange();
  };
  f.resume = () => {
    f.following = true;
    el.scrollTop = el.scrollHeight;
    onChange();
  };
  f.below = () => gap() > 40;
  return f;
}

function drawJump() {
  const v = chat.view;
  if (!v) return;
  v.jump.hidden = v.follow.following || !v.follow.below();
}

// ------------------------------------------------------------------ messages

/** Draw every message. Keeps the reader's place unless they were at the bottom. */
function drawMessages(ctx) {
  const v = chat.view;
  if (!v) return;
  live.node = null;
  replace(v.messages, chat.messages.length ? chat.messages.map((m, i) => messageNode(ctx, m, i)) : hints(ctx));
  v.follow.stick();
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

const words = (text) => (text.match(/\S+/g) ?? []).length;

/**
 * True when the latest stretch of reasoning is mostly phrases already used: a model
 * stuck in a loop. Healthy reasoning that redrafts its answer stays well under this
 * (a 4,600-word Qwen trace peaked at 10%); a real loop sits near 100%.
 */
function repeating(text) {
  const w = text.split(/\s+/).filter(Boolean);
  if (w.length < 600) return false;
  const n = 8;
  const tail = 300;
  const earlier = new Set();
  for (let i = 0; i + n <= w.length - tail; i++) earlier.add(w.slice(i, i + n).join(" "));
  let seen = 0;
  let total = 0;
  for (let i = w.length - tail; i + n <= w.length; i++, total++) if (earlier.has(w.slice(i, i + n).join(" "))) seen++;
  return seen / total > 0.6;
}
const reasoningSummary = (m) =>
  `${m.streaming && !m.content ? "Thinking…" : "Reasoning"} · ${words(m.reasoning).toLocaleString()} words`;

function fillMessage(ctx, node, m) {
  if (m.role === "user") {
    replace(
      node,
      m.scopeLabel ? h("p", { class: "msg-scope" }, `With ${m.scopeLabel}`) : null,
      h("div", { class: "msg-body" }, m.content),
    );
    return;
  }
  const body = h("div", { class: "msg-body" }, renderMarkdown(m.content, markdownOptions(ctx)));
  const reasoningBody = h("div", { class: "msg-reasoning-body", tabindex: "0" }, m.reasoning);
  const expand = h("button", { type: "button", class: "text-button reasoning-expand" }, "Show all");
  const thinking = h(
    "details",
    { class: "msg-reasoning", hidden: !m.reasoning, open: m.streaming && !m.content ? true : null },
    h("summary", {}, m.reasoning ? reasoningSummary(m) : ""),
    reasoningBody,
    expand,
  );
  expand.addEventListener("click", () => {
    const all = thinking.classList.toggle("expanded");
    expand.textContent = all ? "Show less" : "Show all";
  });
  const status =
    m.error ? h("p", { class: "chat-error" }, m.error)
    : m.reason === "length" && !m.content
      ? h("p", { class: "chat-note" }, "The model used its whole length limit thinking and didn't reach an answer. Try again with “Think first” off, or ask a narrower question.")
    : m.reason === "length" ? h("p", { class: "chat-note" }, "The answer reached its length limit.")
    // The stream ended without the service saying the answer was finished
    : m.reason === "incomplete" ? h("p", { class: "chat-note" }, "The answer may be incomplete.")
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

function markdownOptions(ctx) {
  return { findReferences: finder, onReference: (ref) => ctx.goTo(ref.book, ref.chapter, ref.verse, { fromPanel: true }) };
}

/**
 * The answer being streamed, updated in place: reasoning text is appended, and of
 * the answer only the blocks that changed (normally just the last) are replaced.
 * Nothing the reader is looking at, scrolling, or selecting gets rebuilt.
 */
const live = { node: null };

function attachLive(node, m) {
  const thinking = node.querySelector(".msg-reasoning");
  const reasoningBody = thinking.querySelector(".msg-reasoning-body");
  if (!reasoningBody.firstChild) reasoningBody.append(document.createTextNode(""));
  // A new answer starts afresh; a redrawn panel picks the same answer up where it was
  if (live.message !== m) Object.assign(live, { message: m, answering: false, loopCheckedAt: 0 });
  Object.assign(live, {
    node,
    thinking,
    summary: thinking.querySelector("summary"),
    reasoningBody,
    reasoningText: reasoningBody.firstChild,
    reasoningLength: m.reasoning.length,
    reasoningFollow: follower(reasoningBody),
    // Once the reader opens, closes, or scrolls the reasoning, it's theirs to manage
    touched: false,
    body: node.querySelector(".msg-body"),
  });
  const touch = () => { live.touched = true; };
  thinking.querySelector("summary").addEventListener("click", touch);
  reasoningBody.addEventListener("wheel", touch, { passive: true });
  reasoningBody.addEventListener("touchstart", touch, { passive: true });
  reasoningBody.addEventListener("keydown", touch);
}

let drawQueued = false;
/** Update `m`, the answer being streamed, at most once per frame. */
function drawStreaming(ctx, m) {
  if (drawQueued) return;
  drawQueued = true;
  requestAnimationFrame(() => {
    drawQueued = false;
    const v = chat.view;
    if (!v) return;
    // Found by identity: if its conversation is no longer the one shown, draw nothing
    const index = chat.messages.indexOf(m);
    if (index < 0) return;
    const node = v.messages.querySelector(`[data-index="${index}"]`);
    if (!node) return drawMessages(ctx);
    if (live.node !== node) attachLive(node, m);

    if (m.reasoning.length > live.reasoningLength) {
      live.thinking.hidden = false;
      node.querySelector(".typing")?.remove();
      live.reasoningText.appendData(m.reasoning.slice(live.reasoningLength));
      live.reasoningLength = m.reasoning.length;
      live.reasoningFollow.stick();
      if (!m.content && m.reasoning.length - (live.loopCheckedAt ?? 0) > 4000) {
        live.loopCheckedAt = m.reasoning.length;
        const looping = repeating(m.reasoning);
        let note = live.node.querySelector(".loop-note");
        if (looping && !note) {
          note = h("p", { class: "chat-note loop-note" }, "The model seems to be repeating itself. You can stop it and ask again, perhaps with “Think first” off.");
          live.thinking.after(note);
        } else if (!looping) note?.remove();
      }
    }
    if (m.reasoning) live.summary.textContent = reasoningSummary(m);
    if (m.content) {
      node.querySelector(".typing")?.remove();
      // Fold the reasoning away when the answer starts, unless the reader is in it
      if (live.thinking.open && !live.touched && !live.answering) live.thinking.open = false;
      live.answering = true;
      patchChildren(live.body, renderMarkdown(m.content, markdownOptions(ctx)));
    }
    v.follow.stick();
  });
}

/** Make `target`'s children match `fresh`, replacing only from the first difference. */
function patchChildren(target, fresh) {
  const next = [...fresh.childNodes];
  const old = [...target.childNodes];
  let same = 0;
  while (same < old.length && same < next.length && old[same].isEqualNode(next[same])) same++;
  for (const node of old.slice(same)) node.remove();
  target.append(...next.slice(same));
}

/** Answer `m` ended: add its status and tools without touching what's already shown. */
function finishLive(ctx, m) {
  const v = chat.view;
  if (!v) return;
  const index = chat.messages.indexOf(m);
  if (index < 0) {
    // Its conversation was closed or deleted meanwhile: nothing of it is on screen
    live.node = null;
    return;
  }
  const node = v.messages.querySelector(`[data-index="${index}"]`);
  if (!node) return drawMessages(ctx);
  if (live.node !== node) {
    // Nothing streamed into it (an early error): there's nothing to preserve
    fillMessage(ctx, node, m);
  } else {
    node.querySelector(".typing")?.remove();
    patchChildren(live.body, renderMarkdown(m.content, markdownOptions(ctx)));
    if (m.reasoning) live.summary.textContent = reasoningSummary(m);
    const done = messageNode(ctx, m, index);
    for (const part of done.querySelectorAll(":scope > .chat-error, :scope > .chat-note, :scope > .msg-tools")) node.append(part);
  }
  live.node = null;
  v.follow.stick();
}

/** Shown on what can't be used while an answer streams into the open conversation. */
const BUSY_TITLE = "Wait for the answer to finish, or stop it";

function drawSend() {
  const v = chat.view;
  if (!v) return;
  const busy = !!chat.requestId;
  // Kept current here: it's drawn once, but the conversation changes under it
  const fresh = v.body.querySelector("[data-new-conversation]");
  if (fresh) {
    fresh.disabled = busy || (!chat.messages.length && !chat.showHistory);
    fresh.title = busy ? BUSY_TITLE : "New conversation";
  }
  // Another conversation can't be opened until the answer has ended (and been saved)
  for (const b of v.history.querySelectorAll("[data-open-conversation]")) {
    b.disabled = busy;
    if (busy) b.title = BUSY_TITLE;
    else b.removeAttribute("title");
  }
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
  if (chat.preparing) return;
  chat.preparing = true;
  try {
    await sendNow(ctx);
  } finally {
    chat.preparing = false;
  }
}

async function sendNow(ctx) {
  const p = provider(ctx);
  const ai = ctx.settings.ai;
  if (!chat.view || !p || chat.requestId) return;
  const text = chat.view.input.value.trim();
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
  // The panel may have been redrawn while waiting: use what's on screen now
  const v = chat.view;
  if (!v) return;
  const limit = contextWindow(ctx);
  if (limit && promptTokens(ctx, text) + answerTokens(ctx) > limit) {
    chat.showScope = true;
    v.scopeEditor.hidden = false;
    drawBudget(ctx);
    ctx.toast("Too large for this model: attach less");
    return;
  }

  if (!chat.current) {
    const now = Date.now();
    chat.current = { id: `c${now.toString(36)}${Math.random().toString(36).slice(2, 8)}`, title: titleFor(text), created: now, starred: false };
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
  // Asking a question means wanting to see the answer
  chat.view?.follow.resume();
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
        enableThinking: p.preset === "local" ? ai.think : null,
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
        else if (event.type === "textWasReasoning") {
          // A late </think> revealed that what streamed so far was reasoning
          answer.reasoning += answer.content;
          answer.content = "";
        }
        drawStreaming(ctx, answer);
      },
    );
  } catch (error) {
    answer.error = String(error.message ?? error);
  } finally {
    answer.content = answer.content.replace(/^\s+/, "");
    answer.streaming = false;
    chat.requestId = null;
    finishLive(ctx, answer);
    drawSend();
    saveCurrent(ctx);
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

/** Longest report link to open: browsers and GitHub refuse much longer ones. */
const REPORT_URL_LIMIT = 6000;

/** Report a harmful or wrong answer: opens a prefilled report the reader can review. */
function report(ctx, m) {
  const p = provider(ctx);
  const asked = [...chat.messages].slice(0, chat.messages.indexOf(m)).reverse().find((x) => x.role === "user")?.content ?? "";
  // Counted in code points, so a cut never splits a character (encodeURIComponent
  // throws on half a surrogate pair)
  const question = [...asked];
  const answer = [...m.content];
  const cut = (chars, n) => (n < chars.length ? `${chars.slice(0, n).join("")}…` : chars.join(""));
  const trimmed = (chars, n) => (n < chars.length ? " (trimmed)" : "");
  const url = (q, a) =>
    `https://github.com/Divhanthelion/KJV-Interlinear/issues/new?labels=ai-report&title=${encodeURIComponent("AI answer report")}&body=${encodeURIComponent(
      [
        "**What's wrong with this answer?**",
        "",
        "",
        `**Model:** ${p?.name ?? "?"} / ${ctx.settings.ai.model ?? "?"}`,
        `**Question${trimmed(question, q)}:** ${cut(question, q)}`,
        "",
        `**Answer${trimmed(answer, a)}:**`,
        "",
        cut(answer, a).replace(/^/gm, "> "),
      ].join("\n"),
    )}`;
  const fits = (q, a) => url(q, a).length <= REPORT_URL_LIMIT;
  // The most characters (up to `max`) for which ok(n) holds; 0 if none. Below the full
  // length every extra character lengthens the link, so a binary search finds it.
  const most = (max, ok) => {
    let lo = 0;
    let hi = max;
    while (lo < hi) {
      const mid = Math.ceil((lo + hi) / 2);
      if (ok(mid)) lo = mid;
      else hi = mid - 1;
    }
    return lo;
  };
  // Trim the answer first, then the question, by the length of the encoded link
  let q = Math.min(question.length, 600);
  let a = answer.length;
  if (!fits(q, a)) a = most(a, (n) => fits(q, n));
  if (!fits(q, a)) q = most(q, (n) => fits(n, a));
  openExternal(url(q, a));
}

// ------------------------------------------------------------------ saved conversations

/** "Why did Jesus weep in verse 35, when he already knew…" */
function titleFor(question) {
  const t = question.replace(/\s+/g, " ").trim();
  return t.length > 80 ? `${t.slice(0, 77).trimEnd()}…` : t;
}

/** Save the open conversation (after each answer). Failures are shown, never fatal. */
async function saveCurrent(ctx) {
  const c = chat.current;
  if (!c || !chat.messages.length) return;
  const p = provider(ctx);
  const conversation = {
    id: c.id,
    title: c.title,
    created: c.created,
    updated: Date.now(),
    starred: c.starred,
    provider: p?.name ?? null,
    model: ctx.settings.ai.model,
    messages: chat.messages.map(({ streaming, ...m }) => m),
  };
  try {
    await conversationSave(conversation);
  } catch (error) {
    ctx.toast(`Couldn't save this conversation: ${error.message ?? error}`);
  }
}

/** Open a saved conversation to read or continue. */
async function openConversation(ctx, id) {
  if (chat.requestId) return; // the streaming answer belongs to the open one (see drawSend)
  try {
    const c = await conversationLoad(id);
    chat.messages = (c.messages ?? []).map((m) => ({ ...m, streaming: false }));
    chat.current = { id: c.id, title: c.title, created: c.created, starred: !!c.starred };
    chat.showHistory = false;
    ctx.refreshPanel();
    chat.view?.follow.resume();
  } catch (error) {
    ctx.toast(String(error.message ?? error));
  }
}

/** Change a saved conversation's title or star without opening it. */
async function updateSaved(ctx, id, change) {
  const c = await conversationLoad(id);
  change(c);
  await conversationSave(c);
  if (chat.current?.id === id) Object.assign(chat.current, { title: c.title, starred: !!c.starred });
}

async function drawHistory(ctx) {
  const v = chat.view;
  if (!v) return;
  let list;
  try {
    list = await conversationsList();
  } catch (error) {
    replace(v.history, h("p", { class: "chat-error" }, `Couldn't read saved conversations: ${error.message ?? error}`));
    return;
  }
  if (chat.view !== v) return; // redrawn meanwhile
  if (!list.length) {
    replace(v.history, h("p", { class: "empty" }, "No conversations yet. Each conversation is saved here on this device as you go."));
    return;
  }
  const starred = list.filter((c) => c.starred);
  const recent = list.filter((c) => !c.starred);
  const section = (title, items) =>
    items.length ? [h("h3", { class: "section-title" }, title), h("ul", { class: "result-list conversation-list" }, items.map((c) => row(ctx, c)))] : null;
  replace(
    v.history,
    section("Saved", starred),
    section("Recent", recent),
    recent.length ? clearButton(ctx, recent) : null,
  );
}

function row(ctx, c) {
  const open = chat.current?.id === c.id;
  const sub = [timeAgo(c.updated ?? c.created ?? Date.now()), c.model, `${c.questions} question${c.questions === 1 ? "" : "s"}`]
    .filter(Boolean)
    .join(" · ");
  const li = h("li", { class: `conversation-row${open ? " is-open" : ""}` });
  const showRow = () =>
    replace(
      li,
      h(
        "button",
        {
          type: "button",
          class: "row-button",
          "data-open-conversation": "",
          disabled: chat.requestId ? true : null,
          title: chat.requestId ? BUSY_TITLE : null,
          onclick: () => openConversation(ctx, c.id),
        },
        h("span", { class: "grow" }, h("span", { class: "row-main" }, c.title), h("span", { class: "row-sub" }, sub)),
      ),
      h(
        "button",
        {
          type: "button",
          class: "icon-btn",
          "aria-label": c.starred ? `Unsave “${c.title}”` : `Save “${c.title}”`,
          title: c.starred ? "Saved: click to unsave" : "Save (keeps it when you clear history)",
          "aria-pressed": String(!!c.starred),
          onclick: async () => {
            await updateSaved(ctx, c.id, (x) => { x.starred = !x.starred; });
            drawHistory(ctx);
          },
        },
        icon(c.starred ? "starFilled" : "star"),
      ),
      h("button", { type: "button", class: "icon-btn", "aria-label": `Rename “${c.title}”`, title: "Rename", onclick: showRename }, icon("pencil")),
      h("button", { type: "button", class: "icon-btn", "aria-label": `Delete “${c.title}”`, title: "Delete", onclick: showDelete }, icon("trash")),
    );
  const showRename = () => {
    const input = h("input", { type: "text", value: c.title, "aria-label": "Conversation name", maxlength: "120" });
    const save = async () => {
      const title = input.value.trim();
      if (title && title !== c.title) {
        await updateSaved(ctx, c.id, (x) => { x.title = title; });
        drawHistory(ctx);
      } else showRow();
    };
    input.addEventListener("keydown", (e) => {
      if (e.key === "Enter") save();
      if (e.key === "Escape") { e.stopPropagation(); showRow(); }
    });
    replace(
      li,
      h("div", { class: "conversation-edit" }, input,
        h("button", { type: "button", class: "button primary", onclick: save }, "Save"),
        h("button", { type: "button", class: "button", onclick: showRow }, "Cancel")),
    );
    input.focus();
    input.select();
  };
  const showDelete = () =>
    replace(
      li,
      h("div", { class: "conversation-edit" },
        h("span", { class: "grow" }, `Delete “${c.title}”?`),
        h("button", { type: "button", class: "button danger", onclick: async () => {
          await conversationDelete(c.id);
          if (chat.current?.id === c.id) {
            chat.current = null;
            chat.messages = [];
          }
          drawHistory(ctx);
        } }, "Delete"),
        h("button", { type: "button", class: "button", onclick: showRow }, "Cancel")),
    );
  showRow();
  return li;
}

/** "Clear history": deletes the unsaved conversations, after a confirmation. */
function clearButton(ctx, recent) {
  const wrap = h("div", { class: "conversation-clear" });
  const ask = () =>
    replace(
      wrap,
      h("button", { type: "button", class: "text-button", onclick: confirm }, "Clear history"),
    );
  const confirm = () =>
    replace(
      wrap,
      h("span", { class: "grow" }, `Delete ${recent.length} conversation${recent.length === 1 ? "" : "s"}? Saved ones stay.`),
      h("button", { type: "button", class: "button danger", onclick: async () => {
        for (const c of recent) await conversationDelete(c.id);
        if (chat.current && recent.some((c) => c.id === chat.current.id)) {
          chat.current = null;
          chat.messages = [];
        }
        drawHistory(ctx);
      } }, "Delete"),
      h("button", { type: "button", class: "button", onclick: ask }, "Cancel"),
    );
  ask();
  return wrap;
}

// ------------------------------------------------------------------ settings: providers

const editing = { form: null }; // { id | null, preset, name, baseUrl, key, contextWindow, status }

/** Settings, scrolled to a new provider form. */
function openProviderForm(ctx) {
  editing.form = { id: null, preset: "local", name: "", baseUrl: "", key: "", contextWindow: "" };
  ctx.openPanel("settings", { section: "ai" });
}

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
        // A key bound to another address won't be sent here (st.origin is null for a key
        // saved before keys were bound; it binds to this address on first use)
        const elsewhere = st.stored && st.origin && st.origin !== originOf(p.baseUrl);
        const key = !st.stored ? (presetOf(p).keyOptional ? "no key" : "no key yet")
          : elsewhere ? `key saved for ${st.origin}: enter it again`
          : st.storage === "file" ? "key saved in the app's files" : "key in system keychain";
        el.textContent = `${p.baseUrl} · ${key}`;
      }).catch(() => {});
    }
  });
  return [
    h("p", { class: "setting-note" }, "Chat about the text with your own AI: a server on your network or an API key. Keys stay on this device and go only to the address they were saved for, which the Ask panel also contacts on its own to list the models."),
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
  const baseUrlNow = () => (fixedUrl ? preset.baseUrl : f.baseUrl).trim().replace(/\/+$/, "");
  // A saved key only ever goes to the address it was saved for (the Rust side refuses
  // any other), so a provider moved to another service or address needs it again
  const saved = f.id ? ctx.settings.ai.providers.find((p) => p.id === f.id) : null;
  const keyMoved = () => !!saved && (f.preset !== saved.preset || originOf(baseUrlNow()) !== originOf(saved.baseUrl));

  const key = h("input", { id: "provider-key", type: "password", autocomplete: "off", autocapitalize: "off", spellcheck: "false" });
  key.value = f.key; // the property, not the attribute: a typed key stays out of the markup
  key.addEventListener("input", () => { f.key = key.value; });
  const keyHint = h("span", { class: "setting-hint" }, "The saved key goes only to the address it was saved for: enter it again for this one.");
  const drawKey = () => {
    const moved = keyMoved();
    key.placeholder = moved
      ? preset.keyOptional ? "Enter the key again (optional)" : "Enter the API key again"
      : f.id ? "Saved key kept unless you enter a new one" : preset.keyOptional ? "Optional" : "Paste your API key";
    key.required = moved && !preset.keyOptional;
    keyHint.hidden = !moved;
  };
  drawKey();
  url.addEventListener("input", () => {
    f.baseUrl = url.value;
    drawKey();
  });
  const size = h("input", { id: "provider-context", type: "number", min: "1000", step: "1000", value: f.contextWindow ?? "", placeholder: "As reported by the service", inputmode: "numeric" });
  size.addEventListener("input", () => { f.contextWindow = size.value; });

  const save = async () => {
    const baseUrl = baseUrlNow();
    if (!/^https?:\/\/[^\s/]+/.test(baseUrl)) {
      f.status = "Enter the server's address, starting with http:// or https://";
      return ctx.refreshPanel();
    }
    const moved = keyMoved();
    if ((!f.id || moved) && !preset.keyOptional && !f.key.trim()) {
      f.status = moved ? "Enter the API key again: a saved key goes only to the address it was saved for." : "Paste an API key for this service.";
      return ctx.refreshPanel();
    }
    const id = f.id ?? `p${Date.now().toString(36)}`;
    const contextWindow = Number.parseInt(f.contextWindow, 10);
    try {
      if (f.key.trim()) {
        const where = await aiKeySet(id, f.key, baseUrl);
        if (where === "file") ctx.toast("No system keychain found: the key is saved in the app's private files");
      } else if (moved) {
        // The old key can't be used at the new address: forget it rather than keep it
        await aiKeySet(id, "");
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
    const baseUrl = baseUrlNow();
    // A typed key is tried as it is, never stored; without one, the saved key if it
    // belongs to this address, or no key at all (apiKey "")
    const typed = f.key.trim();
    const useSaved = !typed && !!f.id && !keyMoved();
    try {
      const list = await aiModels(
        { providerId: useSaved ? f.id : "__test__", kind: preset.kind, baseUrl },
        useSaved ? undefined : typed,
      );
      const sizes = list.map((m) => m.contextWindow).filter(Boolean);
      f.status = `Connected. ${list.length} model${list.length === 1 ? "" : "s"}${sizes.length ? `, up to ${compactLimit(Math.max(...sizes))} tokens of context` : ""}.`;
    } catch (error) {
      f.status = String(error.message ?? error);
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
    h("label", { class: "field" }, h("span", { class: "field-label" }, "API key"), key, keyHint),
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
