// The Evo side panel (spec US-3, US-8, FR-10, FR-11, FR-14, FR-25): a client
// of an Evo chat. It asks the worker's port (`panel` requests, relayed to
// Evo), shows the chat's messages, the agent's text and steps as they stream,
// and its open questions; the composer lives in composer.js.

import { Composer } from "./composer.js";
import { renderMarkdown } from "./markdown.js";
import { activePage } from "./active.js";

const $ = (id) => document.getElementById(id);
const port = chrome.runtime.connect({ name: "panel" });
let nextReq = 0;
const waiting = new Map();
/** The chat shown, and what Evo said about it. */
const view = { chat: null, title: "", model: null, effort: null, mode: "ask", running: false, status: null, settings: {} };

/** Ask Evo; resolves with the answer's body. */
export function ask(body) {
  const req = ++nextReq;
  port.postMessage({ type: "panel", req, body });
  return new Promise((resolve, reject) => {
    waiting.set(req, { resolve, reject });
    setTimeout(() => {
      if (waiting.delete(req)) reject(new Error("Evo did not answer."));
    }, 30_000);
  });
}

port.onMessage.addListener((message) => {
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
  if (body.chat && body.chat !== view.chat) return;
  if (body.op === "event") onEvent(body.event);
  if (body.op === "question") addQuestion(body.question);
  if (body.op === "settled_question") document.querySelector(`[data-question="${CSS.escape(body.question)}"]`)?.remove();
});

// ── Icons ───────────────────────────────────────────────────────────────────

const ICON = {
  globe: `<svg viewBox="0 0 16 16"><circle cx="8" cy="8" r="5.5"/><path d="M2.5 8h11M8 2.5c1.6 1.6 2.3 3.4 2.3 5.5S9.6 11.9 8 13.5M8 2.5C6.4 4.1 5.7 5.9 5.7 8s.7 3.9 2.3 5.5"/></svg>`,
  click: `<svg viewBox="0 0 16 16"><path d="M5 3l7 4.5-3 .8 2 3.4-1.4.8-2-3.4-2.3 2z"/></svg>`,
  type: `<svg viewBox="0 0 16 16"><rect x="2" y="4.5" width="12" height="7" rx="1.5"/><path d="M4.5 7h.01M7 7h.01M9.5 7h.01M5 9.2h6"/></svg>`,
  eye: `<svg viewBox="0 0 16 16"><path d="M1.8 8S4 3.8 8 3.8 14.2 8 14.2 8 12 12.2 8 12.2 1.8 8 1.8 8z"/><circle cx="8" cy="8" r="1.9"/></svg>`,
  camera: `<svg viewBox="0 0 16 16"><rect x="2" y="4" width="12" height="9" rx="1.5"/><circle cx="8" cy="8.5" r="2.2"/><path d="M5.5 4l1-1.5h3l1 1.5"/></svg>`,
  code: `<svg viewBox="0 0 16 16"><path d="M5.5 4.5L2 8l3.5 3.5M10.5 4.5L14 8l-3.5 3.5"/></svg>`,
  wait: `<svg viewBox="0 0 16 16"><circle cx="8" cy="8" r="5.5"/><path d="M8 5v3l2 1.2"/></svg>`,
  net: `<svg viewBox="0 0 16 16"><path d="M2 8h3l1.5-3.5 3 7L11 8h3"/></svg>`,
  file: `<svg viewBox="0 0 16 16"><path d="M9 2.5H4.5a1 1 0 0 0-1 1v9a1 1 0 0 0 1 1h7a1 1 0 0 0 1-1V6z"/><path d="M9 2.5V6h3.5"/></svg>`,
  tool: `<svg viewBox="0 0 16 16"><path d="M10.5 2.8a3 3 0 0 0-3.7 3.9L2.7 10.8a1.3 1.3 0 0 0 1.9 1.9l4.1-4.1a3 3 0 0 0 3.9-3.7l-1.8 1.8-1.6-.4-.4-1.6z"/></svg>`,
  ask: `<svg viewBox="0 0 16 16"><path d="M8 1.8l5.5 2.2v3.6c0 3-2.3 5.6-5.5 6.6-3.2-1-5.5-3.6-5.5-6.6V4z"/><path d="M8 5.5v3M8 10.5h.01"/></svg>`,
  sum: `<svg viewBox="0 0 16 16"><path d="M3 4h10M3 7h10M3 10h6"/></svg>`,
  find: `<svg viewBox="0 0 16 16"><circle cx="7" cy="7" r="4.2"/><path d="M10.2 10.2L13.5 13.5"/></svg>`,
  form: `<svg viewBox="0 0 16 16"><rect x="2.5" y="2.5" width="11" height="11" rx="1.5"/><path d="M5 6h6M5 9h3.5M10.5 9.5l1 1 2-2"/></svg>`,
  test: `<svg viewBox="0 0 16 16"><path d="M6 2.5h4M7 2.5v4L3.2 12a1 1 0 0 0 .8 1.5h8a1 1 0 0 0 .8-1.5L9 6.5v-4"/><path d="M4.8 10h6.4"/></svg>`,
};

