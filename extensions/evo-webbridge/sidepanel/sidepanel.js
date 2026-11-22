// The Evo side panel (spec US-3, US-8, FR-10, FR-11, FR-14, FR-25): a client
// of an Evo chat. It asks the worker's port (`panel` requests, relayed to
// Evo), shows the chat as the app does — its turns rebuilt and streamed by the
// app's own transcript code (thread.js) — and its open questions; the
// composer lives in composer.js.

import { Composer } from "./composer.js";
import { Thread } from "./thread.js";
import { activePage } from "./active.js";

const $ = (id) => document.getElementById(id);
let nextReq = 0;
const waiting = new Map();
/** The chat shown, and what Evo said about it. */
const view = { chat: null, title: "", model: null, effort: null, levels: [], mode: "ask", running: false, status: null, settings: {} };

/**
 * The port to the extension's worker. The browser stops an idle worker (Evo
 * closed, nothing to relay) and that closes the port: the panel connects
 * again, and once Evo answers it shows its chat again, so its events keep
 * coming to this panel.
 */
let port = null;
function connectPort() {
  port = chrome.runtime.connect({ name: "panel" });
  port.onMessage.addListener(onPortMessage);
  port.onDisconnect.addListener(() => {
    port = null;
    for (const { reject } of waiting.values()) reject(new Error("The connection to Evo was lost. Try again."));
    waiting.clear();
    // The next "connected" status opens the chat again.
    view.status = null;
    setTimeout(connectPort, 300);
  });
}

/** Ask Evo; resolves with the answer's body. Starting a chat's engine can take a while (`timeout`). */
export function ask(body, timeout = 30_000) {
  const req = ++nextReq;
  if (!port) connectPort();
  try {
    port.postMessage({ type: "panel", req, body });
  } catch {
    return Promise.reject(new Error("Evo is not connected."));
  }
  return new Promise((resolve, reject) => {
    waiting.set(req, { resolve, reject });
    setTimeout(() => {
      if (waiting.delete(req)) reject(new Error("Evo did not answer."));
    }, timeout);
  });
}

function onPortMessage(message) {
  if (message.type === "status") return showStatus(message.status);
  if (message.type === "settings") {
    view.settings = message.settings || {};
    composer.setVoice(Boolean(view.settings.voiceInput));
    return;
  }
  if (message.type !== "panel") return;
  const body = message.body || {};
  if (message.req && waiting.has(message.req)) {
    const { resolve, reject } = waiting.get(message.req);
    waiting.delete(message.req);
    if (body.op === "error") reject(new Error(body.message));
    else resolve(body);
    return;
  }
  // Every panel follows the app's approval mode and model short list.
  if (body.op === "mode") return composer.syncMode(body.mode);
  if (body.op === "pinned") return composer.syncPinned(body.pinned);
  if (body.chat && body.chat !== view.chat) return;
  switch (body.op) {
    case "state":
      return composer.applyState(body);
    // The chat's engine lines, folded by the app's transcript reducer.
    case "engine":
      clearWelcome();
      return thread.engine(body.message);
    // A prompt sent from the app (or the session hub): its turn starts here too, as in the app.
    case "prompted":
      clearWelcome();
      return thread.dispatch({ type: "prompt", text: String(body.text || ""), images: Array.isArray(body.images) && body.images.length ? body.images : undefined });
    case "fail":
      return thread.dispatch({ type: "fail", error: String(body.error || "The engine refused the message.") });
    case "engine_exited":
      return thread.dispatch({ type: "engine-exited" });
    case "question":
      return addQuestion(body.question);
    case "settled_question":
      return document.querySelector(`[data-question="${CSS.escape(body.question)}"]`)?.remove();
  }
}

// ── Icons of the welcome page ───────────────────────────────────────────────

