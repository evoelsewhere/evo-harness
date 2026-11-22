// The side panel's thread: the chat's turns, streamed and read as in the app
// (docs/features/browser-bridge/spec.md, FR-10). The state comes from the
// app's own code, bundled in vendor/chat.js (transcript reducer, saved-chat
// rebuild, the words of turns and tool rows); this file draws it as DOM, the
// way apps/desktop/src/features/chat/chat-turn.tsx, user-message.tsx and
// components/ai-elements/{tool,reasoning}.tsx draw it. Keep them in step.
//
// Nothing from the chat becomes HTML: text goes in as text content, replies
// through markdown.js (plan 3.7).

import * as core from "./vendor/chat.js";
import { renderMarkdown } from "./markdown.js";

const svg = (body) => `<svg viewBox="0 0 16 16">${body}</svg>`;
// Icons by tool kind, as tool.tsx picks them (lucide shapes, redrawn at 16px).
const ICONS = {
  shell: svg(`<rect x="2" y="3" width="12" height="10" rx="1.5"/><path d="M4.8 6.5L6.8 8l-2 1.5M8.5 10h2.7"/>`),
  read: svg(`<path d="M9 2.5H4.5a1 1 0 0 0-1 1v9a1 1 0 0 0 1 1h7a1 1 0 0 0 1-1V6z"/><path d="M9 2.5V6h3.5"/>`),
  write: svg(`<path d="M9 2.5H4.5a1 1 0 0 0-1 1v9a1 1 0 0 0 1 1h3"/><path d="M9 2.5V6h3.5M12.5 8.5l1 1-3.5 3.5H9v-1z"/>`),
  search: svg(`<circle cx="7" cy="7" r="4.2"/><path d="M10.2 10.2L13.5 13.5"/>`),
  list: svg(`<path d="M2.5 4.5a1 1 0 0 1 1-1h3l1.2 1.5h4.8a1 1 0 0 1 1 1V8"/><circle cx="10" cy="10.5" r="2"/><path d="M11.5 12l1.5 1.5"/>`),
  web: svg(`<circle cx="8" cy="8" r="5.5"/><path d="M2.5 8h11M8 2.5c1.6 1.6 2.3 3.4 2.3 5.5S9.6 11.9 8 13.5M8 2.5C6.4 4.1 5.7 5.9 5.7 8s.7 3.9 2.3 5.5"/>`),
  browser: svg(`<rect x="2" y="3" width="12" height="10" rx="1.5"/><path d="M2 6h12M4 4.5h.01M5.5 4.5h.01"/>`),
  computer: svg(`<rect x="2" y="3" width="12" height="10" rx="1.5"/><path d="M2 6h12"/>`),
  agent: svg(`<rect x="3.5" y="5" width="9" height="7.5" rx="2"/><path d="M8 2.5V5M6.2 8.5h.01M9.8 8.5h.01M2 8.5v1.5M14 8.5v1.5"/>`),
  session: svg(`<path d="M2.5 4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v4a1 1 0 0 1-1 1H6l-2.5 2V9h0a1 1 0 0 1-1-1z"/><path d="M12.5 6.5h0a1 1 0 0 1 1 1V11a1 1 0 0 1-1 1v1.5L10.5 12H8"/>`),
  skill: svg(`<path d="M5 2.5h6.5a1 1 0 0 1 1 1V12a1.5 1.5 0 0 1-1.5 1.5H5"/><path d="M5 2.5A1.5 1.5 0 0 0 3.5 4v8.5A1 1 0 0 0 5 13.5M7 5.5h3.5M7 8h3.5"/>`),
  memory: svg(`<path d="M6 3a2 2 0 0 0-2 2 2 2 0 0 0-1 3.5A2 2 0 0 0 5 12a2 2 0 0 0 3 .5V3.5A2 2 0 0 0 6 3zM10 3a2 2 0 0 1 2 2 2 2 0 0 1 1 3.5 2 2 0 0 1-2 3.5 2 2 0 0 1-3 .5"/>`),
  ask: svg(`<path d="M8 13.5a5.5 5.5 0 1 0-4.8-2.8L2.5 13.5l2.8-.7A5.5 5.5 0 0 0 8 13.5z"/><path d="M6.6 6.5a1.5 1.5 0 1 1 2 1.4c-.4.2-.6.5-.6.9M8 10.5h.01"/>`),
  todo: svg(`<path d="M2.5 4.5l1 1 2-2M2.5 9.5l1 1 2-2M8 4.5h5.5M8 9.5h5.5M8 13h5.5"/>`),
  code: svg(`<path d="M5.5 2.5H5a1.5 1.5 0 0 0-1.5 1.5v2.5L2.5 8l1 1.5V12A1.5 1.5 0 0 0 5 13.5h.5M10.5 2.5h.5A1.5 1.5 0 0 1 12.5 4v2.5L13.5 8l-1 1.5V12a1.5 1.5 0 0 1-1.5 1.5h-.5"/>`),
  user: svg(`<path d="M4 11.5V7a4 4 0 0 1 8 0v4.5l1 1H3zM6.5 13.5a1.5 1.5 0 0 0 3 0"/>`),
  mcp: svg(`<path d="M6 2.5v3M10 2.5v3M4.5 5.5h7v2a3.5 3.5 0 0 1-7 0zM8 11v2.5"/>`),
  visual: svg(`<circle cx="5" cy="5" r="2.5"/><rect x="9" y="9" width="4.5" height="4.5" rx="1"/><path d="M11.2 2.5l2.3 4H9z"/>`),
  other: svg(`<path d="M10.5 2.8a3 3 0 0 0-3.7 3.9L2.7 10.8a1.3 1.3 0 0 0 1.9 1.9l4.1-4.1a3 3 0 0 0 3.9-3.7l-1.8 1.8-1.6-.4-.4-1.6z"/>`),
};
ICONS.edit = ICONS.write;
const BRAIN = ICONS.memory;
const CHEVRON = svg(`<path d="M6.5 4.5l3.5 3.5-3.5 3.5"/>`);
const COPY = svg(`<rect x="5.5" y="5.5" width="8" height="8" rx="1.5"/><path d="M10.5 5.5V4a1.5 1.5 0 0 0-1.5-1.5H4A1.5 1.5 0 0 0 2.5 4v5A1.5 1.5 0 0 0 4 10.5h1.5"/>`);
const CHECK = svg(`<path d="M3.5 8.5l3 3 6-7"/>`);
const ALERT = svg(`<circle cx="8" cy="8" r="5.5"/><path d="M8 5v3.5M8 10.8h.01"/>`);
const FILE = ICONS.read;
const NOTE = ICONS.session;

