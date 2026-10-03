// The study assistant: chat with a language model about passages the reader
// chooses, in any translations, with commentaries and cross-references attached
// (context.js), using the reader's own AI provider (a local server or their API
// key). Also the provider setup shown in Settings.

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
import { APP } from "./brand.js";
import { compact, compactLimit, contextEditorMoved, fixed, loadCatalogues, openContextEditor, resolve, sizeOf } from "./context.js";
import { h, icon, replace, timeAgo } from "./dom.js";
import { referenceFinder, renderMarkdown } from "./markdown.js";
import { sanitizeContext } from "./settings.js";

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
/** The instructions, in tokens, until Rust has counted them (crates/core/src/context.rs). */
const INSTRUCTION_TOKENS = 600;

const chat = {
  // { role, content, reasoning, scopeLabel, context, usage, error, reason, streaming }: a
  // question keeps the context it was asked with (every passage fixed where it was)
  messages: [],
  requestId: null,
  pendingConsent: null, // text waiting for the reader to allow a provider
  models: new Map(), // providerId -> { list, error, loading }
  size: null, // the context's size (context.js sizeOf)
  sizing: 0, // which size request is the latest
  // The open saved conversation's own context (as its last question was asked), used
  // in place of the reader's usual one until a new conversation starts
  override: null,
  draft: "",
  view: null, // current DOM references
  // The open conversation as saved: { id, title, created, starred }; null until the first question
  current: null,
  showHistory: false,
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

/** The context the next question goes with: this conversation's, or the reader's. */
function activeContext(ctx) {
  return chat.override ?? ctx.settings.ai.context;
}

/** Change the context in use (the conversation's own, or the reader's saved one). */
function changeContext(ctx, mutator) {
  if (chat.override) mutator(chat.override);
  else ctx.changeSettings((s) => mutator(s.ai.context));
}

/** Size the context in use again; the latest request wins. */
async function refreshSize(ctx) {
  const seq = ++chat.sizing;
  let size;
  try {
    size = await sizeOf(ctx, activeContext(ctx), chat.size);
  } catch (error) {
    size = { key: null, label: "", tokens: 0, verses: 0, passages: [], error: String(error.message ?? error) };
  }
  if (seq !== chat.sizing) return;
  chat.size = size;
  drawBudget(ctx);
}

/** Tokens the next request will use before the answer, corrected for this model. */
function promptTokens(ctx, extraText = "", contextTokens = chat.size?.tokens ?? 0) {
  const factor = ctx.settings.ai.calibration[calibrationKey(ctx)] ?? 1;
  const history = chat.messages.reduce((n, m) => n + estimateTokens(m.content ?? ""), 0);
  const instructions = chat.size?.instructions ?? INSTRUCTION_TOKENS;
  return Math.ceil((contextTokens + instructions + history + estimateTokens(extraText)) * factor);
}

function answerTokens(ctx) {
  const max = modelInfo(ctx)?.maxOutput;
  return max ? Math.min(max, ANSWER_TOKENS) : ANSWER_TOKENS;
}


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
        h("p", {}, "Ask questions about any passages, in any of the translations, with commentaries and cross-references attached for the model to read."),
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
  const consent = h("div", { class: "chat-consent", hidden: true });

  chat.view = { body, input, sendButton, messages, jump, budget, consent };
  chat.view.follow = follower(messages, drawJump);
  jump.addEventListener("click", () => chat.view.follow.resume());

  const history = h("div", { class: "chat-history" });
  chat.view.history = history;

  replace(
    body,
    h(
      "div",
      { class: "chat", "data-mode": chat.showHistory ? "history" : "chat" },
      h("div", { class: "chat-top" }, modelPicker(ctx), budget),
      h("div", { class: "chat-scroll" }, messages, jump),
      history,
      consent,
      h("div", { class: "chat-compose" }, input, sendButton),
    ),
  );
  if (chat.showHistory) drawHistory(ctx);
  if (previous && !previous.following) chat.view.follow.following = false;
  drawMessages(ctx);
  if (previous && !previous.following) messages.scrollTop = previous.top;
  drawSend();
  drawConsent(ctx);
  refreshSize(ctx);
  // Every provider's models, so any of them can be picked from the menu
  for (const p of ai.providers) loadModels(ctx, { p });
  autosize(input);
  drawBudget(ctx);
  return input;
}

/** The reader moved (new chapter or verse): passages that follow them have changed. */
export function chatScopeChanged(ctx) {
  if (chat.view) refreshSize(ctx);
  contextEditorMoved(ctx);
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
          chat.messages = [];
          chat.current = null;
          chat.override = null;
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
          h("button", { type: "button", class: "text-button", onclick: () => loadModels(ctx, { force: true }) }, "Retry"),
        )
      : null,
  );
}

