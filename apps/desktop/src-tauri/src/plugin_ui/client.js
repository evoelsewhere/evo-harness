// window.evo: what a plugin view gets (docs/features/plugin-ui/spec.md, FR-10,
// FR-12). Injected before the page's own scripts; every call is a POST to
// /__bridge on the view's own origin, checked by the shell (bridge.rs).
// The shell sends events with window.__evo_dispatch({kind, …}).
(() => {
  if (window.evo) return;
  const boot = __EVO_BOOT__;

  class EvoError extends Error {
    constructor(code, message) {
      super(message);
      this.name = "EvoError";
      this.code = code;
    }
  }

  let seq = 0;
  async function call(name, args) {
    let response;
    try {
      response = await fetch("/__bridge", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ id: ++seq, call: name, args: args || {} }),
      });
    } catch (error) {
      throw new EvoError("unreachable", "Evo did not answer: " + String(error));
    }
    let reply;
    try {
      reply = await response.json();
    } catch {
      throw new EvoError("bad_reply", "Evo answered " + response.status);
    }
    if (reply && reply.ok) return reply.value;
    throw new EvoError((reply && reply.code) || "failed", (reply && reply.message) || "the call failed");
  }

  // ── Theme (FR-12): tokens become --evo-<name> on the root element ──────────
  let theme = boot.theme || { tokens: {}, dark: false };
  const themeListeners = new Set();
  function applyTheme() {
    const root = document.documentElement;
    if (!root) return;
    for (const [name, value] of Object.entries(theme.tokens || {})) root.style.setProperty("--evo-" + name, String(value));
    root.dataset.evoTheme = theme.dark ? "dark" : "light";
    root.style.colorScheme = theme.dark ? "dark" : "light";
  }
  applyTheme();
  if (!document.documentElement) document.addEventListener("readystatechange", applyTheme, { once: true });
  document.addEventListener("DOMContentLoaded", applyTheme, { once: true });

  // ── Chat events: one subscription per chat ("" = the window's open chat) ──
  const chatListeners = new Map();
  function onChatEvent(fn, chat) {
    if (typeof fn !== "function") throw new TypeError("chat.onEvent takes a function");
    const key = chat || "";
    let set = chatListeners.get(key);
    const first = !set;
    if (!set) chatListeners.set(key, (set = new Set()));
    set.add(fn);
    const ready = first ? call("chat.subscribe", chat ? { chat } : {}) : Promise.resolve();
    ready.catch((error) => {
      set.delete(fn);
      if (set.size === 0) chatListeners.delete(key);
      console.error("[evo] chat.onEvent:", error);
    });
    return () => {
      if (!set.delete(fn) || set.size > 0) return;
      chatListeners.delete(key);
      call("chat.unsubscribe", chat ? { chat } : {}).catch(() => {});
    };
  }

  let data = null;
  const dataLoaded = boot.view.slot === "cards" ? call("view.data").then((value) => (data = value)) : Promise.resolve(null);

  function emit(set, ...args) {
    for (const fn of [...set]) {
      try {
        fn(...args);
      } catch (error) {
        console.error("[evo] listener failed:", error);
      }
    }
  }

  // ── Plugin AI (plugin-ai FR-1 – FR-3, FR-24): a run is an async iterator of
  // events with .id, .result and .cancel(). Events can come before ai.run
  // answers with the id: they wait in `early`.
  const aiRuns = new Map();
  const early = new Map();
  // The view's tool functions per run (FR-10), and calls that came first.
  const aiTools = new Map();
  const earlyCalls = new Map();
  // options.tools: [{name, description, inputSchema, run}] or
  // {name: {description, input | inputSchema, run}}. Functions stay here;
  // the shell gets the definitions.
  function splitTools(options) {
    const given = options && options.tools;
    if (!given) return { options: options || {}, fns: null };
    const list = Array.isArray(given) ? given : Object.keys(given).map((name) => ({ name, ...given[name] }));
    const fns = {};
    const tools = list.map((tool) => {
      if (!tool || typeof tool.name !== "string") throw new EvoError("bad_request", "every tool needs a name");
      if (typeof tool.run !== "function") throw new EvoError("bad_request", `tool \`${tool.name}\` needs a run(input) function`);
      fns[tool.name] = tool.run;
      return { name: tool.name, description: tool.description || "", inputSchema: tool.inputSchema || tool.input || { type: "object" } };
    });
    return { options: { ...options, tools }, fns };
  }
  async function aiToolCall(event) {
    const fns = aiTools.get(event.runId);
    if (!fns) {
      if (!earlyCalls.has(event.runId)) earlyCalls.set(event.runId, []);
      earlyCalls.get(event.runId).push(event);
      return;
    }
    let result;
    let isError = false;
    try {
      const fn = fns[event.tool];
      if (!fn) throw new Error(`this view has no tool \`${event.tool}\``);
      result = await fn(event.input || {});
      if (result === undefined) result = null;
    } catch (error) {
      result = String((error && error.message) || error);
      isError = true;
    }
    call("ai.toolResult", { runId: event.runId, callId: event.callId, result, isError }).catch(() => undefined);
  }
  function startRun(options) {
    let split;
    try {
      split = splitTools(options);
    } catch (error) {
      return () => Promise.reject(error);
    }
    return () =>
      call("ai.run", { options: split.options }).then((value) => {
        if (split.fns) {
          aiTools.set(value.runId, split.fns);
          (earlyCalls.get(value.runId) || []).forEach(aiToolCall);
          earlyCalls.delete(value.runId);
        }
        return value.runId;
      });
  }
  function aiEvent(runId, event) {
    const push = aiRuns.get(runId);
    if (push) push(event);
    else {
      if (!early.has(runId)) early.set(runId, []);
      early.get(runId).push(event);
    }
  }
  function aiRun(start, replay) {
    const queue = [];
    let wake = null;
    let ended = false;
    let settle;
    const result = new Promise((resolve, reject) => {
      settle = { resolve, reject };
    });
    result.catch(() => undefined);
    const push = (event) => {
      queue.push(event);
      if (event && event.type === "done") {
        ended = true;
        aiRuns.forEach((fn, id) => {
          if (fn === push) {
            aiRuns.delete(id);
            aiTools.delete(id);
          }
        });
        const r = event.result || {};
        if (r.stopped === "error") settle.reject(new EvoError("ai", r.error || "the AI run failed"));
        else settle.resolve(r);
      }
      if (wake) {
        const w = wake;
        wake = null;
        w();
      }
    };
    const id = start().then(
      (runId) => {
        if (!ended) aiRuns.set(runId, push);
        (replay || []).forEach(push);
        (early.get(runId) || []).forEach(push);
        early.delete(runId);
        return runId;
      },
      (error) => {
        ended = true;
        settle.reject(error);
        if (wake) wake();
        throw error;
      },
    );
    id.catch(() => undefined);
    const iterator = {
      async next() {
        for (;;) {
          if (queue.length) return { value: queue.shift(), done: false };
          if (ended) {
            await id.catch(() => undefined);
            return { value: undefined, done: true };
          }
          await new Promise((resolve) => (wake = resolve));
        }
      },
      [Symbol.asyncIterator]() {
        return this;
      },
    };
    return Object.freeze({
      id,
      result,
      cancel: () => id.then((runId) => call("ai.cancel", { runId })),
      next: () => iterator.next(),
      [Symbol.asyncIterator]: () => iterator,
    });
  }

  Object.defineProperty(window, "__evo_dispatch", {
    value: (event) => {
      if (!event || typeof event !== "object") return;
      if (event.kind === "chat.event") {
        const set = chatListeners.get(event.subscription);
        if (set) emit(set, event.event, event.chat);
      } else if (event.kind === "ai.event") {
        aiEvent(event.runId, event.event);
      } else if (event.kind === "ai.tool") {
        aiToolCall(event);
      } else if (event.kind === "theme") {
        theme = event.theme || theme;
        applyTheme();
        emit(themeListeners, theme);
      }
    },
  });

  const evo = {
    plugin: Object.freeze({ ...boot.plugin }),
    // The view's id, slot and title; a card's data and its height (FR-7, FR-10).
    view: Object.freeze({
      ...boot.view,
      data: () => dataLoaded.then(() => data),
      resize: (height) => call("view.resize", { height }),
    }),
    EvoError,
    chat: Object.freeze({
      active: () => call("chat.active").then((value) => value.chat),
      list: (options) => call("chat.list", options && options.archived ? { archived: true } : {}).then((value) => value.chats),
      messages: (chat, options) => call("chat.messages", { ...(chat ? { chat } : {}), ...(options && options.limit ? { limit: options.limit } : {}) }).then((value) => value.messages),
      onEvent: onChatEvent,
      send: (text, chat) => call("chat.send", { text, ...(chat ? { chat } : {}) }).then((value) => value.chat),
    }),
    tools: Object.freeze({
      call: (name, args) => call("tools.call", { name, args: args || {} }),
    }),
    ui: Object.freeze({
      notify: (body, options) => call("ui.notify", { body, ...(options || {}) }),
      open: (view) => call("ui.open", { view }),
    }),
    composer: Object.freeze({
      insert: (text) => call("composer.insert", { text }),
    }),
    theme: Object.freeze({
      tokens: () => ({ ...theme }),
      onChange: (fn) => {
        themeListeners.add(fn);
        return () => themeListeners.delete(fn);
      },
    }),
    // The person's AI models, directly (needs `ai`).
    ai: Object.freeze({
      models: () => call("ai.models"),
      run: (options) => aiRun(startRun(options)),
      complete: (options) => aiRun(startRun(options)).result,
      follow: (runId) => {
        let replay = [];
        return aiRun(() =>
          call("ai.follow", { runId }).then((value) => {
            if (!value.live) replay = (value.events || []).length ? value.events : [{ type: "done", result: value.result || {} }];
            return runId;
          }).then((id) => {
            replay.forEach((event) => aiEvent(id, event));
            return id;
          }),
        );
      },
      runs: () => call("ai.runs"),
      // Threads: a run with `thread` continues it (FR-18).
      threads: () => call("ai.threads"),
      thread: (thread) => call("ai.thread", { thread }),
      deleteThread: (thread) => call("ai.deleteThread", { thread }),
    }),
    // The plugin's own backend: one of its tools, started when it sleeps.
    backend: Object.freeze({
      call: (tool, args) => call("backend.call", { tool, args: args || {} }),
    }),
    // The plugin's accounts: states only; signIn opens the sign-in and
    // resolves when it ends ("signedIn" or "cancelled").
    auth: Object.freeze({
      status: () => call("auth.status").then((value) => value.accounts),
      signIn: (account) => call("auth.signIn", { account }).then((value) => value.state),
    }),
    // Optional permissions: "granted" | "denied" | "prompt"; request asks the
    // person once ("prompted") and keeps the answer.
    permissions: Object.freeze({
      query: (id) => call("permissions.query", { id }).then((value) => value.state),
      request: (id) => call("permissions.request", { id }).then((value) => value.state),
    }),
    storage: Object.freeze({
      get: (key) => call("storage.get", { key }),
      set: (key, value) => call("storage.set", { key, value: value === undefined ? null : value }),
      delete: (key) => call("storage.delete", { key }),
      list: () => call("storage.list"),
    }),
  };
  Object.defineProperty(window, "evo", { value: Object.freeze(evo), enumerable: true });
})();