/** An element with text content only. */
function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined && text !== null) node.textContent = String(text);
  return node;
}

const objectArgs = (part) => (part.args && typeof part.args === "object" ? part.args : {});

/** Open a link in a new tab of this browser, never in the panel. */
function link(label, url, className = "") {
  const a = el("a", className, label);
  if (/^https?:\/\//.test(url)) {
    a.href = url;
    a.target = "_blank";
    a.rel = "noopener noreferrer";
  }
  a.title = url;
  return a;
}

export class Thread {
  /**
   * @param {HTMLElement} root where the turns go
   * @param {{ onRetry(text: string): void, onOpenInApp(): void, providerName(id?: string): string | undefined }} hooks
   */
  constructor(root, hooks) {
    this.root = root;
    this.hooks = hooks;
    this.state = core.initialTranscript;
    /** turn id → { turn, node }: a turn's node is rebuilt only when the reducer changed it. */
    this.nodes = new Map();
    /** Collapsibles the person opened or closed (`key` → open), and turns unfolded. */
    this.toggled = new Map();
    this.unfolded = new Set();
    this.frame = 0;
    this.onChange = () => {};
    // "Working for 12s" ticks.
    this.clock = setInterval(() => this.tick(), 1000);
  }

  get busy() {
    return Boolean(this.state.busy);
  }

  get queued() {
    return this.state.queued ?? [];
  }

  get empty() {
    return this.state.turns.length === 0;
  }

  /** A saved chat, rebuilt as the app rebuilds it (`transcriptFromMessages`). */
  load(messages, { running = false } = {}) {
    this.state = core.transcriptFromMessages(Array.isArray(messages) ? messages : [], { running });
    this.nodes.clear();
    this.toggled.clear();
    this.unfolded.clear();
    this.root.replaceChildren();
    this.render(true);
  }

