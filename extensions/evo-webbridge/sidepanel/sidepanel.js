// The Evo side panel (spec US-3, US-8, FR-10, FR-11, FR-14, FR-25): a client
// of an Evo chat. It asks the worker's port (`panel` requests, relayed to
// Evo), shows the chat's messages, the agent's text and tool activity as they
// stream, and its open questions; the composer lives in composer.js.

import { Composer } from "./composer.js";

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

// ── Connection ──────────────────────────────────────────────────────────────

const OFFLINE = {
  connecting: "Connecting to Evo…",
  "evo-not-running": "Evo is not running. Open Evo, then try again.",
  "not-set-up": "Evo is not set up for this browser yet. In Evo, open Settings → Webbridge and choose Set up.",
  refused: "Evo refused the connection.",
  error: "Something went wrong reaching Evo.",
};

function showStatus(status) {
  const was = view.status?.state;
  view.status = status;
  const online = status.state === "connected";
  $("offline").hidden = online;
  $("offline-text").textContent = OFFLINE[status.state] || status.state;
  $("retry").hidden = status.state === "connecting";
  if (online && was !== "connected") start();
}

$("retry").addEventListener("click", () => port.postMessage({ type: "reconnect" }));

async function start() {
  const { last } = await chrome.storage.local.get("last");
  if (last) await openChat(last).catch(() => renderEmpty());
  else renderEmpty();
}

// ── The chat ────────────────────────────────────────────────────────────────

async function openChat(chat) {
  const body = await ask({ op: "open", chat });
  applyChat(body);
  chrome.storage.local.set({ last: body.chat });
  $("messages").replaceChildren();
  for (const message of body.messages || []) addMessage(message.role, message.text, message.tools);
  for (const question of body.questions || []) addQuestion(question);
  if (!(body.messages || []).length) renderEmpty();
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

async function newChat() {
  closeMenus();
  $("history-view").hidden = true;
  const body = await ask({ op: "new" });
  applyChat(body);
  chrome.storage.local.set({ last: body.chat });
  $("messages").replaceChildren();
  renderEmpty();
}

async function renderEmpty() {
  const [tab] = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
  let site = "";
  try {
    const url = new URL(tab?.url || "");
    site = /^https?:$/.test(url.protocol) ? url.host : "";
  } catch {
    site = "";
  }
  const empty = document.createElement("div");
  empty.className = "empty";
  empty.innerHTML = `<img src="../icons/icon-128.png" alt=""><div>How can Evo help with this page?</div>`;
  if (site) {
    const line = document.createElement("div");
    line.className = "site";
    line.textContent = site;
    empty.append(line);
  }
  $("messages").replaceChildren(empty);
  composer.focus();
}

function addMessage(role, text, tools = []) {
  document.querySelector(".empty")?.remove();
  const node = document.createElement("div");
  node.className = `msg ${role}`;
  setText(node, text || "");
  $("messages").append(node);
  for (const name of tools || []) addTool(name, "");
  return node;
}

/** Text with `code` spans; never HTML from the chat (spec 3.7). */
function setText(node, text) {
  node.replaceChildren();
  const parts = text.split(/(`[^`\n]+`)/);
  for (const part of parts) {
    if (part.startsWith("`") && part.endsWith("`") && part.length > 2) {
      const code = document.createElement("code");
      code.textContent = part.slice(1, -1);
      node.append(code);
    } else {
      node.append(document.createTextNode(part));
    }
  }
  node.dataset.raw = text;
}

function addTool(name, summary, error = false) {
  const line = document.createElement("div");
  line.className = `tool${error ? " error" : ""}`;
  line.textContent = `> ${String(name).replace(/^browser_/, "")}${summary ? ` ${summary}` : ""}`;
  $("messages").append(line);
}

let streaming = null;
let indicator = null;

function onEvent(event) {
  switch (event.kind) {
    case "start":
      setRunning(true);
      break;
    case "user":
      if (!pendingEcho.delete(event.text)) addMessage("user", event.text);
      break;
    case "text":
      if (!streaming) streaming = addMessage("assistant", "");
      setText(streaming, (streaming.dataset.raw || "") + (event.delta || ""));
      break;
    case "message_end":
      streaming = null;
      break;
    case "tool":
      streaming = null;
      addTool(event.name, event.summary);
      break;
    case "tool_end":
      if (event.error) addTool(event.name, "failed", true);
      break;
    case "settled":
      streaming = null;
      setRunning(false);
      break;
  }
  scrollDown();
}

function setRunning(running) {
  view.running = running;
  composer.setRunning(running);
  if (running && !indicator) {
    indicator = document.createElement("div");
    indicator.className = "working";
    indicator.innerHTML = `<span class="pulse"></span><span>Working…</span>`;
  }
  if (running) $("messages").append(indicator);
  else indicator?.remove();
}

function addQuestion(question) {
  if (document.querySelector(`[data-question="${CSS.escape(question.requestId)}"]`)) return;
  document.querySelector(".empty")?.remove();
  const card = document.createElement("div");
  card.className = "question";
  card.dataset.question = question.requestId;
  const title = document.createElement("div");
  title.className = "title";
  title.textContent = question.title;
  card.append(title);
  if (question.body) {
    const body = document.createElement("div");
    body.className = "body";
    body.textContent = question.body;
    card.append(body);
  }
  const options = document.createElement("div");
  options.className = "options";
  (question.options || []).forEach((option, index) => {
    const button = document.createElement("button");
    button.textContent = option;
    if (/for this site|^allow$/i.test(option)) button.className = "primary";
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
  if (!view.chat) await newChat();
  addMessage("user", text);
  pendingEcho.add(text);
  setRunning(true);
  try {
    await ask({ op: "send", chat: view.chat, text, tab, attachments });
  } catch (error) {
    setRunning(false);
    addTool("not sent", error.message, true);
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

async function showHistory(query = "") {
  const body = await ask({ op: "chats", query });
  const list = $("history-list");
  list.replaceChildren(
    ...(body.items || []).map((item) => {
      const li = document.createElement("li");
      const button = document.createElement("button");
      const name = document.createElement("span");
      name.textContent = item.title;
      const where = document.createElement("span");
      where.className = "where";
      where.textContent = item.workspace;
      button.append(name, where);
      button.addEventListener("click", async () => {
        $("history-view").hidden = true;
        await openChat(item.chat);
      });
      li.append(button);
      return li;
    }),
  );
}

$("history").addEventListener("click", () => {
  const panel = $("history-view");
  panel.hidden = !panel.hidden;
  if (!panel.hidden) {
    $("history-search").value = "";
    showHistory().catch(() => {});
    $("history-search").focus();
  }
});
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

$("menu").addEventListener("click", async () => {
  const list = $("menu-list");
  const show = list.hidden;
  closeMenus();
  if (!show) return;
  const info = await chrome.runtime.sendMessage({ type: "popup-info" }).catch(() => null);
  $("menu-site").hidden = !info?.site;
  $("menu-site-name").textContent = info?.site || "";
  $("menu-site-permission").value = info?.permission || "ask";
  list.hidden = false;
});
$("menu-site-permission").addEventListener("change", (event) => {
  chrome.runtime.sendMessage({ type: "set-site", site: $("menu-site-name").textContent, permission: event.target.value });
});
$("menu-stop-all").addEventListener("click", () => {
  closeMenus();
  chrome.runtime.sendMessage({ type: "stop-all" });
});
$("menu-open-app").addEventListener("click", () => {
  closeMenus();
  if (view.chat) ask({ op: "open_in_app", chat: view.chat }).catch(() => {});
});