const ICON = {
  globe: `<svg viewBox="0 0 16 16"><circle cx="8" cy="8" r="5.5"/><path d="M2.5 8h11M8 2.5c1.6 1.6 2.3 3.4 2.3 5.5S9.6 11.9 8 13.5M8 2.5C6.4 4.1 5.7 5.9 5.7 8s.7 3.9 2.3 5.5"/></svg>`,
  ask: `<svg viewBox="0 0 16 16"><path d="M8 13.5a5.5 5.5 0 1 0-4.8-2.8L2.5 13.5l2.8-.7A5.5 5.5 0 0 0 8 13.5z"/><path d="M6.6 6.5a1.5 1.5 0 1 1 2 1.4c-.4.2-.6.5-.6.9M8 10.5h.01"/></svg>`,
  shield: `<svg viewBox="0 0 16 16"><path d="M8 1.8l5.5 2.2v3.6c0 3-2.3 5.6-5.5 6.6-3.2-1-5.5-3.6-5.5-6.6V4z"/><path d="M8 5.5v3M8 10.5h.01"/></svg>`,
  sum: `<svg viewBox="0 0 16 16"><path d="M3 4h10M3 7h10M3 10h6"/></svg>`,
  find: `<svg viewBox="0 0 16 16"><circle cx="7" cy="7" r="4.2"/><path d="M10.2 10.2L13.5 13.5"/></svg>`,
  form: `<svg viewBox="0 0 16 16"><rect x="2.5" y="2.5" width="11" height="11" rx="1.5"/><path d="M5 6h6M5 9h3.5M10.5 9.5l1 1 2-2"/></svg>`,
  test: `<svg viewBox="0 0 16 16"><path d="M6 2.5h4M7 2.5v4L3.2 12a1 1 0 0 0 .8 1.5h8a1 1 0 0 0 .8-1.5L9 6.5v-4"/><path d="M4.8 10h6.4"/></svg>`,
};

// ── Connection ──────────────────────────────────────────────────────────────

const OFFLINE = {
  connecting: "Connecting to Evo…",
  "evo-not-running": "Evo is not running. Open Evo, then try again.",
  "not-set-up": "Evo is not set up for this browser yet. In Evo, open Settings → Browser and choose Set up.",
  refused: "Evo refused the connection.",
  error: "Something went wrong reaching Evo.",
};

function showStatus(status) {
  const was = view.status?.state;
  view.status = status;
  const online = status.state === "connected";
  $("offline").hidden = online || status.state === "connecting";
  $("offline-text").textContent = OFFLINE[status.state] || status.state;
  $("link-dot").className = `link-dot ${online ? "on" : status.state === "connecting" ? "" : "off"}`;
  $("link-text").textContent = online ? `Connected${status.label ? ` · ${status.label}` : ""}` : status.state === "connecting" ? "Connecting…" : "Not connected";
  if (online && was !== "connected") start();
}

$("retry").addEventListener("click", () => port?.postMessage({ type: "reconnect" }));

async function start() {
  const { last } = await chrome.storage.local.get("last");
  const chat = view.chat || last;
  if (chat) {
    const opened = await openChat(chat).then(
      () => true,
      () => false,
    );
    if (opened) return;
  }
  view.chat = null;
  thread.clear();
  renderWelcome();
  const defaults = await ask({ op: "defaults" }).catch(() => null);
  if (defaults?.mode && !view.chat) composer.syncMode(defaults.mode);
}

// ── The chat ────────────────────────────────────────────────────────────────

/** The turns, as the app draws them. */
const thread = new Thread($("thread"), {
  onRetry: (text) => send({ text, attachments: [] }),
  onOpenInApp: () => view.chat && ask({ op: "open_in_app", chat: view.chat }).catch(() => {}),
  // The provider's name as Settings shows it is the app's; its id, capitalized, reads well enough.
  providerName: (id) => (id ? id.charAt(0).toUpperCase() + id.slice(1) : undefined),
});
thread.onChange = (state) => {
  setRunning(Boolean(state.busy));
  composer.showQueued(state.queued ?? []);
};