/** Load a provider's models (the current one unless `p` is given). */
function loadModels(ctx, { force = false, p = provider(ctx) } = {}) {
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

// ------------------------------------------------------------------ context

/** Open the context editor on the context in use. */
function editContext(ctx) {
  openContextEditor(ctx, {
    get: () => activeContext(ctx),
    set: (mutator) => changeContext(ctx, mutator),
    conversation: !!chat.override,
    budget: (tokens) => {
      const limit = contextWindow(ctx);
      const reserve = answerTokens(ctx);
      return { limit, reserve, fits: !limit || promptTokens(ctx, chat.view?.input.value ?? "", tokens) + reserve <= limit };
    },
    onSize: (size) => {
      chat.sizing++;
      chat.size = size;
      drawBudget(ctx);
    },
    onChange: () => refreshSize(ctx),
  });
}

function drawBudget(ctx) {
  const v = chat.view;
  if (!v) return;
  const size = chat.size;
  const none = !activeContext(ctx).passages.length;
  const limit = contextWindow(ctx);
  const used = promptTokens(ctx, v.input.value);
  const fits = !limit || used + answerTokens(ctx) <= limit;
  // Share of the room left after the answer's reserve
  const fraction = limit ? Math.min(1, used / Math.max(1, limit - answerTokens(ctx))) : 0;
  const label = none ? "Nothing attached" : size?.error ? "Couldn’t read the passages" : size?.label || (size ? "Nothing yet" : "…");
  const detail = none || !size ? "" : limit ? `≈${compact(used)} of ${compactLimit(limit)} tokens` : `≈${compact(used)} tokens`;
  replace(
    v.budget,
    h(
      "button",
      {
        type: "button",
        class: "chat-scope-button",
        "aria-haspopup": "dialog",
        title: "Choose what the assistant reads",
        onclick: () => editContext(ctx),
      },
      h("span", { class: "chat-scope-label" }, h("span", { class: "muted" }, chat.override ? "This conversation reads: " : "Reads: "), label),
      h("span", { class: `chat-scope-size${fits ? "" : " over"}` }, detail),
      h("span", { class: "chat-scope-edit" }, "Change"),
    ),
    limit && !none ? meter(fraction, fits) : null,
    size?.error ? h("p", { class: "chat-error small" }, size.error) : null,
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
/** Update the answer being streamed, at most once per frame. */
function drawStreaming(ctx) {
  if (drawQueued) return;
  drawQueued = true;
  requestAnimationFrame(() => {
    drawQueued = false;
    const v = chat.view;
    if (!v) return;
    const index = chat.messages.length - 1;
    const m = chat.messages[index];
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

/** The answer ended: add its status and tools without touching what's already shown. */
function finishLive(ctx) {
  const v = chat.view;
  if (!v) return;
  const index = chat.messages.length - 1;
  const m = chat.messages[index];
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

function drawSend() {
  const v = chat.view;
  if (!v) return;
  const busy = !!chat.requestId;
  // Kept current here: it's drawn once, but the conversation changes under it
  const fresh = v.body.querySelector("[data-new-conversation]");
  if (fresh) fresh.disabled = busy || (!chat.messages.length && !chat.showHistory);
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
    drawBudget(ctx);
    ctx.toast("Too large for this model: attach less");
    editContext(ctx);
    return;
  }
  const known = await loadCatalogues().catch(() => null);
  const context = activeContext(ctx);
  const spec = chat.size?.spec ?? resolve(ctx, context, known).spec;

  if (!chat.current) {
    const now = Date.now();
    chat.current = { id: `c${now.toString(36)}${Math.random().toString(36).slice(2, 8)}`, title: titleFor(text), created: now, starred: false };
  }
  const history = chat.messages
    .filter((m) => m.content && !m.error)
    .map((m) => ({ role: m.role, content: m.content }));
  history.push({ role: "user", content: text });
  chat.messages.push({
    role: "user",
    content: text,
    scopeLabel: spec.passages.length ? (chat.size?.label ?? null) : null,
    context: fixed(ctx, context, known, chat.size),
  });
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
        context: spec,
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
        drawStreaming(ctx);
      },
    );
  } catch (error) {
    answer.error = String(error.message ?? error);
  } finally {
    answer.content = answer.content.replace(/^\s+/, "");
    answer.streaming = false;
    chat.requestId = null;
    finishLive(ctx);
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
  const url = `${APP.issues}/new?labels=ai-report&title=${encodeURIComponent("AI answer report")}&body=${encodeURIComponent(body)}`;
  openExternal(url);
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
  try {
    const c = await conversationLoad(id);
    chat.messages = (c.messages ?? []).map((m) => ({ ...m, streaming: false }));
    chat.current = { id: c.id, title: c.title, created: c.created, starred: !!c.starred };
    // Follow-up questions go with what the last one was asked with
    const asked = chat.messages.findLast((m) => m.role === "user" && m.context);
    chat.override = asked ? sanitizeContext(asked.context) : null;
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
        { type: "button", class: "row-button", onclick: () => openConversation(ctx, c.id) },
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
            chat.override = null;
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
          chat.override = null;
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