/** A tool's icon and how a person says what it does. */
function describeTool(name, summary) {
  const short = String(name || "").replace(/^browser_/, "");
  const table = {
    navigate: ["globe", "Opened"],
    tabs: ["globe", "Tabs"],
    click: ["click", "Clicked"],
    hover: ["click", "Hovered"],
    drag: ["click", "Dragged"],
    type: ["type", "Typed"],
    key: ["type", "Pressed"],
    form_input: ["form", "Filled"],
    read_page: ["eye", "Read the page"],
    find: ["find", "Looked for"],
    frames: ["eye", "Read frames"],
    screenshot: ["camera", "Took a screenshot"],
    pdf: ["file", "Saved a PDF"],
    eval: ["code", "Ran a script"],
    wait: ["wait", "Waited"],
    network: ["net", "Read network"],
    console: ["code", "Read the console"],
    mock: ["net", "Mocked requests"],
    upload: ["file", "Attached a file"],
    scroll: ["eye", "Scrolled"],
    viewport: ["eye", "Set the viewport"],
  };
  const [icon, verb] = table[short] || ["tool", short.replace(/_/g, " ")];
  return { icon: ICON[icon], verb, detail: summary || "" };
}

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

$("retry").addEventListener("click", () => port.postMessage({ type: "reconnect" }));

async function start() {
  const { last } = await chrome.storage.local.get("last");
  if (last) await openChat(last).catch(() => renderWelcome());
  else renderWelcome();
}

// ── The chat ────────────────────────────────────────────────────────────────

async function openChat(chat) {
  const body = await ask({ op: "open", chat });
  applyChat(body);
  chrome.storage.local.set({ last: body.chat });
  $("messages").replaceChildren();
  steps = null;
  for (const message of body.messages || []) {
    if (message.role === "user") addUser(message.text);
    else {
      if (message.text) addAssistant(message.text);
      for (const name of message.tools || []) addStep(name, "", "done");
    }
  }
  for (const question of body.questions || []) addQuestion(question);
  if (!(body.messages || []).length && !(body.questions || []).length) renderWelcome();
  scrollDown();
}

function applyChat(body) {
  view.chat = body.chat;
  view.title = body.title || "New chat";
  view.model = body.model;
  view.effort = body.effort;
  view.mode = body.mode || "ask";
  $("chat-title").textContent = view.title;
  composer.showState(view);
}

/** The new chat being made: starting its engine takes a moment, and a message sent meanwhile waits for it. */
let creating = null;