async function openChat(chat) {
  const body = await ask({ op: "open", chat }, 90_000);
  applyChat(body);
  chrome.storage.local.set({ last: body.chat });
  clearWelcome();
  $("extras").replaceChildren();
  // The turns rebuilt from the saved messages, as the app rebuilds them;
  // opened in the middle of a turn, its last turn goes on live.
  thread.load(body.messages || [], { running: Boolean(body.running) });
  for (const question of body.questions || []) addQuestion(question);
  if (thread.empty && !(body.questions || []).length) renderWelcome();
  scrollDown();
}

function applyChat(body) {
  view.chat = body.chat;
  view.title = body.title || "New chat";
  view.model = body.model;
  view.effort = body.effort;
  view.levels = body.levels || [];
  view.mode = body.mode || "ask";
  $("chat-title").textContent = view.title;
  composer.showState(view);
}

/** The new chat being made: starting its engine takes a moment, and a message sent meanwhile waits for it. */
let creating = null;

function newChat({ welcome = true } = {}) {
  if (creating) return creating;
  if (welcome) closeMenus();
  $("history-view").hidden = true;
  // Leave the old chat at once, so nothing typed now goes to it.
  view.chat = null;
  thread.clear();
  $("extras").replaceChildren();
  $("chat-title").textContent = "New chat";
  // Sending from the welcome page keeps the message just shown: no welcome over it.
  if (welcome) renderWelcome();
  creating = (async () => {
    try {
      const body = await ask({ op: "new" }, 90_000);
      applyChat(body);
      chrome.storage.local.set({ last: body.chat });
    } finally {
      creating = null;
    }
  })();
  return creating;
}

const SUGGESTIONS = [
  { icon: "sum", title: "Summarize this page", hint: "The main points, short", text: "Summarize this page in a few bullet points." },
  { icon: "find", title: "Find the key facts", hint: "Dates, numbers, names", text: "Find the key facts on this page: dates, amounts, names and links I should know about." },
  { icon: "form", title: "Fill in a form", hint: "With what I tell you", text: "Help me fill in the form on this page. Ask me for anything you need first." },
  { icon: "test", title: "Test this page", hint: "Console, network, links", text: "Test this page: check the console for errors, failed network requests and broken links, then report what you find." },
];

/** Bumped by whatever replaces the welcome page, so a welcome still loading does not cover it. */
let welcomeGen = 0;

async function renderWelcome() {
  const gen = ++welcomeGen;
  const [tab] = await activePage();
  if (gen !== welcomeGen || !thread.empty) return;
  let site = "";
  try {
    const url = new URL(tab?.url || "");
    site = /^https?:$/.test(url.protocol) ? url.host : "";
  } catch {
    site = "";
  }
  const welcome = document.createElement("div");
  welcome.className = "welcome";
  welcome.innerHTML = `<div class="hello"><img class="mark" src="../icons/icon-128.png" alt=""><h1></h1><p></p></div>`;
  welcome.querySelector("h1").textContent = site ? "What should we do on this page?" : "What can Evo do for you?";
  welcome.querySelector("p").textContent = "Ask about the page, or let Evo work in your browser while you watch.";
  if (site) {
    const card = document.createElement("div");
    card.className = "page-card";
    const icon = tab.favIconUrl && /^https?:|^data:/.test(tab.favIconUrl) ? Object.assign(document.createElement("img"), { src: tab.favIconUrl, alt: "" }) : null;
    if (icon) card.append(icon);
    else card.insertAdjacentHTML("beforeend", `<span class="favicon">${ICON.globe}</span>`);
    const what = document.createElement("div");
    what.className = "what";
    what.append(Object.assign(document.createElement("b"), { textContent: tab.title || site }), Object.assign(document.createElement("span"), { textContent: site }));
    card.append(what);
    welcome.append(card);
  }
  const grid = document.createElement("div");
  grid.className = "suggestions";
  for (const suggestion of SUGGESTIONS) {
    const button = document.createElement("button");
    button.className = "suggestion";
    button.innerHTML = ICON[suggestion.icon];
    button.append(Object.assign(document.createElement("b"), { textContent: suggestion.title }), Object.assign(document.createElement("span"), { textContent: suggestion.hint }));
    button.addEventListener("click", () => composer.sendText(suggestion.text));
    grid.append(button);
  }
  welcome.append(grid);
  $("welcome-slot").replaceChildren(welcome);
  composer.focus();
}