  clear() {
    this.load([]);
  }

  dispatch(action) {
    this.state = core.transcriptReducer(this.state, action);
    this.schedule();
  }

  /** An engine line of the chat. */
  engine(message) {
    // A prompt sent from the app into a chat with no turn yet: the reducer
    // starts turns from prompts, and an echo with no turn changes nothing.
    if (message?.type === "message_start" && message.message?.role === "user" && this.state.turns.length === 0) {
      const text = core.typedPrompt(core.userText(message.message));
      if (!core.noteKind(text)) this.dispatch({ type: "prompt", text });
    }
    this.dispatch({ type: "engine", message });
  }

  schedule() {
    if (this.frame) return;
    const draw = () => {
      this.frame = 0;
      this.render();
    };
    // A hidden page gets no animation frames: draw on a timer, so it is up to date when shown.
    this.frame = document.hidden ? setTimeout(draw, 50) : requestAnimationFrame(draw);
  }

  render(force = false) {
    const list = this.root;
    const scroller = list.closest(".messages");
    const stick = force || nearBottom(scroller);
    const seen = new Set();
    let previous = null;
    for (const turn of this.state.turns) {
      seen.add(turn.id);
      let entry = this.nodes.get(turn.id);
      if (!entry || entry.turn !== turn) {
        const node = this.drawTurn(turn);
        if (entry) entry.node.replaceWith(node);
        entry = { turn, node };
        this.nodes.set(turn.id, entry);
      }
      const want = previous ? previous.nextSibling : list.firstChild;
      if (entry.node !== want) list.insertBefore(entry.node, want);
      previous = entry.node;
    }
    for (const [id, entry] of this.nodes) {
      if (!seen.has(id)) {
        entry.node.remove();
        this.nodes.delete(id);
      }
    }
    if (stick) scrollToEnd(scroller);
    this.onChange(this.state);
  }

  /** Redraw one turn (a collapsible's state changed). */
  redraw(turnId) {
    const entry = this.nodes.get(turnId);
    if (!entry) return;
    const node = this.drawTurn(entry.turn);
    entry.node.replaceWith(node);
    entry.node = node;
  }

  tick() {
    for (const node of this.root.querySelectorAll("[data-started]")) {
      const since = Number(node.dataset.started);
      const step = node.dataset.step ? ` · ${node.dataset.step}` : "";
      node.textContent = `Working for ${core.seconds(Date.now() - since)}${step}`;
    }
  }

  // ── A turn ─────────────────────────────────────────────────────────────

  drawTurn(turn) {
    const node = el("div", "turn");
    node.dataset.turn = turn.id;
    const kind = core.noteKind(turn.prompt);
    if (kind) node.append(this.drawNote(kind, turn.prompt));
    else if (turn.prompt || turn.images?.length) node.append(this.drawPrompt(turn));
    // A delivery with nothing under it yet shows only its line.
    if (turn.parts.length || turn.status !== "done" || turn.error) node.append(this.drawReply(turn));
    return node;
  }

  /** An automated message as an event line (chat-turn.tsx `BackgroundNote`). */
  drawNote(kind, text) {
    const line = el("div", "note-line");
    line.insertAdjacentHTML("beforeend", NOTE);
    line.prepend(el("span", "rule"));
    line.append(el("span", "what", core.noteLabel(kind, text.trim())), el("span", "rule"));
    return line;
  }

