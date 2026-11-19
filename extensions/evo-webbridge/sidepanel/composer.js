// The side panel's composer (spec FR-10, FR-26 – FR-29): Include this tab,
// the + menu (a screenshot of the page, files, another open tab) with chips,
// the approval mode switch, model and effort, the microphone, Send and Stop.

import { activePage } from "./active.js";

const $ = (id) => document.getElementById(id);

const MODES = [
  { id: "ask", label: "Ask every time" },
  { id: "auto-edits", label: "Approve edits" },
  { id: "auto", label: "Approve for me" },
];

export class Composer {
  constructor({ ask, view, onSend, onStop }) {
    this.ask = ask;
    this.view = view;
    this.onSend = onSend;
    this.onStop = onStop;
    this.includeTab = true;
    this.extraTab = null;
    this.attachments = [];
    this.running = false;
    this.input = $("input");
    this.input.addEventListener("keydown", (event) => {
      if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
        event.preventDefault();
        this.submit();
      }
    });
    this.input.addEventListener("input", () => {
      this.grow();
      this.updateSend();
    });
    $("send").addEventListener("click", () => (this.running ? this.onStop() : this.submit()));
    $("plus").addEventListener("click", () => this.toggle("plus-list"));
    $("attach-shot").addEventListener("click", () => this.screenshot());
    $("attach-file").addEventListener("click", () => $("file").click());
    $("file").addEventListener("change", (event) => this.files(event.target.files));
    $("attach-tab").addEventListener("click", () => this.pickTab());
    $("mode").addEventListener("click", () => this.nextMode());
    $("model").addEventListener("click", () => this.models());
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

  updateSend() {
    const ready = this.input.value.trim().length > 0 || this.attachments.length > 0;
    $("send").classList.toggle("ready", ready && !this.running);
    $("send").disabled = !ready && !this.running;
  }

  grow() {
    this.input.style.height = "auto";
    this.input.style.height = `${Math.min(this.input.scrollHeight, 160)}px`;
  }

  toggle(id) {
    const list = $(id);
    const show = list.hidden;
    for (const other of ["menu-list", "plus-list", "model-list"]) $(other).hidden = true;
    list.hidden = !show;
  }

  setRunning(running) {
    this.running = running;
    $("send").classList.toggle("stop", running);
    $("send").title = running ? "Stop" : "Send";
    $("send").setAttribute("aria-label", running ? "Stop" : "Send");
    $("send").innerHTML = running
      ? `<svg viewBox="0 0 16 16"><rect x="4.5" y="4.5" width="7" height="7" rx="1.5" fill="currentColor"/></svg>`
      : `<svg viewBox="0 0 16 16"><path d="M8 12.5v-9M4.2 7.2L8 3.5l3.8 3.7"/></svg>`;
    this.updateSend();
  }

  showState(view) {
    $("mode-label").textContent = MODES.find((m) => m.id === view.mode)?.label || view.mode;
    $("mode").classList.toggle("auto", view.mode !== "ask");
    const known = (value) => value && value !== "none";
    const name = known(view.model?.provider) && known(view.model?.id) ? view.model.name || view.model.id : "";
    $("model-label").textContent = !name ? "Choose a model" : view.effort ? `${name} · ${view.effort}` : name;
  }

  async nextMode() {
    if (!this.view.chat) return;
    const index = MODES.findIndex((m) => m.id === this.view.mode);
    const next = MODES[(index + 1) % MODES.length].id;
    const body = await this.ask({ op: "set_mode", chat: this.view.chat, mode: next }).catch(() => null);
    if (body) {
      this.view.mode = body.mode;
      this.showState(this.view);
    }
  }

  async models() {
    if (!this.view.chat) return;
    this.toggle("model-list");
    const list = $("model-list");
    if (list.hidden) return;
    list.replaceChildren(Object.assign(document.createElement("div"), { className: "group eyebrow", textContent: "Loading…" }));
    const body = await this.ask({ op: "models", chat: this.view.chat }).catch((error) => ({ error }));
    if (body.error) {
      list.replaceChildren(Object.assign(document.createElement("div"), { className: "group", textContent: body.error.message }));
      return;
    }
    const items = [];
    if ((body.levels || []).length) {
      items.push(Object.assign(document.createElement("div"), { className: "group eyebrow", textContent: "Effort" }));
      for (const level of body.levels) items.push(this.item(level, level === this.view.effort, () => this.setEffort(level)));
    }
    items.push(Object.assign(document.createElement("div"), { className: "group eyebrow", textContent: "Model" }));
    for (const model of body.models || []) {
      const current = model.id === this.view.model?.id && model.provider === this.view.model?.provider;
      items.push(this.item(`${model.name || model.id} · ${model.provider}`, current, () => this.setModel(model)));
    }
    list.replaceChildren(...items);
  }

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

  async setModel(model) {
    const body = await this.ask({ op: "set_model", chat: this.view.chat, provider: model.provider, modelId: model.id }).catch(() => null);
    if (body) Object.assign(this.view, { model: body.model, effort: body.effort }), this.showState(this.view);
  }

  async setEffort(level) {
    const body = await this.ask({ op: "set_effort", chat: this.view.chat, level }).catch(() => null);
    if (body) Object.assign(this.view, { effort: body.effort }), this.showState(this.view);
  }

  // ── Attachments ───────────────────────────────────────────────────────────

  async screenshot() {
    $("plus-list").hidden = true;
    try {
      const win = await chrome.windows.getLastFocused();
      const url = await chrome.tabs.captureVisibleTab(win.id, { format: "png" });
      await this.attach("page.png", "image", url.split(",")[1]);
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
      if (data !== null) await this.attach(file.name || "file", file.type.startsWith("image/") ? "image" : "file", data);
    }
    $("file").value = "";
  }

  async attach(name, kind, data) {
    try {
      const body = await this.ask({ op: "attach", chat: this.view.chat, name, kind, data });
      this.attachments.push({ name, kind, path: body.path });
      this.renderChips();
    } catch (error) {
      this.note(error.message);
    }
  }

  async pickTab() {
    $("plus-list").hidden = true;
    const tabs = (await chrome.tabs.query({ windowType: "normal" })).filter((t) => /^https?:/.test(t.url || ""));
    const list = $("model-list");
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

  async submit() {
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
    await this.onSend({ text, tab, attachments });
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
