// The connection to Evo through the native messaging host
// (docs/features/browser-bridge/plan.md 3.3). One port per browser profile.
// Requests from Evo carry an `id`; their handler's value or error goes back as
// `result`. Messages without an id are notices. Large messages from Evo come
// as `chunk` frames and are joined here.

const HOST = "dev.evo.webbridge";
const WIRE_VERSION = 1;

/** @type {chrome.runtime.Port | null} */
let port = null;
let backoff = 1000;
let retryTimer = null;

/** What the side panel shows. */
export const status = {
  state: "connecting", // connecting | connected | evo-not-running | not-set-up | refused | error
  reason: "",
  label: "",
  app: "",
};

/** @type {Map<string, (message: any) => any>} */
const handlers = new Map();
/** @type {Set<() => void>} */
const statusListeners = new Set();
/** @type {Map<number, string[]>} */
const chunks = new Map();

/** Handle messages of type `t` from Evo. A handler's return value (or promise) answers a request. */
export function on(t, handler) {
  handlers.set(t, handler);
}

export function onStatus(listener) {
  statusListeners.add(listener);
}

function setStatus(state, reason = "") {
  status.state = state;
  status.reason = reason;
  for (const listener of statusListeners) listener();
}

/** Send a message to Evo. */
export function send(message) {
  if (!port) return false;
  try {
    port.postMessage(message);
    return true;
  } catch {
    return false;
  }
}

/** The extension's own identity for this browser profile, kept in its storage. */
export async function identity() {
  const stored = await chrome.storage.local.get(["profile", "label"]);
  let profile = stored.profile;
  if (!profile) {
    const bytes = crypto.getRandomValues(new Uint8Array(6));
    profile = "p" + Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
    await chrome.storage.local.set({ profile });
  }
  return { profile, label: stored.label || "" };
}

export function browserName() {
  const brands = navigator.userAgentData?.brands?.map((b) => b.brand) ?? [];
  return brands.some((b) => /edge/i.test(b)) || /Edg\//.test(navigator.userAgent) ? "edge" : "chrome";
}

/**
 * Connect to Evo. `launch` lets the host start Evo when it is not running; it
 * is set only when the browser itself just started (spec FR-19).
 */
let connecting = null;

export function connect(options = {}) {
  if (port) return Promise.resolve();
  // One connection at a time: the worker's start and its events may ask together.
  connecting ??= open(options).finally(() => {
    connecting = null;
  });
  return connecting;
}

async function open({ launch = false }) {
  clearTimeout(retryTimer);
  setStatus("connecting");
  const { profile, label } = await identity();
  try {
    port = chrome.runtime.connectNative(HOST);
  } catch (error) {
    port = null;
    setStatus("not-set-up", String(error?.message || error));
    return;
  }
  port.onMessage.addListener(receive);
  port.onDisconnect.addListener(() => {
    const error = chrome.runtime.lastError?.message || "";
    port = null;
    if (/not found|forbidden/i.test(error)) setStatus("not-set-up", error);
    else if (status.state === "connected" || status.state === "connecting") setStatus("evo-not-running", error);
    for (const listener of disconnectListeners) listener();
    scheduleRetry();
  });
  port.postMessage({
    t: "hello",
    v: WIRE_VERSION,
    browser: browserName(),
    profile,
    label,
    version: chrome.runtime.getManifest().version,
    debugger: typeof chrome.debugger?.attach === "function",
    launch,
  });
}

/** @type {Set<() => void>} */
const disconnectListeners = new Set();
export function onDisconnect(listener) {
  disconnectListeners.add(listener);
}

function scheduleRetry() {
  clearTimeout(retryTimer);
  if (status.state === "not-set-up") return;
  retryTimer = setTimeout(() => connect(), backoff);
  backoff = Math.min(backoff * 2, 30_000);
}

async function receive(message) {
  if (message?.t === "chunk") {
    const parts = chunks.get(message.id) ?? [];
    parts[message.seq] = message.data;
    chunks.set(message.id, parts);
    if (!message.last) return;
    chunks.delete(message.id);
    try {
      message = JSON.parse(parts.join(""));
    } catch {
      return;
    }
  }
  switch (message?.t) {
    case "welcome":
      backoff = 1000;
      status.label = message.label || "";
      status.app = message.app || "";
      if (message.label) chrome.storage.local.set({ label: message.label });
      setStatus("connected");
      return;
    case "label":
      status.label = message.label || "";
      chrome.storage.local.set({ label: status.label });
      setStatus(status.state);
      return;
    case "bye": {
      const reason = String(message.reason || "");
      setStatus(reason.startsWith("evo-not-running") ? "evo-not-running" : reason.startsWith("not-evo") ? "refused" : "error", reason);
      return;
    }
  }
  const handler = handlers.get(message?.t);
  if (message?.id === undefined) {
    if (handler) {
      try {
        await handler(message);
      } catch (error) {
        console.warn("[evo] notice failed", message.t, error);
      }
    }
    return;
  }
  if (!handler) {
    send({ t: "result", id: message.id, ok: false, code: "failed", message: `unknown request ${message.t}` });
    return;
  }
  try {
    const value = await handler(message);
    send({ t: "result", id: message.id, ok: true, value: value ?? null });
  } catch (error) {
    send({ t: "result", id: message.id, ok: false, code: error?.code || "failed", message: String(error?.message || error) });
  }
}

/** An error with a wire code (`gone`, `refused`, `restricted`, `busy`, `failed`). */
export function failure(code, message) {
  const error = new Error(message);
  error.code = code;
  return error;
}
