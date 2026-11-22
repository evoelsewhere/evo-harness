// The side panel's composer (spec FR-10, FR-26 – FR-29): Include this tab,
// the + menu (a screenshot of the page, files, another open tab) with chips,
// the approval mode switch, model and effort, the microphone, Send and Stop.

import { activePage } from "./active.js";
import { providerIcon } from "./providers.js";

const $ = (id) => document.getElementById(id);

// The app's approval menu (apps/desktop/src/features/chat/chat-composer.tsx).
const MODES = [
  { id: "ask", label: "Ask every time", hint: "Asks before edits and commands" },
  { id: "auto-edits", label: "Approve edits", hint: "Edits run, commands ask" },
  { id: "auto", label: "Approve for me", hint: "Everything runs" },
];

// The app's model menu (apps/desktop/src/components/ai-elements/model-selector.tsx).
const SHORT_LIST = 4;
const EFFORT_NAMES = { off: "Off", xhigh: "Extra high" };
export const effortName = (level) => EFFORT_NAMES[level] ?? String(level).charAt(0).toUpperCase() + String(level).slice(1);
const modelKey = (model) => `${model.provider}/${model.id}`;
const el = (tag, props = {}, ...children) => {
  const node = Object.assign(document.createElement(tag), props);
  node.append(...children);
  return node;
};
const CHEVRON = `<svg viewBox="0 0 16 16"><path d="M6.5 4.5l3.5 3.5-3.5 3.5"/></svg>`;
const BACK = `<svg viewBox="0 0 16 16"><path d="M9.5 4.5L6 8l3.5 3.5"/></svg>`;
const STAR = `<svg viewBox="0 0 16 16"><path d="M8 2.3l1.7 3.5 3.8.5-2.8 2.7.7 3.8L8 11l-3.4 1.8.7-3.8-2.8-2.7 3.8-.5z"/></svg>`;