function clearWelcome() {
  welcomeGen++;
  $("welcome-slot").replaceChildren();
}

function setRunning(running) {
  view.running = running;
  composer.setRunning(running);
}

function addNotice(text) {
  clearWelcome();
  $("extras").append(Object.assign(document.createElement("div"), { className: "notice", textContent: text }));
  scrollDown();
}

// ── Questions (FR-14): approvals and the agent's questions, answered here or in the app ──

function addQuestion(question) {
  if (document.querySelector(`[data-question="${CSS.escape(question.requestId)}"]`)) return;
  clearWelcome();
  const asked = question.kind === "questions" && Array.isArray(question.ui?.questions) ? question.ui.questions : null;
  const card = document.createElement("div");
  card.className = "question";
  card.dataset.question = question.requestId;
  const head = document.createElement("div");
  head.className = "head";
  head.innerHTML = asked ? ICON.ask : ICON.shield;
  const text = document.createElement("div");
  text.append(Object.assign(document.createElement("div"), { className: "title", textContent: asked ? "The agent has questions" : question.title }));
  if (question.body && !asked) text.append(Object.assign(document.createElement("div"), { className: "body", textContent: question.body }));
  head.append(text);
  card.append(head);

  /** Send an answer; the card goes once Evo took it (or the app answered first). */
  const answer = async (body) => {
    card.querySelectorAll("button, input").forEach((b) => (b.disabled = true));
    try {
      await ask({ chat: view.chat, question: question.requestId, ...body });
      card.remove();
    } catch (error) {
      card.querySelectorAll("button, input").forEach((b) => (b.disabled = false));
      if (/already answered/i.test(error.message)) card.remove();
      else addNotice(`Not answered: ${error.message}`);
    }
  };

  if (asked) {
    // The app's questions panel (questions-panel.tsx): options (one, or
    // several), an answer in your own words, Skip and Send.
    const answers = asked.map(() => ({ selected: [], other: "" }));
    const send = document.createElement("button");
    send.className = "primary";
    send.textContent = "Send";
    const ready = () => (send.disabled = !answers.every((a) => a.selected.length || a.other.trim()));
    asked.forEach((q, qi) => {
      const multi = Boolean(q?.multi_select ?? q?.multiSelect);
      const set = document.createElement("fieldset");
      set.className = "ask";
      const legend = document.createElement("legend");
      if (q?.header) legend.append(Object.assign(document.createElement("span"), { className: "tag", textContent: String(q.header) }));
      legend.append(Object.assign(document.createElement("span"), { textContent: String(q?.question ?? "") }));
      set.append(legend);
      const options = Array.isArray(q?.options) ? q.options : [];
      const buttons = [];
      options.forEach((option, oi) => {
        const label = String(option?.label ?? option);
        const button = document.createElement("button");
        button.className = `choice ${multi ? "multi" : "single"}`;
        button.setAttribute("role", multi ? "checkbox" : "radio");
        button.setAttribute("aria-checked", "false");
        const words = document.createElement("span");
        words.className = "words";
        words.append(Object.assign(document.createElement("span"), { textContent: `${oi < 9 ? `${oi + 1}  ` : ""}${label}` }));
        if (option?.description) words.append(Object.assign(document.createElement("span"), { className: "hint", textContent: String(option.description) }));
        button.append(Object.assign(document.createElement("i"), { className: "mark" }), words);
        button.addEventListener("click", () => {
          const current = answers[qi];
          if (multi) current.selected = current.selected.includes(label) ? current.selected.filter((x) => x !== label) : [...current.selected, label];
          else {
            current.selected = current.selected[0] === label ? [] : [label];
            current.other = "";
            input.value = "";
          }
          buttons.forEach((b, i) => b.setAttribute("aria-checked", String(current.selected.includes(String(options[i]?.label ?? options[i])))));
          ready();
        });
        buttons.push(button);
        set.append(button);
      });
      const input = document.createElement("input");
      input.placeholder = options.length ? "Or answer in your own words" : "Your answer";
      input.setAttribute("aria-label", `Answer to: ${q?.question ?? ""}`);
      input.addEventListener("input", () => {
        answers[qi].other = input.value;
        if (!multi && input.value) {
          answers[qi].selected = [];
          buttons.forEach((b) => b.setAttribute("aria-checked", "false"));
        }
        ready();
      });
      set.append(input);
      card.append(set);
    });
    const actions = document.createElement("div");
    actions.className = "options";
    const skip = document.createElement("button");
    skip.textContent = "Skip";
    skip.title = "The agent decides itself";
    skip.addEventListener("click", () => answer({ op: "answer_questions", answers: null }));
    send.addEventListener("click", () =>
      answer({ op: "answer_questions", answers: answers.map((a) => ({ selected: a.selected, other: a.other.trim() || undefined })) }),
    );
    ready();
    actions.append(skip, send);
    card.append(actions);
  } else {
    const options = document.createElement("div");
    options.className = "options";
    (question.options || []).forEach((option, index) => {
      const button = document.createElement("button");
      button.textContent = option;
      if (/for this site|^allow( once)?$/i.test(option)) button.className = "primary";
      button.addEventListener("click", () => answer({ op: "answer", index }));
      options.append(button);
    });
    card.append(options);
  }
  $("extras").append(card);
  scrollDown();
}