  /** The person's message (user-message.tsx `UserPrompt`): pictures, text, pasted blocks, files. */
  drawPrompt(turn) {
    const annotated = core.parseAnnotatedMessage(turn.prompt);
    const { text, pasted, files } = core.splitPrompt(annotated?.text ?? turn.prompt);
    const node = el("div", "msg user");
    if (turn.images?.length) {
      const pictures = el("div", "pictures");
      for (const [index, src] of turn.images.entries()) {
        if (!/^data:image\//.test(src)) continue;
        const img = el("img");
        img.src = src;
        img.alt = `Picture ${index + 1}`;
        pictures.append(img);
      }
      node.append(pictures);
    }
    if (text) node.append(el("div", "bubble", text));
    for (const block of pasted) {
      const chip = el("details", "pasted");
      chip.append(el("summary", "", `Pasted text · ${block.lines} lines`), el("pre", "", block.text));
      node.append(chip);
    }
    if (files.length) {
      const row = el("div", "files");
      for (const path of files) {
        const chip = el("span", "file-chip");
        chip.title = path;
        chip.insertAdjacentHTML("beforeend", FILE);
        chip.append(el("span", "", String(path).split(/[\\/]/).pop()));
        row.append(chip);
      }
      node.append(row);
    }
    if (annotated?.annotations?.length) node.append(el("div", "files", `${annotated.annotations.length} comment${annotated.annotations.length > 1 ? "s" : ""} on the document`));
    const copy = [text, ...pasted.map((block) => block.text)].filter(Boolean).join("\n\n");
    if (copy) {
      const footer = el("div", "msg-footer");
      footer.append(copyButton(copy));
      node.append(footer);
    }
    return node;
  }

  /** The agent's part of a turn (chat-turn.tsx `AssistantMessage`). */
  drawReply(turn) {
    const node = el("div", "msg assistant");
    const content = el("div", "reply");
    node.append(content);
    const running = turn.status === "running";
    const all = core.segments(turn.parts);
    const hasProcess = all.some((segment) => segment.kind === "process");
    // A completed turn folds its process; failed or stopped turns stay open.
    const foldable = turn.status === "done" && hasProcess;
    const expanded = this.unfolded.has(turn.id);
    const shown =
      foldable && !expanded
        ? all.filter(
            (segment) =>
              segment.kind === "text" ||
              segment.kind === "widget" ||
              segment.parts.some((part) => core.runningInBackground(part) || (part.kind === "tool" && part.name === "start_session" && part.phase === "done")),
          )
        : all;

    if (running) {
      const status = el("div", "run-status");
      status.setAttribute("role", "status");
      status.append(el("span", "spinner"));
      const words = el("span", "shimmer");
      words.dataset.started = String(turn.startedAt);
      const step = core.runningStep(turn.parts);
      if (step) words.dataset.step = step;
      words.textContent = `Working for ${core.seconds(Date.now() - turn.startedAt)}${step ? ` · ${step}` : ""}`;
      status.append(words);
      content.append(status);
    }
    if (foldable) {
      const fold = el("button", "fold", `Completed in ${core.seconds(turn.meta.durationMs ?? (turn.endedAt ?? turn.startedAt) - turn.startedAt)}`);
      fold.insertAdjacentHTML("beforeend", CHEVRON);
      fold.setAttribute("aria-expanded", String(expanded));
      fold.addEventListener("click", () => {
        if (expanded) this.unfolded.delete(turn.id);
        else this.unfolded.add(turn.id);
        this.redraw(turn.id);
      });
      content.append(fold);
    }
    shown.forEach((segment) => {
      if (segment.kind === "text") {
        const body = el("div", `markdown${segment.streaming ? " streaming" : ""}`);
        body.append(renderMarkdown(segment.text));
        content.append(body);
      } else if (segment.kind === "widget") {
        const trace = el("div", "trace");
        trace.append(this.drawTool(segment.part, running, turn.id));
        content.append(trace);
      } else {
        const index = all.indexOf(segment);
        content.append(this.drawGroup(segment, running, running && index === all.length - 1, turn.id));
      }
    });
    // Files the agent sent stay in sight when the process folds.
    if (foldable && !expanded) {
      for (const part of turn.parts) {
        const sent = sentFiles(part);
        if (sent) content.append(drawSentFiles(sent));
      }
    }
    if (turn.error) content.append(this.drawError(turn));
    if (turn.status === "aborted") content.append(el("p", "stopped", "Stopped."));
    if (!running) {
      const footer = el("div", "msg-footer");
      const reply = core.turnReply(turn);
      if (reply) footer.append(copyButton(core.replyMarkdown(reply)));
      const meta = core.turnMetaLine(turn.meta);
      if (meta) footer.append(el("span", "meta", meta));
      if (footer.childNodes.length) content.append(footer);
    }
    return node;
  }

  /** A failed turn (turn-error.tsx): one sentence, what to do, and Details. */
  drawError(turn) {
    const explained = core.explainError(turn.error, this.hooks.providerName(turn.meta.provider));
    const box = el("div", "turn-error");
    box.setAttribute("role", "alert");
    const head = el("div", "head");
    head.insertAdjacentHTML("beforeend", ALERT);
    head.append(el("span", "", explained.title));
    box.append(head);
    const actions = el("div", "actions");
    if (explained.kind === "auth" || explained.kind === "context") {
      const open = el("button", "small", explained.kind === "auth" ? "Open Providers in Evo" : "Compact in Evo");
      open.addEventListener("click", () => this.hooks.onOpenInApp());
      actions.append(open);
    } else if (explained.kind !== "other" && turn.prompt) {
      const retry = el("button", "small", "Retry");
      retry.addEventListener("click", () => this.hooks.onRetry(turn.prompt));
      actions.append(retry);
    }
    if (explained.detail.trim() !== explained.title.trim()) {
      const details = el("details", "error-details");
      details.append(el("summary", "", "Details"), el("pre", "", explained.detail));
      actions.append(details);
    }
    if (actions.childNodes.length) box.append(actions);
    return box;
  }

  // ── Process: reasoning and tool rows (Trace) ───────────────────────────

  /** A process group: its rows, under a title once there is more than one. */
  drawGroup(segment, running, turnLive, turnId) {
    const background = segment.parts.some(core.runningInBackground);
    const live = turnLive || background;
    const rows = el("div", "trace");
    if (segment.parts.length === 1) {
      rows.append(this.drawPart(segment.parts[0], running, turnId));
      return rows;
    }
    const line = traceLine(core.groupTitle(segment.parts, live), live ? "running" : undefined);
    return this.collapsible(`group:${segment.key}`, live, line, () => {
      for (const part of segment.parts) rows.append(this.drawPart(part, running, turnId));
      return rows;
    }, turnId, "group-body");
  }

  drawPart(part, running, turnId) {
    return part.kind === "reasoning" ? this.drawReasoning(part, turnId) : this.drawTool(part, running, turnId);
  }

  /** reasoning.tsx: collapsed, a one-line preview, "Thinking" then "Thought for Ns". */
  drawReasoning(part, turnId) {
    const line = traceLine(core.reasoningTitle(part), part.streaming ? "running" : undefined, BRAIN);
    if (part.text) line.append(el("span", "trace-desc preview", core.preview(part.text, part.streaming)));
    if (!part.text) return wrapRow(line);
    return this.collapsible(`reasoning:${part.key}`, false, line, () => el("div", "reasoning-text", part.text), turnId);
  }

  /** tool.tsx `Tool`: the line, then the call's body when it has one. */
  drawTool(part, streaming, turnId) {
    const header = core.toolHeader(part, streaming);
    const { kind, phase, live, failed, stopped, child } = header;
    const status = live ? "running" : failed ? "error" : undefined;
    const title = header.fileVerb && header.about ? `${header.fileVerb} ${header.about}` : header.label;
    const line = traceLine(title, status, ICONS[kind] || ICONS.other, kind === "shell" ? "mono" : "");
    if (header.fileVerb && header.about) line.querySelector(".trace-title").title = header.about;
    if (stopped && header.fileVerb) line.append(el("span", "trace-desc", "stopped"));
    if (child?.background) line.append(el("span", "trace-desc", "background"));
    if (header.diff && (header.diff.added || header.diff.removed)) {
      const counts = el("span", "trace-desc mono");
      counts.append(el("span", "added", `+${header.diff.added}`), " ", el("span", "removed", `-${header.diff.removed}`));
      line.append(counts);
    }

    // A widget is drawn in the app; the panel says where to see it.
    if (part.name === "show_widget" && phase !== "error") {
      const box = el("div", "widget-box");
      box.append(wrapRow(line), el("div", "widget-note", phase === "preparing" ? core.widgetLoading(part) || "Building the widget…" : "This widget shows in Evo."));
      const open = el("button", "small", "Open in Evo");
      open.addEventListener("click", () => this.hooks.onOpenInApp());
      if (phase !== "preparing") box.append(open);
      return box;
    }
    if (phase === "preparing") return wrapRow(line);
    const sent = sentFiles(part);
    if (sent) {
      const box = el("div", "tool-box");
      box.append(wrapRow(line), drawSentFiles(sent));
      return box;
    }
    if (kind === "agent") {
      return this.collapsible(`tool:${part.key}`, live || failed, line, () => drawAgent(part, child), turnId);
    }
    if (phase === "running" && !part.output) return wrapRow(line);
    return this.collapsible(`tool:${part.key}`, failed, line, () => toolBody(part, header), turnId);
  }

  /**
   * A row that opens: `defaultOpen` until the person toggles it; its body is
   * built only when shown (a long run's outputs stay out of the page).
   */
  collapsible(key, defaultOpen, line, body, turnId, bodyClass = "") {
    const open = this.toggled.has(key) ? this.toggled.get(key) : defaultOpen;
    const box = el("div", "collapsible");
    const trigger = el("button", "trigger");
    trigger.append(line);
    trigger.insertAdjacentHTML("beforeend", `<span class="chev">${CHEVRON}</span>`);
    trigger.setAttribute("aria-expanded", String(open));
    box.append(trigger);
    if (open) {
      const content = el("div", `trace-body ${bodyClass}`.trim());
      content.append(body());
      box.append(content);
    }
    trigger.addEventListener("click", () => {
      this.toggled.set(key, !open);
      this.redraw(turnId);
    });
    return box;
  }
}

// ── Pieces ────────────────────────────────────────────────────────────────

function traceLine(title, status, icon, titleClass = "") {
  const line = el("span", "trace-line");
  if (status) line.dataset.status = status;
  if (icon) {
    const holder = el("span", "trace-icon");
    holder.innerHTML = icon;
    line.append(holder);
  }
  line.append(el("span", `trace-title ${titleClass}`.trim(), title));
  return line;
}

function wrapRow(line) {
  const row = el("div", "trace-row");
  row.append(line);
  return row;
}

function copyButton(text) {
  const button = el("button", "icon-action");
  button.title = "Copy";
  button.setAttribute("aria-label", "Copy");
  button.innerHTML = COPY;
  button.addEventListener("click", async () => {
    await navigator.clipboard.writeText(text).catch(() => {});
    button.innerHTML = CHECK;
    button.title = "Copied";
    setTimeout(() => {
      button.innerHTML = COPY;
      button.title = "Copy";
    }, 2000);
  });
  return button;
}

function code(text, { failed = false, title, wrap = true } = {}) {
  const block = el("div", `code-block${failed ? " failed" : ""}`);
  if (title) block.append(el("div", "code-title", title));
  block.append(el("pre", wrap ? "wrap" : "", text || "(no output)"));
  return block;
}

/** A shell call: the command and its output, as a terminal. */
function terminal(command, output, failed) {
  const block = el("div", `code-block${failed ? " failed" : ""}`);
  const pre = el("pre", "wrap");
  if (command) {
    pre.append(el("span", "prompt", "$ "), command, "\n");
  }
  pre.append(output || (failed ? "" : "(no output)"));
  block.append(pre);
  return block;
}

function diffBlock(diff, path) {
  const block = el("div", "code-block");
  if (path) block.append(el("div", "code-title", path));
  const pre = el("pre", "diff");
  for (const line of diff.lines) pre.append(el("span", `line ${line.variant}`, line.text || " "));
  block.append(pre);
  return block;
}

function sentFiles(part) {
  if (part.kind !== "tool" || part.name !== "send_file") return null;
  const result = part.details?.externalToolResult;
  const sent = result && !result.isError ? result.structuredContent : undefined;
  return sent && Array.isArray(sent.files) && sent.files.length ? sent : null;
}

function drawSentFiles(sent) {
  const box = el("div", "sent-files");
  if (sent.caption) box.append(el("div", "caption", sent.caption));
  for (const file of sent.files) {
    const row = el("div", "file-chip");
    row.title = file.path || "";
    row.insertAdjacentHTML("beforeend", FILE);
    row.append(el("span", "", file.name || String(file.path || "").split(/[\\/]/).pop()));
    box.append(row);
  }
  box.append(el("div", "hint", "Open them from Evo."));
  return box;
}

/** A subagent's run (agent-run.tsx, in short): its steps and its report. */
function drawAgent(part, child) {
  const box = el("div", "agent-run");
  const facts = [child?.agentType, child?.model, child?.toolUses ? `${child.toolUses} tool uses` : "", child?.status].filter(Boolean).join(" · ");
  if (facts) box.append(el("div", "facts", facts));
  for (const tool of child?.tools ?? []) {
    const step = core.toolHeader({ kind: "tool", key: tool.callId, callId: tool.callId, name: tool.name, args: tool.args ?? {}, argsText: "", phase: tool.phase, output: "", startedAt: 0 }, child.status === "running");
    box.append(traceLine(step.label, step.live ? "running" : step.failed ? "error" : undefined, ICONS[step.kind] || ICONS.other));
  }
  const report = child?.report || (child?.status === "running" ? child?.text : "") || part.output;
  if (report) {
    const body = el("div", "markdown");
    body.append(renderMarkdown(report));
    box.append(body);
  }
  if (child?.error) box.append(el("div", "turn-error-line", child.error));
  return box;
}

/** The body of a finished call, chosen as tool.tsx chooses it. */
function toolBody(part, header) {
  const { kind, failed, about } = header;
  const args = objectArgs(part);
  if (kind === "browser" || kind === "computer") {
    const box = el("div", "stack");
    for (const src of part.images ?? []) {
      if (!/^data:image\//.test(src)) continue;
      const img = el("img", "shot");
      img.src = src;
      img.alt = kind === "computer" ? "App screenshot" : "Browser screenshot";
      box.append(img);
    }
    if (part.output) box.append(code(part.output, { failed }));
    return box;
  }
  if (kind === "todo" && !failed && Array.isArray(args.todos)) {
    const list = el("ul", "todos");
    for (const todo of args.todos) {
      const item = el("li", `todo ${todo?.status || "pending"}`, todo?.content ?? todo?.activeForm ?? "");
      list.append(item);
    }
    return list;
  }
  if (kind === "ask" && !failed) return code(part.output, { title: "Answers" });
  if (part.name === "web_search" && !failed && Array.isArray(part.details?.results)) {
    const results = part.details.results;
    if (!results.length) return el("p", "muted", "No results.");
    const list = el("ol", "web-results");
    for (const result of results) {
      let host = result.url;
      try {
        host = new URL(result.url).hostname.replace(/^www\./, "");
      } catch {
        // Keep the address.
      }
      const item = el("li");
      const top = el("span", "top");
      top.append(link(result.title || host, result.url, "title"), el("span", "host", host));
      item.append(top);
      if (result.snippet) item.append(el("span", "snippet", result.snippet));
      list.append(item);
    }
    return list;
  }
  if (part.name === "web_fetch" && !failed) {
    return code(core.webPageBody(part.output), { title: part.details?.title ? String(part.details.title) : String(part.details?.finalUrl ?? "Page") });
  }
  if (kind === "shell") return terminal(core.subject(part), part.output, failed);
  if (header.diff && !failed) return diffBlock(header.diff, about);
  if (kind === "read" && !failed) return code(part.output, { title: about });
  if (kind === "skill" && !failed) return code(core.skillBody(part.output), { title: "Instructions" });
  if (kind === "memory" && !failed && args.action === "save") return code(String(args.content ?? ""), { title: String(args.description ?? "Memory") });
  if (kind === "memory" && !failed && args.action === "update" && part.details?.diff) {
    const removed = Array.isArray(part.details.diff.removed) ? part.details.diff.removed : [];
    const added = Array.isArray(part.details.diff.added) ? part.details.diff.added : [];
    if (!removed.length && !added.length) return el("p", "muted", "No line changed.");
    return diffBlock({ lines: [...removed.map((text) => ({ variant: "removed", text })), ...added.map((text) => ({ variant: "added", text }))] });
  }
  if (kind === "other" || kind === "mcp") {
    const box = el("div", "stack");
    box.append(
      code(JSON.stringify((part.name === "mcp_call_tool" ? args.arguments : part.args) ?? {}, null, 2), { title: "Input" }),
      code(part.output, { failed, title: failed ? "Error" : "Output" }),
    );
    return box;
  }
  return code(part.output, { failed });
}

function nearBottom(list) {
  return !list || list.scrollHeight - list.scrollTop - list.clientHeight < 80;
}

function scrollToEnd(list) {
  if (list) list.scrollTop = list.scrollHeight;
}