function newChat() {
  if (creating) return creating;
  closeMenus();
  $("history-view").hidden = true;
  // Leave the old chat at once, so nothing typed now goes to it.
  view.chat = null;
  steps = null;
  $("chat-title").textContent = "New chat";
  renderWelcome();
  creating = (async () => {
    try {
      const body = await ask({ op: "new" });
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

async function renderWelcome() {
  const [tab] = await activePage();
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
  $("messages").replaceChildren(welcome);
  composer.focus();
}

function clearWelcome() {
  document.querySelector(".welcome")?.remove();
}

function addUser(text) {
  clearWelcome();
  steps = null;
  const node = document.createElement("div");
  node.className = "msg user";
  const bubble = document.createElement("div");
  bubble.className = "bubble";
  bubble.textContent = text;
  node.append(bubble);
  $("messages").append(node);
  return node;
}

function addAssistant(text) {
  clearWelcome();
  steps = null;
  const node = document.createElement("div");
  node.className = "msg assistant";
  node.innerHTML = `<img class="avatar" src="../icons/icon-48.png" alt="">`;
  const body = document.createElement("div");
  body.className = "body";
  node.append(body);
  setMarkdown(body, text);
  $("messages").append(node);
  return body;
}

/** Markdown the agent wrote, built as DOM nodes: no HTML from the chat reaches the page (plan 3.7). */
function setMarkdown(body, text) {
  body.dataset.raw = text;
  body.replaceChildren(renderMarkdown(text));
}

/** The agent's steps between its messages, grouped (`steps`). */
let steps = null;

function addStep(name, summary, state = "running") {
  clearWelcome();
  if (!steps) {
    steps = document.createElement("div");
    steps.className = "steps";
    $("messages").append(steps);
  }
  const { icon, verb, detail } = describeTool(name, summary);
  const step = document.createElement("div");
  step.className = `step ${state}`;
  step.dataset.tool = name;
  step.innerHTML = icon;
  const what = document.createElement("span");
  what.className = "what";
  what.textContent = verb;
  if (detail) what.append(" ", Object.assign(document.createElement("em"), { textContent: detail }));
  step.append(what, Object.assign(document.createElement("span"), { className: "state" }));
  steps.append(step);
  // A long run folds its earlier steps.
  const all = steps.querySelectorAll(".step");
  if (all.length > 6 && !steps.querySelector("details")) {
    const details = document.createElement("details");
    details.append(Object.assign(document.createElement("summary"), { textContent: "Earlier steps" }));
    steps.prepend(details);
  }
  if (all.length > 6) steps.querySelector("details").append(all[0]);
  return step;
}

function finishStep(name, error) {
  const running = [...document.querySelectorAll(".step.running")].reverse().find((s) => s.dataset.tool === name) || document.querySelector(".step.running");
  if (running) running.className = `step ${error ? "error" : "done"}`;
}

let streaming = null;
let indicator = null;

function onEvent(event) {
  switch (event.kind) {
    case "start":
      setRunning(true);
      break;
    case "user":
      if (!pendingEcho.delete(event.text)) addUser(event.text);
      break;
    case "text":
      if (!streaming) {
        streaming = addAssistant("");
        streaming.classList.add("cursor");
      }
      setMarkdown(streaming, (streaming.dataset.raw || "") + (event.delta || ""));
      break;
    case "message_end":
      streaming?.classList.remove("cursor");
      streaming = null;
      break;
    case "tool":
      streaming?.classList.remove("cursor");
      streaming = null;
      addStep(event.name, event.summary);
      break;
    case "tool_end":
      finishStep(event.name, event.error);
      break;
    case "settled":
      streaming?.classList.remove("cursor");
      streaming = null;
      document.querySelectorAll(".step.running").forEach((s) => (s.className = "step done"));
      setRunning(false);
      if (event.outcome === "failed") addNotice("The run failed. Check the model in Evo.");
      break;
  }
  if (view.running) $("messages").append(indicator);
  scrollDown();
}

function setRunning(running) {
  view.running = running;
  composer.setRunning(running);
  if (running && !indicator) {
    indicator = document.createElement("div");
    indicator.className = "working";
    indicator.textContent = "Working…";
  }
  if (running) $("messages").append(indicator);
  else indicator?.remove();
}

function addNotice(text) {
  $("messages").append(Object.assign(document.createElement("div"), { className: "notice", textContent: text }));
}

function addQuestion(question) {
  if (document.querySelector(`[data-question="${CSS.escape(question.requestId)}"]`)) return;
  clearWelcome();
  const card = document.createElement("div");
  card.className = "question";
  card.dataset.question = question.requestId;
  const head = document.createElement("div");
  head.className = "head";
  head.innerHTML = ICON.ask;
  const text = document.createElement("div");
  text.append(Object.assign(document.createElement("div"), { className: "title", textContent: question.title }));
  if (question.body) text.append(Object.assign(document.createElement("div"), { className: "body", textContent: question.body }));
  head.append(text);
  card.append(head);
  const options = document.createElement("div");
  options.className = "options";
  (question.options || []).forEach((option, index) => {
    const button = document.createElement("button");
    button.textContent = option;
    if (/for this site|^allow( once)?$/i.test(option)) button.className = "primary";
    button.addEventListener("click", async () => {
      card.querySelectorAll("button").forEach((b) => (b.disabled = true));
      await ask({ op: "answer", chat: view.chat, question: question.requestId, index }).catch(() => {});
      card.remove();
    });
    options.append(button);
  });
  card.append(options);
  $("messages").append(card);
  scrollDown();
}

function scrollDown() {
  const list = $("messages");
  list.scrollTop = list.scrollHeight;
}

// ── Sending ─────────────────────────────────────────────────────────────────

const pendingEcho = new Set();

async function send({ text, tab, attachments }) {
  if (!view.chat) newChat().catch(() => {});
  addUser(text);
  pendingEcho.add(text);
  setRunning(true);
  try {
    // A chat still being made gets the message once it is there.
    if (creating) await creating;
    const sent = await ask({ op: "send", chat: view.chat, text, tab, attachments });
    if (sent?.title && sent.chat === view.chat) {
      view.title = sent.title;
      $("chat-title").textContent = sent.title;
    }
  } catch (error) {
    setRunning(false);
    addNotice(`Not sent: ${error.message}`);
  }
  scrollDown();
}

const composer = new Composer({
  ask,
  view,
  onSend: send,
  onStop: () => ask({ op: "stop", chat: view.chat }).catch(() => {}),
});

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
  const body = await ask({ op: "chats", query });
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
        await openChat(item.chat);
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
$("title-button").addEventListener("click", toggleHistory);
let searchTimer;
$("history-search").addEventListener("input", (event) => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => showHistory(event.target.value).catch(() => {}), 200);
});
$("new-chat").addEventListener("click", () => newChat().catch(() => {}));
$("close").addEventListener("click", () => window.close());

function closeMenus() {
  for (const id of ["menu-list", "plus-list", "model-list"]) $(id).hidden = true;
}
document.addEventListener("click", (event) => {
  if (!event.target.closest(".menu, #menu, #plus, #model")) closeMenus();
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