function scrollDown() {
  const list = $("messages");
  list.scrollTop = list.scrollHeight;
}

// ── Sending ─────────────────────────────────────────────────────────────────

/**
 * What the thread shows for a sent message: the text and its files as `@path`
 * lines, as the engine echoes it (`panel.rs` `prompt_parts`), so the echo
 * changes nothing.
 */
function shownPrompt(text, attachments) {
  return text.trim() + attachments.filter((a) => a.kind !== "image").map((a) => `\n@${a.path}`).join("");
}

/** Attachments as Evo reads them: saved paths, without the pictures kept for the thread. */
const forEvo = (attachments) => attachments.map(({ name, kind, path }) => ({ name, kind, path }));

async function send({ text, tab, attachments, queue = false }) {
  clearWelcome();
  // While the agent works: read at its next step, or after it (Ctrl+Enter), as in the app.
  if (thread.busy && view.chat) {
    try {
      const body = await ask({ op: "queue", chat: view.chat, text, attachments: forEvo(attachments), deliverAs: queue ? "followUp" : "steer" });
      const images = attachments.filter((a) => a.kind === "image" && a.dataUrl).map((a) => a.dataUrl);
      thread.dispatch({ type: "queued", item: { entryId: body.entryId, text: shownPrompt(text, attachments), images: images.length ? images : undefined, deliverAs: body.deliverAs } });
    } catch (error) {
      addNotice(`Not sent: ${error.message}`);
    }
    return;
  }
  if (!view.chat) newChat({ welcome: false }).catch(() => {});
  const images = attachments.filter((a) => a.kind === "image" && a.dataUrl).map((a) => a.dataUrl);
  thread.dispatch({ type: "prompt", text: shownPrompt(text, attachments), images });
  try {
    // A chat still being made gets the message once it is there.
    if (creating) await creating;
    if (!view.chat) throw new Error("The chat could not be made.");
    const sent = await ask({ op: "send", chat: view.chat, text, tab, attachments: forEvo(attachments) }, 90_000);
    if (sent?.title && sent.chat === view.chat) {
      view.title = sent.title;
      $("chat-title").textContent = sent.title;
    }
  } catch (error) {
    thread.dispatch({ type: "fail", error: `Not sent: ${error.message}` });
  }
}