export class Composer {
  constructor({ ask, view, onSend, onStop, ensureChat, onCancelQueued, onSendNow }) {
    this.ask = ask;
    this.view = view;
    this.onSend = onSend;
    this.onStop = onStop;
    this.onCancelQueued = onCancelQueued;
    this.onSendNow = onSendNow;
    this.ensureChat = ensureChat;
    /** What the model menu lists: the engine's catalog, the short list and the default. */
    this.catalog = { models: [], levels: [], pinned: [], default: null, loaded: false };
    /** The menu's page: the short list, Effort or More models. */
    this.modelPage = "main";
    this.includeTab = true;
    this.extraTab = null;
    this.attachments = [];
    this.running = false;
    this.input = $("input");
    this.input.addEventListener("keydown", (event) => {
      if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
        event.preventDefault();
        // While the agent works, Ctrl+Enter queues the message for after the run (as in the app).
        this.submit(this.running && (event.ctrlKey || event.metaKey));
      }
    });
    this.input.addEventListener("input", () => {
      this.grow();
      this.updateSend();
    });
    // While the agent works the button stops it, or sends what is typed as its next step.
    $("send").addEventListener("click", () => (this.running && !this.hasDraft() ? this.onStop() : this.submit()));
    $("plus").addEventListener("click", () => this.toggle("plus-list"));
    $("attach-shot").addEventListener("click", () => this.screenshot());
    $("attach-file").addEventListener("click", () => $("file").click());
    $("file").addEventListener("change", (event) => this.files(event.target.files));
    $("attach-tab").addEventListener("click", () => this.pickTab());
    $("mode").addEventListener("click", () => this.modes());
    $("model").addEventListener("click", () => this.models());
    // 1–4 pick from the short list, as the numbers beside them say (as in the app).
    $("model-list").addEventListener("keydown", (event) => {
      if (this.modelPage !== "main" || event.ctrlKey || event.metaKey || event.altKey) return;
      const index = Number(event.key) - 1;
      const list = this.shortList();
      if (!(index >= 0 && index < list.length)) return;
      event.preventDefault();
      this.choose(list[index]);
    });
    $("mic").addEventListener("click", () => this.dictate());
    document.addEventListener("paste", (event) => {
      if (event.clipboardData?.files?.length) {
        event.preventDefault();
        this.files(event.clipboardData.files);
      }
    });
    this.input.addEventListener("dragover", (event) => event.preventDefault());
    this.input.addEventListener("drop", (event) => {
      if (event.dataTransfer?.files?.length) {
        event.preventDefault();
        this.files(event.dataTransfer.files);
      }
    });
    this.renderChips();
  }

  focus() {
    this.input.focus();
  }

  /** Send a suggestion as if typed. */
  sendText(text) {
    this.input.value = text;
    this.grow();
    this.submit();
  }

  hasDraft() {
    return this.input.value.trim().length > 0 || this.attachments.length > 0;
  }

  updateSend() {
    const ready = this.hasDraft();
    const stop = this.running && !ready;
    $("send").classList.toggle("ready", ready);
    $("send").classList.toggle("stop", stop);
    $("send").disabled = !ready && !this.running;
    $("send").title = stop ? "Stop" : this.running ? "Send now (the agent reads it at its next step)" : "Send";
    $("send").setAttribute("aria-label", stop ? "Stop" : "Send");
    $("send").innerHTML = stop
      ? `<svg viewBox="0 0 16 16"><rect x="4.5" y="4.5" width="7" height="7" rx="1.5" fill="currentColor"/></svg>`
      : `<svg viewBox="0 0 16 16"><path d="M8 12.5v-9M4.2 7.2L8 3.5l3.8 3.7"/></svg>`;
  }

  grow() {
    this.input.style.height = "auto";
    this.input.style.height = `${Math.min(this.input.scrollHeight, 160)}px`;
  }

  toggle(id) {
    const list = $(id);
    const show = list.hidden;
    for (const other of ["menu-list", "plus-list", "model-list", "mode-list"]) $(other).hidden = true;
    list.hidden = !show;
  }

  setRunning(running) {
    if (this.running === running) return;
    this.running = running;
    // The app's placeholder while the agent works.
    this.input.placeholder = running ? "Enter to send now, Ctrl+Enter to queue for later" : "Ask Evo about this page…";
    this.updateSend();
  }

  /** Messages sent while the agent works and not read yet (queued-messages.tsx). */
  showQueued(items) {
    const list = $("queued");
    list.replaceChildren(
      ...items.map((item) => {
        const row = document.createElement("div");
        row.className = "queued-row";
        const label = document.createElement("span");
        label.className = `label${item.deliverAs === "steer" ? " steer" : ""}`;
        label.textContent = item.deliverAs === "steer" ? "Next step" : "Queued";
        const text = document.createElement("span");
        text.className = "text";
        const shown = item.text.replace(/(\n@[^\n]+)+$/, "").trim();
        text.textContent = shown || (item.images?.length ? "Picture" : "Attachment");
        text.title = item.text;
        row.append(label, text);
        const button = (title, svg, onClick) => {
          const b = document.createElement("button");
          b.className = "icon";
          b.title = title;
          b.setAttribute("aria-label", title);
          b.innerHTML = svg;
          b.addEventListener("click", onClick);
          return b;
        };
        if (item.deliverAs === "followUp") {
          row.append(button("Send now", `<svg viewBox="0 0 16 16"><path d="M2.5 8h10M9 4.5L12.5 8 9 11.5"/></svg>`, () => this.onSendNow(item)));
        }
        row.append(button("Remove", `<svg viewBox="0 0 16 16"><path d="M4.5 4.5l7 7M11.5 4.5l-7 7"/></svg>`, () => this.onCancelQueued(item)));
        return row;
      }),
    );
    list.hidden = items.length === 0;
  }

  showState(view) {
    const mode = MODES.find((m) => m.id === view.mode);
    $("mode-label").textContent = mode?.label || view.mode;
    $("mode").classList.toggle("auto", view.mode !== "ask");
    const known = (value) => value && value !== "none";
    const name = known(view.model?.provider) && known(view.model?.id) ? view.model.name || view.model.id : "";
    // The trigger reads "Model Effort", as the app's does; Effort only for a model that thinks.
    const levels = view.levels || this.catalog.levels;
    const showEffort = levels.length > 1 && (view.model?.reasoning ?? true);
    const effort = showEffort && view.effort && view.effort !== "off" ? effortName(view.effort) : "";
    $("model-label").textContent = name || (view.chat ? "No model" : "Model");
    $("model-effort").textContent = name ? effort : "";
    $("model-effort").hidden = !(name && effort);
    $("model").setAttribute("aria-label", `Model: ${name || "none"}${effort ? `, effort ${effort}` : ""}`);
    if (!$("model-list").hidden && this.modelPage !== "tabs") this.renderModels();
    if (!$("mode-list").hidden) this.renderModes();
  }

  // ── Approval mode (the app's menu) ────────────────────────────────────────

  modes() {
    this.toggle("mode-list");
    if (!$("mode-list").hidden) this.renderModes();
  }

  renderModes() {
    const items = MODES.map((mode) => {
      const button = el(
        "button",
        { className: "with-hint" },
        el("span", { className: "label" }, el("span", { textContent: mode.label }), el("span", { className: "hint", textContent: mode.hint })),
      );
      button.setAttribute("role", "menuitemradio");
      button.setAttribute("aria-checked", String(mode.id === this.view.mode));
      button.addEventListener("click", () => {
        $("mode-list").hidden = true;
        this.setMode(mode.id);
      });
      return button;
    });
    $("mode-list").replaceChildren(el("div", { className: "group eyebrow", textContent: "Approvals" }), ...items);
  }

  async setMode(mode) {
    const before = this.view.mode;
    this.view.mode = mode;
    this.showState(this.view);
    // The mode is the app's own: it needs no chat.
    const body = await this.ask({ op: "set_mode", chat: this.view.chat || undefined, mode }).catch((error) => {
      this.note(`The approval mode was not changed: ${error.message}`);
      return null;
    });
    this.view.mode = body?.mode || (body ? mode : before);
    this.showState(this.view);
  }

  /** The app or another panel changed the mode. */
  syncMode(mode) {
    if (!MODES.some((m) => m.id === mode)) return;
    this.view.mode = mode;
    this.showState(this.view);
  }

  // ── Model and effort (the app's menu) ─────────────────────────────────────

  async models() {
    this.toggle("model-list");
    const list = $("model-list");
    if (list.hidden) return;
    this.modelPage = "main";
    list.replaceChildren(el("div", { className: "group eyebrow", textContent: "Loading models…" }));
    try {
      // The welcome page has no chat yet: its chat is made now, as sending would.
      if (!this.view.chat) await this.ensureChat();
      const body = await this.ask({ op: "models", chat: this.view.chat }, 90_000);
      this.catalog = {
        models: body.models || [],
        levels: body.levels || [],
        pinned: body.pinned || [],
        default: body.default || null,
        loaded: true,
      };
      this.view.levels = this.catalog.levels;
      const current = this.current();
      if (current) this.view.model = { ...this.view.model, name: current.name || current.id, reasoning: current.reasoning };
      this.showState(this.view);
      if (!list.hidden) this.renderModels();
      list.focus();
    } catch (error) {
      list.replaceChildren(el("div", { className: "group", textContent: error.message }));
    }
  }

  current() {
    const { provider, id } = this.view.model || {};
    return this.catalog.models.find((m) => m.provider === provider && m.id === id);
  }

  /** The current model, pinned ones, then the current provider's models and any others, up to four. */
  shortList() {
    const { models, pinned } = this.catalog;
    const byKey = new Map(models.map((m) => [modelKey(m), m]));
    const current = this.current();
    const currentKey = current ? modelKey(current) : undefined;
    const keys = [];
    const add = (key) => {
      if (key && byKey.has(key) && !keys.includes(key) && keys.length < SHORT_LIST) keys.push(key);
    };
    if (currentKey && !pinned.includes(currentKey)) add(currentKey);
    pinned.forEach(add);
    models.filter((m) => m.provider === this.view.model?.provider).forEach((m) => add(modelKey(m)));
    models.forEach((m) => add(modelKey(m)));
    return keys.map((key) => byKey.get(key));
  }

  renderModels() {
    const list = $("model-list");
    const { models, levels } = this.catalog;
    if (!this.catalog.loaded) return;
    if (!models.length) {
      list.replaceChildren(el("div", { className: "group", textContent: "No models yet. Add a provider in Evo → Settings → Models." }));
      return;
    }
    const current = this.current();
    const currentKey = current ? modelKey(current) : undefined;
    const showEffort = levels.length > 1 && (current?.reasoning ?? true);
    const short = this.shortList();
    if (this.modelPage === "effort" && showEffort) return list.replaceChildren(...this.effortPage());
    if (this.modelPage === "more") return list.replaceChildren(...this.morePage(currentKey));
    this.modelPage = "main";

    const items = short.map((model, index) => {
      const key = modelKey(model);
      const active = key === currentKey;
      const row = el("button", { className: "model-item" }, providerIcon(model.provider), el("span", { className: "name", textContent: model.name || model.id }));
      if (key === this.catalog.default) row.append(el("span", { className: "badge", textContent: "Default" }));
      const tail = el("span", { className: "tail" });
      if (active) tail.innerHTML = `<svg viewBox="0 0 16 16"><path d="M3.5 8.5l3 3 6-7"/></svg>`;
      else tail.textContent = String(index + 1);
      row.append(tail);
      row.setAttribute("role", "menuitemradio");
      row.setAttribute("aria-checked", String(active));
      row.addEventListener("click", () => this.choose(model));
      return row;
    });
    if (showEffort || models.length > short.length) items.push(el("div", { className: "sep" }));
    if (showEffort) {
      const row = el("button", { className: "sub-trigger" }, el("span", { textContent: "Effort" }), el("span", { className: "value", textContent: effortName(this.view.effort ?? "default") }));
      row.insertAdjacentHTML("beforeend", CHEVRON);
      row.addEventListener("click", () => ((this.modelPage = "effort"), this.renderModels()));
      items.push(row);
    }
    if (models.length > short.length) {
      const row = el("button", { className: "sub-trigger" }, el("span", { textContent: "More models" }));
      row.insertAdjacentHTML("beforeend", CHEVRON);
      row.addEventListener("click", () => ((this.modelPage = "more"), this.renderModels()));
      items.push(row);
    }
    if (current && currentKey !== this.catalog.default) {
      items.push(el("div", { className: "sep" }));
      const row = el("button", {}, el("span", { className: "name", textContent: `Use ${current.name || current.id} for new chats` }));
      row.insertAdjacentHTML("afterbegin", STAR);
      row.addEventListener("click", () => this.setDefault(current));
      items.push(row);
    }
    list.replaceChildren(...items);
  }

  backRow(label) {
    const row = el("button", { className: "back" }, el("span", { textContent: label }));
    row.insertAdjacentHTML("afterbegin", BACK);
    row.addEventListener("click", () => ((this.modelPage = "main"), this.renderModels()));
    return row;
  }

  effortPage() {
    return [
      this.backRow("Effort"),
      ...this.catalog.levels.map((level) => {
        const row = el("button", {}, el("span", { className: "name", textContent: effortName(level) }));
        row.setAttribute("role", "menuitemradio");
        row.setAttribute("aria-checked", String(level === this.view.effort));
        row.addEventListener("click", () => this.setEffort(level));
        return row;
      }),
    ];
  }

  morePage(currentKey) {
    const groups = new Map();
    for (const model of this.catalog.models) groups.set(model.provider, [...(groups.get(model.provider) || []), model]);
    const items = [this.backRow("More models")];
    [...groups.entries()].forEach(([provider, models], index) => {
      if (index > 0) items.push(el("div", { className: "sep" }));
      items.push(el("div", { className: "group label" }, providerIcon(provider), el("span", { textContent: provider })));
      for (const model of models) {
        const row = el("button", {}, el("span", { className: "name", textContent: model.name || model.id }));
        row.setAttribute("role", "menuitemradio");
        row.setAttribute("aria-checked", String(modelKey(model) === currentKey));
        row.addEventListener("click", () => this.choose(model));
        items.push(row);
      }
    });
    return items;
  }

  /** Picks keep the menu open (to set the effort next, say), as in the app. */
  async choose(model) {
    const key = modelKey(model);
    const listed = this.shortList().map(modelKey);
    const pinned = listed.includes(key) ? listed : [key, ...listed].slice(0, SHORT_LIST);
    if (pinned.join("\n") !== this.catalog.pinned.join("\n")) {
      this.catalog.pinned = pinned;
      this.ask({ op: "set_pinned", pinned }).catch(() => {});
    }
    this.modelPage = "main";
    const current = this.current();
    if (current && modelKey(current) === key) return this.renderModels();
    // Shown at once; the engine's answer settles it.
    const before = { model: this.view.model, effort: this.view.effort };
    this.view.model = { provider: model.provider, id: model.id, name: model.name || model.id, reasoning: model.reasoning };
    this.showState(this.view);
    try {
      this.applyState(await this.ask({ op: "set_model", chat: this.view.chat, provider: model.provider, modelId: model.id }));
    } catch (error) {
      Object.assign(this.view, before);
      this.showState(this.view);
      this.note(`The model was not changed: ${error.message}`);
    }
  }

  async setEffort(level) {
    const before = this.view.effort;
    this.view.effort = level;
    this.showState(this.view);
    try {
      this.applyState(await this.ask({ op: "set_effort", chat: this.view.chat, level }));
    } catch (error) {
      this.view.effort = before;
      this.showState(this.view);
      this.note(`The effort was not changed: ${error.message}`);
    }
  }

  async setDefault(model) {
    try {
      const body = await this.ask({ op: "set_default", chat: this.view.chat, provider: model.provider, modelId: model.id });
      this.catalog.default = body.default || null;
      this.renderModels();
    } catch (error) {
      this.note(`The default model was not changed: ${error.message}`);
    }
  }

  /** A `chat` or `state` body: the chat's model, effort, levels and mode. */
  applyState(body) {
    if (!body || (body.chat && body.chat !== this.view.chat)) return;
    if (body.model) this.view.model = body.model;
    if ("effort" in body) this.view.effort = body.effort;
    if (Array.isArray(body.levels)) {
      this.view.levels = body.levels;
      if (body.levels.length) this.catalog.levels = body.levels;
    }
    if (body.mode) this.view.mode = body.mode;
    this.showState(this.view);
  }

  /** The app or another panel changed the short list. */
  syncPinned(pinned) {
    if (!Array.isArray(pinned)) return;
    this.catalog.pinned = pinned;
    if (!$("model-list").hidden && this.modelPage !== "tabs") this.renderModels();
  }

  // ── Attachments ───────────────────────────────────────────────────────────

  async screenshot() {
    $("plus-list").hidden = true;
    try {
      const win = await chrome.windows.getLastFocused();
      const url = await chrome.tabs.captureVisibleTab(win.id, { format: "png" });
      await this.attach("page.png", "image", url.split(",")[1], "image/png");
    } catch (error) {
      // The browser lets the extension see a page only after the person clicks its button there.
      this.note(
        /activeTab|all_urls/.test(error.message)
          ? "The browser lets Evo capture a page only right after you click the Evo button on it. Click the Evo button in the toolbar on this page, then try again."
          : `The screenshot was not taken: ${error.message}`,
      );
    }
  }

  async files(list) {
    $("plus-list").hidden = true;
    for (const file of list) {
      const data = await new Promise((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result).split(",")[1] || "");
        reader.onerror = () => reject(reader.error);
        reader.readAsDataURL(file);
      }).catch(() => null);
      if (data !== null) await this.attach(file.name || "file", file.type.startsWith("image/") ? "image" : "file", data, file.type);
    }
    $("file").value = "";
  }

  async attach(name, kind, data, mime = "") {
    try {
      const body = await this.ask({ op: "attach", chat: this.view.chat, name, kind, data });
      // A picture shows in the person's message as in the app; only its path goes to Evo.
      const dataUrl = kind === "image" ? `data:${mime || "image/png"};base64,${data}` : undefined;
      this.attachments.push({ name, kind, path: body.path, dataUrl });
      this.renderChips();
    } catch (error) {
      this.note(error.message);
    }
  }

  /** A plain radio row that closes the menu it is in. */
  item(label, checked, onClick) {
    const button = document.createElement("button");
    button.setAttribute("role", "menuitemradio");
    button.setAttribute("aria-checked", String(checked));
    button.textContent = label;
    button.addEventListener("click", () => {
      $("model-list").hidden = true;
      onClick();
    });
    return button;
  }

  async pickTab() {
    $("plus-list").hidden = true;
    const tabs = (await chrome.tabs.query({ windowType: "normal" })).filter((t) => /^https?:/.test(t.url || ""));
    const list = $("model-list");
    this.modelPage = "tabs";
    list.replaceChildren(
      Object.assign(document.createElement("div"), { className: "group eyebrow", textContent: "Add a tab" }),
      ...tabs.slice(0, 20).map((tab) =>
        this.item(tab.title || tab.url, this.extraTab?.id === tab.id, () => {
          this.extraTab = { id: tab.id, title: tab.title || tab.url };
          this.includeTab = false;
          this.renderChips();
        }),
      ),
    );
    list.hidden = false;
  }

  note(text) {
    $("messages").append(Object.assign(document.createElement("div"), { className: "notice", textContent: text }));
    $("messages").scrollTop = $("messages").scrollHeight;
  }

  async renderChips() {
    const chips = [];
    const [tab] = await activePage();
    if (this.extraTab) {
      chips.push(this.chip(`Tab · ${this.extraTab.title}`, true, () => ((this.extraTab = null), this.renderChips())));
    } else if (tab && /^https?:/.test(tab.url || "")) {
      const chip = this.chip(tab.title || tab.url, this.includeTab, null, tab.favIconUrl);
      chip.classList.add("toggle");
      chip.classList.toggle("off", !this.includeTab);
      chip.title = this.includeTab ? "The agent gets this tab; click to leave it out" : "Click to include this tab";
      chip.addEventListener("click", () => ((this.includeTab = !this.includeTab), this.renderChips()));
      chips.push(chip);
    }
    this.attachments.forEach((attachment, index) => {
      chips.push(this.chip(attachment.name, true, () => (this.attachments.splice(index, 1), this.renderChips())));
    });
    $("chips").replaceChildren(...chips);
    this.updateSend();
  }

  chip(label, on, onRemove, favicon) {
    const chip = document.createElement("span");
    chip.className = `chip${on ? "" : " off"}`;
    const dot = favicon && /^https?:|^data:/.test(favicon) ? Object.assign(document.createElement("img"), { src: favicon, alt: "" }) : document.createElement("i");
    if (dot.tagName === "I") dot.className = "dot";
    const text = document.createElement("span");
    text.textContent = label;
    chip.append(dot, text);
    if (onRemove) {
      const remove = document.createElement("button");
      remove.setAttribute("aria-label", `Remove ${label}`);
      remove.textContent = "×";
      remove.addEventListener("click", (event) => {
        event.stopPropagation();
        onRemove();
      });
      chip.append(remove);
    }
    return chip;
  }

  async submit(queue = false) {
    const text = this.input.value.trim();
    if (!text && this.attachments.length === 0) return;
    let tab = this.extraTab?.id;
    if (tab === undefined && this.includeTab) {
      const [active] = await activePage();
      if (active && /^https?:/.test(active.url || "")) tab = active.id;
    }
    const attachments = this.attachments;
    this.input.value = "";
    this.grow();
    this.attachments = [];
    this.extraTab = null;
    this.renderChips();
    await this.onSend({ text, tab, attachments, queue });
  }

  // ── Voice (FR-29) ─────────────────────────────────────────────────────────

  setVoice(on) {
    $("mic").hidden = !on || !("webkitSpeechRecognition" in self);
  }

  async dictate() {
    if (this.recognition) {
      this.recognition.stop();
      return;
    }
    // A side panel cannot show the microphone prompt: a page of the extension asks once.
    const permission = await navigator.permissions.query({ name: "microphone" }).catch(() => null);
    if (permission?.state !== "granted") {
      chrome.tabs.create({ url: chrome.runtime.getURL("permission/permission.html") });
      return;
    }
    const Recognition = self.webkitSpeechRecognition;
    const recognition = new Recognition();
    recognition.lang = navigator.language || "en-US";
    recognition.interimResults = true;
    recognition.continuous = true;
    const before = this.input.value;
    recognition.onresult = (event) => {
      let text = "";
      for (const result of event.results) text += result[0].transcript;
      this.input.value = before ? `${before} ${text}` : text;
      this.grow();
    };
    recognition.onend = () => {
      this.recognition = null;
      $("mic").classList.remove("listening");
    };
    this.recognition = recognition;
    $("mic").classList.add("listening");
    recognition.start();
  }
}