const composer = new Composer({
  ask,
  view,
  onSend: send,
  onStop: () => (view.chat ? ask({ op: "stop", chat: view.chat }).catch((error) => addNotice(`Not stopped: ${error.message}`)) : null),
  ensureChat: () => (view.chat ? Promise.resolve() : newChat({ welcome: false })),
  onCancelQueued: async (item) => {
    await ask({ op: "cancel_queued", chat: view.chat, entryId: item.entryId }).catch(() => {});
    thread.dispatch({ type: "unqueued", entryId: item.entryId });
  },
  onSendNow: async (item) => {
    // A queued message becomes the next step (queued-messages.tsx "Send now").
    await ask({ op: "cancel_queued", chat: view.chat, entryId: item.entryId }).catch(() => {});
    thread.dispatch({ type: "unqueued", entryId: item.entryId });
    await send({ text: item.text, attachments: [], queue: false });
  },
});
connectPort();

// ── History, menu, close ────────────────────────────────────────────────────

function when(ms) {
  const minutes = Math.round((Date.now() - ms) / 60_000);
  if (minutes < 1) return "now";
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours}h`;
  return `${Math.round(hours / 24)}d`;
}

async function showHistory(query = "") {
  const body = await ask({ op: "chats", query }).catch((error) => ({ items: [], error }));
  if (body.error) {
    $("history-list").replaceChildren(Object.assign(document.createElement("li"), { className: "empty", textContent: body.error.message }));
    return;
  }
  const list = $("history-list");
  list.replaceChildren(
    ...(body.items || []).map((item) => {
      const li = document.createElement("li");
      const button = document.createElement("button");
      if (item.chat === view.chat) button.className = "current";
      button.append(
        Object.assign(document.createElement("span"), { className: "name", textContent: item.title }),
        Object.assign(document.createElement("span"), { className: "when", textContent: when(item.updated) }),
        Object.assign(document.createElement("span"), { className: "where", textContent: item.workspace }),
      );
      button.addEventListener("click", async () => {
        $("history-view").hidden = true;
        await openChat(item.chat).catch((error) => addNotice(`The chat did not open: ${error.message}`));
      });
      li.append(button);
      return li;
    }),
  );
}

function toggleHistory() {
  const panel = $("history-view");
  panel.hidden = !panel.hidden;
  if (!panel.hidden) {
    $("history-search").value = "";
    showHistory().catch(() => {});
    $("history-search").focus();
  }
}
$("history").addEventListener("click", toggleHistory);
// The list is the shared history: chats made or renamed in the app show here too.
$("title-button").addEventListener("click", toggleHistory);
let searchTimer;
$("history-search").addEventListener("input", (event) => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => showHistory(event.target.value).catch(() => {}), 200);
});
$("new-chat").addEventListener("click", () => newChat().catch(() => {}));
$("close").addEventListener("click", () => window.close());

function closeMenus() {
  for (const id of ["menu-list", "plus-list", "model-list", "mode-list"]) $(id).hidden = true;
}
document.addEventListener("click", (event) => {
  // `isConnected`: a click on a menu row that re-rendered the menu (a page of the model menu).
  if (event.target.isConnected && !event.target.closest(".menu, #menu, #plus, #model, #mode")) closeMenus();
});
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") closeMenus();
});

function showPermission(value) {
  for (const button of $("menu-site-permission").querySelectorAll("button")) button.setAttribute("aria-checked", String(button.dataset.value === value));
}

$("menu").addEventListener("click", async () => {
  const list = $("menu-list");
  const show = list.hidden;
  closeMenus();
  if (!show) return;
  const info = await chrome.runtime.sendMessage({ type: "site-info" }).catch(() => null);
  $("menu-site").hidden = !info?.site;
  $("menu-site-name").textContent = info?.site || "";
  showPermission(info?.permission || "ask");
  list.hidden = false;
});
$("menu-site-permission").addEventListener("click", (event) => {
  const value = event.target.closest("button")?.dataset.value;
  if (!value) return;
  showPermission(value);
  chrome.runtime.sendMessage({ type: "set-site", site: $("menu-site-name").textContent, permission: value });
});
$("menu-stop-all").addEventListener("click", () => {
  closeMenus();
  chrome.runtime.sendMessage({ type: "stop-all" });
});
$("menu-open-app").addEventListener("click", () => {
  closeMenus();
  if (view.chat) ask({ op: "open_in_app", chat: view.chat }).catch(() => {});
});
