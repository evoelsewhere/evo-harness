// Injected into every page of the in-app browser (src/browser.rs) before its
// own scripts. It keeps the page's console and errors, and gives the agent a
// readable outline of the page with stable element refs (`ref_N`) to act on.
// The page cannot reach the app: these pages have no IPC.
//
// Agent browser (docs/features/agent-browser): refs inside a same-origin
// iframe are frame-qualified (`f1:ref_3`; frame ids come from `frames()`), and
// every function that takes a ref accepts them. Hover, drag and wheel are
// script-driven fallbacks for when DevTools input is not available. Cross-
// origin frames cannot be reached from here. Dialogs are held by the shell on
// Windows; where it cannot (`window.__evo_noShellDialogs`, set by the shell
// before this script) alert / confirm / prompt answer by a preset policy.
(() => {
  if (window.__evo) return;
  const logs = [];
  let logSeq = 0;
  const text = (value) => {
    try {
      if (typeof value === "string") return value;
      if (value instanceof Error) return value.stack || value.message;
      return JSON.stringify(value);
    } catch {
      return String(value);
    }
  };
  const push = (level, args) => {
    logs.push({ seq: ++logSeq, level, at: Date.now(), text: args.map(text).join(" ").slice(0, 4000) });
    if (logs.length > 500) logs.shift();
  };
  for (const level of ["log", "info", "warn", "error", "debug"]) {
    const original = console[level];
    console[level] = function (...args) {
      try {
        push(level, args);
      } catch {}
      return original.apply(this, args);
    };
  }
  addEventListener("error", (event) => {
    // Resource load failures (img, script) reach here too, without a message.
    if (!event.message && event.target && event.target !== window) {
      const target = event.target;
      push("error", [`Failed to load ${target.tagName?.toLowerCase() || "resource"} ${target.src || target.href || ""}`.trim()]);
      return;
    }
    push("error", [`${event.message}${event.filename ? ` (${event.filename}:${event.lineno})` : ""}`]);
  }, true);
  addEventListener("unhandledrejection", (event) => {
    push("error", [`Unhandled rejection: ${text(event.reason?.stack || event.reason)}`]);
  });

  // Dialog fallback: only where the shell cannot hold dialogs open. The log
  // keeps what the page asked and what it was answered.
  const dialogs = [];
  let dialogAnswer = null; // { accept, text } or null for the defaults
  if (window.__evo_noShellDialogs) {
    const record = (kind, message, answer) => {
      dialogs.push({ kind, message: String(message ?? ""), at: Date.now(), answer });
      if (dialogs.length > 100) dialogs.shift();
      return answer;
    };
    window.alert = function alert(message) {
      record("alert", message, null);
    };
    window.confirm = function confirm(message) {
      return record("confirm", message, dialogAnswer ? !!dialogAnswer.accept : false);
    };
    window.prompt = function prompt(message, defaultValue) {
      const answer = dialogAnswer && dialogAnswer.accept ? String(dialogAnswer.text ?? defaultValue ?? "") : null;
      return record("prompt", message, answer);
    };
  } else {
    // The app's dialog plugin replaces alert and confirm in every webview with
    // versions that return a promise: a confirm then reads as true, the page is
    // never held and the shell never sees a dialog. The real functions only
    // exist in the isolated world the shell registers (`evo-dialogs`, see
    // browser/hooks.rs), which shares the DOM but not the page's globals. The
    // replacements below call into it through a synchronous DOM event, so the
    // page blocks inside the real dialog and the shell holds it open for the
    // agent (ScriptDialogOpening).
    const isPlugin = (fn) => {
      try {
        return /plugin:dialog/.test(Function.prototype.toString.call(fn));
      } catch {
        return false;
      }
    };
    const viaIsolatedWorld = (kind, message) => {
      const slot = document.createElement("evo-dialog");
      slot.setAttribute("data-kind", kind);
      slot.setAttribute("data-message", String(message ?? ""));
      document.dispatchEvent(new MouseEvent("evo-dialog", { relatedTarget: slot }));
      return slot.getAttribute("data-result");
    };
    const restore = () => {
      if (isPlugin(window.alert)) {
        window.alert = function alert(message) {
          viaIsolatedWorld("alert", message);
        };
      }
      if (isPlugin(window.confirm)) {
        window.confirm = function confirm(message) {
          return viaIsolatedWorld("confirm", message) === "true";
        };
      }
    };
    restore();
    // The plugin's script may run after this one.
    addEventListener("DOMContentLoaded", restore);
    addEventListener("load", restore);
  }

  let seq = 0;
  let frameSeq = 0;
  const refs = new Map();
  /** The window an element lives in (its own frame's, not necessarily the top page's). */
  const winOf = (el) => el.ownerDocument?.defaultView || window;
  /** A stable id for an iframe element: f1, f2, … in order of discovery. */
  const frameId = (frameEl) => frameEl.__evoFrame || (frameEl.__evoFrame = `f${++frameSeq}`);
  /** The frame prefix for refs of elements in `doc` ("" for the top page). */
  const framePrefix = (doc) => {
    if (!doc || doc === document) return "";
    const frameEl = doc.defaultView?.frameElement;
    return frameEl ? `${frameId(frameEl)}:` : "";
  };
  const refOf = (el) => {
    if (!el.__evoRef) {
      el.__evoRef = `${framePrefix(el.ownerDocument)}ref_${++seq}`;
      refs.set(el.__evoRef, new WeakRef(el));
    }
    return el.__evoRef;
  };
  /** Whether the element, and every frame around it, is still on the page. */
  const attached = (el) => {
    if (!el.isConnected) return false;
    let w = el.ownerDocument?.defaultView;
    while (w && w !== window) {
      const frameEl = w.frameElement;
      if (!frameEl || !frameEl.isConnected) return false;
      w = frameEl.ownerDocument?.defaultView;
    }
    return w === window;
  };
  const CROSS_ORIGIN = "cannot reach this frame without DevTools access (cross-origin)";
  /** Every iframe of the page (nested ones too), same-origin ones with their document. */
  const allFrames = () => {
    const out = [];
    const walk = (doc, depth) => {
      for (const el of doc.querySelectorAll("iframe,frame")) {
        let inner = null;
        try {
          inner = el.contentDocument || null;
        } catch {}
        out.push({ id: frameId(el), el, doc: inner, url: inner ? inner.URL : el.src || "" });
        if (inner && depth < 6) walk(inner, depth + 1);
      }
    };
    walk(document, 0);
    return out;
  };
  /** The document of frame `id` (the top page when none); throws when it cannot be reached. */
  const docOf = (id) => {
    if (!id || id === "top") return document;
    const frame = allFrames().find((f) => f.id === String(id));
    if (!frame) throw new Error(`No frame ${id} on the page; list them with frames()`);
    if (!frame.doc) throw new Error(CROSS_ORIGIN);
    return frame.doc;
  };
  /** Same-origin content of one iframe element, or null. */
  const innerDoc = (frameEl) => {
    try {
      return frameEl.contentDocument || null;
    } catch {
      return null;
    }
  };
  const byRef = (ref) => {
    const el = refs.get(String(ref))?.deref();
    if (!el || !attached(el)) {
      const frame = /^(f\d+):/.exec(String(ref));
      if (frame) docOf(frame[1]); // throws for an unknown or cross-origin frame
      throw new Error(`${ref} is no longer on the page; read the page again`);
    }
    return el;
  };
  const SKIP = new Set(["SCRIPT", "STYLE", "NOSCRIPT", "TEMPLATE", "SVG", "HEAD", "META", "LINK"]);
  const shown = (el) => {
    const style = winOf(el).getComputedStyle(el);
    if (style.display === "none" || style.visibility === "hidden" || style.opacity === "0") return false;
    if (style.display === "contents") return true;
    const rect = el.getBoundingClientRect();
    // An empty box can still hold visible content (absolutely placed children).
    return rect.width > 0 || rect.height > 0 || (style.overflow === "visible" && !!el.firstElementChild);
  };
  const INTERACTIVE =
    'a[href],button,input:not([type=hidden]),select,textarea,summary,[role=button],[role=link],[role=checkbox],[role=radio],[role=switch],[role=tab],[role=menuitem],[role=option],[role=combobox],[role=textbox],[role=slider],[contenteditable=""],[contenteditable=true],[tabindex]:not([tabindex="-1"])';
  const inputRole = (el) =>
    ({ checkbox: "checkbox", radio: "radio", button: "button", submit: "button", reset: "button", image: "button", range: "slider", search: "searchbox", file: "file" })[
      (el.type || "").toLowerCase()
    ] || "textbox";
  const roleOf = (el) => {
    const explicit = el.getAttribute("role");
    if (explicit) return explicit;
    const tag = el.tagName;
    if (tag === "A") return el.hasAttribute("href") ? "link" : "";
    if (tag === "BUTTON" || tag === "SUMMARY") return "button";
    if (tag === "INPUT") return inputRole(el);
    if (tag === "SELECT") return "combobox";
    if (tag === "TEXTAREA") return "textbox";
    if (tag === "IMG") return "img";
    if (/^H[1-6]$/.test(tag)) return "heading";
    if (el.isContentEditable) return "textbox";
    return "";
  };
  const clean = (value, max = 120) => {
    const flat = (value || "").replace(/\s+/g, " ").trim();
    return flat.length > max ? `${flat.slice(0, max - 1)}…` : flat;
  };
  const nameOf = (el) => {
    const labelled = el.getAttribute("aria-labelledby");
    if (labelled) {
      const label = labelled
        .split(/\s+/)
        .map((id) => el.ownerDocument.getElementById(id)?.innerText || "")
        .join(" ");
      if (clean(label)) return clean(label);
    }
    return clean(
      el.getAttribute("aria-label") ||
        el.getAttribute("alt") ||
        el.labels?.[0]?.innerText ||
        el.getAttribute("title") ||
        el.getAttribute("placeholder") ||
        (el.tagName === "INPUT" || el.tagName === "SELECT" || el.tagName === "TEXTAREA" ? "" : el.innerText) ||
        el.getAttribute("name") ||
        el.getAttribute("value") ||
        "",
    );
  };
  const typeOf = (el) => {
    if (el.tagName !== "INPUT") return "";
    const type = (el.type || "text").toLowerCase();
    return ["text", "checkbox", "radio", "button", "submit", "reset", "image", "range", "search"].includes(type) ? "" : type;
  };
  const describe = (el) => {
    const role = roleOf(el) || el.tagName.toLowerCase();
    const name = nameOf(el);
    const parts = [`${role}${name ? ` "${name}"` : ""}`];
    if (typeOf(el)) parts.push(`type=${typeOf(el)}`);
    if (el.tagName === "A" && el.href) parts.push(`href=${clean(el.getAttribute("href"), 80)}`);
    if (el.tagName === "SELECT") parts.push(`value="${clean(el.selectedOptions?.[0]?.text, 80)}"`);
    else if ("value" in el && el.value && el.type !== "password" && el.tagName !== "BUTTON") parts.push(`value="${clean(el.value, 80)}"`);
    if (el.type === "password" && el.value) parts.push("value=••••");
    if (el.isContentEditable && el.textContent) parts.push(`value="${clean(el.textContent, 80)}"`);
    if (el.checked || el.getAttribute("aria-checked") === "true") parts.push("checked");
    if (el.disabled || el.getAttribute("aria-disabled") === "true") parts.push("disabled");
    if (el.getAttribute("aria-expanded")) parts.push(`expanded=${el.getAttribute("aria-expanded")}`);
    if (el.getAttribute("aria-selected") === "true") parts.push("selected");
    if (el.ownerDocument.activeElement === el) parts.push("focused");
    return `${parts.join(" ")} [${refOf(el)}]`;
  };
  const isInline = (el) => ["inline", "contents", "inline-block", "inline-flex"].includes(winOf(el).getComputedStyle(el).display);
  /** What an iframe line says about reaching into it. */
  const frameNote = (frameEl) => {
    const id = frameId(frameEl);
    return innerDoc(frameEl)
      ? { id, same: true, note: `same origin; read it with frame "${id}", its refs look like ${id}:ref_N` }
      : { id, same: false, note: "cross-origin: not reachable without DevTools access" };
  };

  /** Walk the page into lines: headings, text with inline links, and elements with refs. */
  const outline = (doc, filter, maxChars) => {
    const lines = [];
    let size = 0;
    let full = false;
    let line = "";
    const add = (value) => {
      if (full || !value) return;
      const room = maxChars - size;
      if (value.length + 1 > room) {
        if (room > 200) lines.push(`${value.slice(0, room - 1)}…`);
        full = true;
        return;
      }
      lines.push(value);
      size += value.length + 1;
    };
    const flush = () => {
      const value = line.replace(/\s+/g, " ").replace(/ ([,.;:!?)])/g, "$1").trim();
      line = "";
      if (filter === "all") add(value);
    };
    const visit = (el, inline) => {
      for (const node of el.childNodes) {
        if (full) return;
        if (node.nodeType === 3) {
          line += node.nodeValue;
          continue;
        }
        if (node.nodeType !== 1 || SKIP.has(node.tagName.toUpperCase())) continue;
        const child = node;
        if (!shown(child)) continue;
        const tag = child.tagName;
        if (/^H[1-6]$/.test(tag)) {
          flush();
          add(`${"#".repeat(Number(tag[1]))} ${clean(child.innerText, 300)} [${refOf(child)}]`);
          for (const inner of child.querySelectorAll(INTERACTIVE)) if (shown(inner)) add(`- ${describe(inner)}`);
          continue;
        }
        if (tag === "PRE") {
          flush();
          if (filter === "all") add(child.innerText.replace(/\n+$/, ""));
          continue;
        }
        if (tag === "BR") {
          if (inline) line += " ";
          else flush();
          continue;
        }
        if (child.matches(INTERACTIVE)) {
          const nested = child.querySelector(`${INTERACTIVE},h1,h2,h3,h4,h5,h6,p,li,table`);
          if (filter === "all" && isInline(child) && !nested) {
            // A link or control inside text stays in its sentence.
            line += roleOf(child) === "link" ? ` [${clean(child.innerText, 200) || nameOf(child)}](${refOf(child)}) ` : ` {${describe(child)}} `;
            continue;
          }
          flush();
          add(`- ${describe(child)}`);
          // Containers (a clickable card, a scroll area) still show what they hold.
          if (nested || child.children.length > 3) visit(child, false);
          flush();
          continue;
        }
        if (tag === "IMG") {
          const alt = nameOf(child);
          if (filter === "all" && alt) {
            if (isInline(child)) line += ` [img "${alt}"] `;
            else {
              flush();
              add(`- img "${alt}"`);
            }
          }
          continue;
        }
        if (tag === "IFRAME" || tag === "FRAME") {
          flush();
          const { id, note } = frameNote(child);
          const label = clean(child.getAttribute("title") || child.getAttribute("name") || child.src, 120);
          add(`- iframe ${id}${label ? ` ${label}` : ""} (${note})`);
          continue;
        }
        if (isInline(child)) {
          visit(child, true);
          continue;
        }
        // A table row stays on one line: cells joined with " | ".
        if (tag === "TD" || tag === "TH") {
          if (line.trim()) line += " | ";
          visit(child, true);
          continue;
        }
        flush();
        visit(child, false);
        flush();
      }
    };
    visit(doc.body || doc.documentElement, false);
    flush();
    return { lines, full };
  };

  const SKIP_TEXT = new Set([...SKIP, "TEXTAREA", "SELECT", "INPUT", "OPTION", "DATALIST", "CANVAS", "VIDEO", "AUDIO"]);
  /**
   * The page as text, block structure kept as line breaks. With `md` it is
   * Markdown: headings (#), lists, block quotes, links, emphasis, code spans
   * and fences, images with alt text, and tables as Markdown tables. Without
   * it nothing is marked up and a table row is its cells joined with " | ".
   * Same-origin iframes appear as a marker line naming their frame id.
   */
  const extract = (doc, md, maxChars) => {
    const out = [];
    let size = 0;
    let full = false;
    let line = "";
    let quote = 0; // block quote depth
    let flat = 0; // inside a heading or cell: a line break is a space
    let cell = 0; // inside a table cell being collected into `line`
    let marker = null; // list marker waiting for its first line
    const lists = []; // { ordered, n }
    const emit = (value) => {
      if (full) return;
      const room = maxChars - size;
      if (value.length + 1 > room) {
        if (room > 200) out.push(`${value.slice(0, room - 1)}…`);
        full = true;
        return;
      }
      out.push(value);
      size += value.length + 1;
    };
    const blank = () => {
      if (md && !cell && out.length && out[out.length - 1] !== "") emit("");
    };
    const indent = () => "   ".repeat(Math.max(0, lists.length - 1));
    /** The prefix of a continuation line: block quote marks and list indentation. */
    const lead = () => (md ? "> ".repeat(quote) + (lists.length ? indent() + "   " : "") : "");
    const flush = () => {
      if (cell) {
        line += " ";
        return;
      }
      const value = line.replace(/\s+/g, " ").trim();
      line = "";
      if (!value) return;
      let prefix = md ? "> ".repeat(quote) : "";
      if (md && (marker !== null || lists.length)) prefix += marker !== null ? indent() + marker : indent() + "   ";
      marker = null;
      emit(prefix + value);
    };
    const wrap = (start, open, close) => {
      const inner = line.slice(start);
      const core = inner.trim();
      if (!core) return;
      const at = inner.indexOf(core);
      line = line.slice(0, start) + inner.slice(0, at) + open + core + close + inner.slice(at + core.length);
    };
    const hrefOf = (a) => {
      const raw = a.getAttribute("href") || "";
      if (!raw || /^\s*javascript:/i.test(raw)) return "";
      return (raw.startsWith("#") ? raw : a.href || raw).replace(/\s/g, "%20").replace(/\)/g, "%29");
    };
    const tableOf = (table) => {
      const rows = [];
      for (const tr of table.rows) {
        if (!shown(tr)) continue;
        const cells = [];
        for (const td of tr.cells) {
          if (!shown(td)) continue;
          const saved = line;
          line = "";
          cell++;
          visit(td);
          cell--;
          let value = line.replace(/\s+/g, " ").trim();
          line = saved;
          if (md) value = value.replace(/\|/g, "\\|");
          cells.push(value);
          for (let i = 1; i < (td.colSpan || 1) && i < 20; i++) cells.push("");
        }
        if (cells.length) rows.push(cells);
      }
      return rows;
    };
    const visit = (el) => {
      for (const node of el.childNodes) {
        if (full) return;
        if (node.nodeType === 3) {
          line += node.nodeValue;
          continue;
        }
        if (node.nodeType !== 1 || SKIP_TEXT.has(node.tagName.toUpperCase())) continue;
        const child = node;
        if (!shown(child)) continue;
        const tag = child.tagName;
        if (/^H[1-6]$/.test(tag)) {
          flush();
          blank();
          flat++;
          visit(child);
          flat--;
          const value = line.replace(/\s+/g, " ").trim();
          line = value && md && !cell ? `${"#".repeat(Number(tag[1]))} ${value}` : value;
          flush();
          blank();
          continue;
        }
        if (tag === "PRE") {
          const raw = (child.innerText || child.textContent || "").replace(/\s+$/, "");
          if (cell) {
            line += ` ${raw.replace(/\s+/g, " ")} `;
            continue;
          }
          flush();
          blank();
          if (raw) {
            const rows = raw.split("\n");
            if (md) {
              const hint = `${child.querySelector("code")?.className || ""} ${child.className || ""}`;
              const language = /(?:^|\s)(?:lang|language)-([\w+#.-]+)/.exec(hint)?.[1] || "";
              let fence = "```";
              while (raw.includes(fence)) fence += "`";
              const base = lead();
              for (const row of [`${fence}${language}`, ...rows, fence]) emit(base + row);
            } else {
              for (const row of rows) emit(row);
            }
          }
          blank();
          continue;
        }
        if (tag === "BR") {
          if (flat || cell) line += " ";
          else flush();
          continue;
        }
        if (tag === "HR") {
          if (cell) continue;
          flush();
          if (md) {
            blank();
            emit(`${lead()}---`);
            blank();
          }
          continue;
        }
        if (tag === "IMG") {
          const alt = clean(child.getAttribute("alt"), 200);
          if (md && alt) {
            const src = child.currentSrc || child.src || "";
            line += ` ![${alt}](${src && !src.startsWith("data:") ? src.replace(/\s/g, "%20").replace(/\)/g, "%29") : ""}) `;
          }
          continue;
        }
        if (tag === "IFRAME" || tag === "FRAME") {
          const { id, same } = frameNote(child);
          const label = clean(child.getAttribute("title") || child.getAttribute("name") || child.src, 80);
          const mark = `[iframe ${id}${label ? ` ${label}` : ""}: ${same ? `read it with frame "${id}"` : "cross-origin, not reachable without DevTools access"}]`;
          if (cell) line += ` ${mark} `;
          else {
            flush();
            emit(lead() + mark);
          }
          continue;
        }
        if (tag === "TABLE" && !cell) {
          flush();
          blank();
          const caption = child.caption && shown(child.caption) ? clean(child.caption.innerText, 200) : "";
          if (caption) {
            line = caption;
            flush();
          }
          const rows = tableOf(child);
          if (rows.length) {
            const width = Math.max(...rows.map((r) => r.length));
            const pad = (r) => [...r, ...Array(width - r.length).fill("")];
            const base = lead();
            if (md) {
              emit(`${base}| ${pad(rows[0]).join(" | ")} |`);
              emit(`${base}|${" --- |".repeat(width)}`);
              for (const r of rows.slice(1)) emit(`${base}| ${pad(r).join(" | ")} |`);
            } else {
              for (const r of rows) {
                const cells = [...r];
                while (cells.length && !cells[cells.length - 1]) cells.pop();
                emit(cells.join(" | "));
              }
            }
          }
          blank();
          continue;
        }
        if (tag === "UL" || tag === "OL" || tag === "MENU") {
          flush();
          if (!lists.length) blank();
          lists.push({ ordered: tag === "OL", n: (parseInt(child.getAttribute("start"), 10) || 1) - 1 });
          visit(child);
          lists.pop();
          flush();
          marker = null;
          if (!lists.length) blank();
          continue;
        }
        if (tag === "LI") {
          flush();
          const list = lists[lists.length - 1];
          marker = list ? (list.ordered ? `${++list.n}. ` : "- ") : "- ";
          visit(child);
          flush();
          marker = null;
          continue;
        }
        if (tag === "BLOCKQUOTE") {
          flush();
          blank();
          quote++;
          visit(child);
          flush();
          quote--;
          blank();
          continue;
        }
        if (tag === "P") {
          flush();
          blank();
          visit(child);
          flush();
          blank();
          continue;
        }
        if (md && !cell) {
          // Inline formatting stays inside its line.
          const start = line.length;
          if (tag === "A" && hrefOf(child)) {
            const href = hrefOf(child);
            const before = out.length;
            visit(child);
            if (out.length === before) wrap(start, "[", `](${href})`);
            else {
              // A link around block content: its target after the last line.
              let last = out.length - 1;
              while (last >= 0 && out[last] === "") last--;
              if (last >= 0) {
                out[last] += ` (${href})`;
                size += href.length + 3;
              }
            }
            continue;
          }
          const marks = { B: "**", STRONG: "**", EM: "*", I: "*", S: "~~", DEL: "~~", STRIKE: "~~" };
          if (marks[tag] || tag === "CODE") {
            visit(child);
            if (tag === "CODE") {
              const inner = line.slice(start);
              const fence = inner.includes("`") ? "``" : "`";
              wrap(start, fence, fence);
            } else wrap(start, marks[tag], marks[tag]);
            continue;
          }
        } else if (cell && tag === "A" && md && hrefOf(child)) {
          const start = line.length;
          visit(child);
          wrap(start, "[", `](${hrefOf(child)})`);
          continue;
        }
        if (isInline(child) || cell) {
          visit(child);
          continue;
        }
        flush();
        visit(child);
        flush();
      }
    };
    visit(doc.body || doc.documentElement);
    flush();
    while (out.length && out[out.length - 1] === "") out.pop();
    return { lines: out, full };
  };

  /**
   * The innermost visible elements whose text contains `needle`, in page
   * order (a phrase can span inline tags: "<b>Workflows</b> are").
   */
  const textMatches = (doc, needle, limit) => {
    const hits = [];
    const has = (el) => (el.textContent || "").replace(/\s+/g, " ").toLowerCase().includes(needle);
    const visit = (el) => {
      let inChild = false;
      for (const child of el.children) {
        if (hits.length >= limit) return true;
        if (SKIP.has(child.tagName.toUpperCase()) || !has(child)) continue;
        if (visit(child)) inChild = true;
      }
      if (inChild || hits.length >= limit) return inChild;
      if (!shown(el)) return false;
      hits.push({ parent: el });
      return true;
    };
    const root = doc.body || doc.documentElement;
    if (needle && has(root)) visit(root);
    return hits;
  };

  /** The documents a search covers: the top page, one frame, or all of them. */
  const docsFor = (frame) => {
    if (frame === "all") return [document, ...allFrames().filter((f) => f.doc).map((f) => f.doc)];
    return [docOf(frame)];
  };

  // --- geometry and synthetic input -----------------------------------------

  /** Where an iframe's content starts in its parent's viewport, and its scale. */
  const frameOrigin = (frameEl) => {
    const rect = frameEl.getBoundingClientRect();
    const style = winOf(frameEl).getComputedStyle(frameEl);
    const sx = (frameEl.offsetWidth ? rect.width / frameEl.offsetWidth : 1) || 1;
    const sy = (frameEl.offsetHeight ? rect.height / frameEl.offsetHeight : 1) || 1;
    const px = (frameEl.clientLeft || 0) + (parseFloat(style.paddingLeft) || 0);
    const py = (frameEl.clientTop || 0) + (parseFloat(style.paddingTop) || 0);
    return { ox: rect.left + px * sx, oy: rect.top + py * sy, sx, sy };
  };
  /** A point (and size) in `doc`'s viewport, in the top page's viewport. */
  const toTop = (doc, x, y, w = 0, h = 0) => {
    let d = doc;
    while (d && d !== document) {
      const frameEl = d.defaultView?.frameElement;
      if (!frameEl) break;
      const o = frameOrigin(frameEl);
      x = o.ox + x * o.sx;
      y = o.oy + y * o.sy;
      w *= o.sx;
      h *= o.sy;
      d = frameEl.ownerDocument;
    }
    return { x, y, width: w, height: h };
  };
  /** The innermost element at a point of the top viewport, looking into same-origin frames. */
  const deepFromPoint = (tx, ty) => {
    let x = tx;
    let y = ty;
    let el = document.elementFromPoint(x, y);
    for (let depth = 0; el && (el.tagName === "IFRAME" || el.tagName === "FRAME") && depth < 6; depth++) {
      const inner = innerDoc(el);
      if (!inner) break;
      const o = frameOrigin(el);
      const ix = (x - o.ox) / o.sx;
      const iy = (y - o.oy) / o.sy;
      const next = inner.elementFromPoint(ix, iy);
      if (!next) break;
      el = next;
      x = ix;
      y = iy;
    }
    return el ? { el, x, y } : null;
  };
  /** Bring `el` into view; its centre in its own viewport. */
  const localPoint = (el, block = "center") => {
    el.scrollIntoView({ block, inline: block === "center" ? "center" : "nearest", behavior: "instant" });
    const rects = [...el.getClientRects()].filter((r) => r.width > 0 && r.height > 0);
    const rect = rects[0] || el.getBoundingClientRect();
    return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2, width: rect.width, height: rect.height, rect };
  };
  /** The element a pointer at the centre of `el` would hit, if that is `el` or inside it; else `el`. */
  const targetAt = (el, x, y) => {
    const top = el.ownerDocument.elementFromPoint(x, y);
    return top && (top === el || el.contains(top)) ? top : el;
  };
  const NO_BUBBLE = /^(mouse|pointer)(enter|leave)$/;
  /** A mouse or pointer event for `el`'s own window, at (x, y) of its viewport. */
  const pointerEvent = (el, type, x, y, extra = {}) => {
    const w = winOf(el);
    const init = {
      bubbles: !NO_BUBBLE.test(type),
      cancelable: !NO_BUBBLE.test(type),
      composed: true,
      view: w,
      clientX: x,
      clientY: y,
      screenX: x + (w.screenX || 0),
      screenY: y + (w.screenY || 0),
      button: 0,
      buttons: 0,
      ...extra,
    };
    if (type.startsWith("pointer") && w.PointerEvent) return new w.PointerEvent(type, { pointerId: 1, pointerType: "mouse", isPrimary: true, width: 1, height: 1, ...init });
    return new w.MouseEvent(type, init);
  };
  const fire = (el, type, x, y, extra) => el.dispatchEvent(pointerEvent(el, type, x, y, extra));
  const pathOf = (el) => {
    const path = [];
    for (let n = el; n; n = n.parentElement) path.push(n);
    return path;
  };
  let hovered = null;
  /** Move the pointer onto `target`: leave / out for what it left, over / enter for what it entered, then move. */
  const hoverAt = (target, x, y) => {
    const prev = hovered && attached(hovered) ? hovered : null;
    const prevPath = prev ? pathOf(prev) : [];
    const nextPath = pathOf(target);
    if (prev !== target) {
      if (prev) {
        const prevX = prev.ownerDocument === target.ownerDocument ? x : 0;
        const prevY = prev.ownerDocument === target.ownerDocument ? y : 0;
        fire(prev, "pointerout", prevX, prevY, { relatedTarget: target });
        fire(prev, "mouseout", prevX, prevY, { relatedTarget: target });
        for (const n of prevPath) {
          if (nextPath.includes(n)) continue;
          fire(n, "pointerleave", prevX, prevY, { relatedTarget: target });
          fire(n, "mouseleave", prevX, prevY, { relatedTarget: target });
        }
      }
      fire(target, "pointerover", x, y, { relatedTarget: prev });
      for (const n of [...nextPath].reverse()) if (!prevPath.includes(n)) fire(n, "pointerenter", x, y, { relatedTarget: prev });
      fire(target, "mouseover", x, y, { relatedTarget: prev });
      for (const n of [...nextPath].reverse()) if (!prevPath.includes(n)) fire(n, "mouseenter", x, y, { relatedTarget: prev });
    }
    fire(target, "pointermove", x, y);
    fire(target, "mousemove", x, y);
    hovered = target;
  };
  /** Native range sliders ignore synthetic mouse events: set the value the drag would reach. */
  const dragRange = (el, endX) => {
    const r = el.getBoundingClientRect();
    const box = toTop(el.ownerDocument, r.left, r.top, r.width, r.height);
    const fraction = Math.min(1, Math.max(0, (endX - box.x) / (box.width || 1)));
    const min = parseFloat(el.min) || 0;
    const max = el.max === "" ? 100 : parseFloat(el.max);
    const step = parseFloat(el.step) > 0 ? parseFloat(el.step) : 1;
    const value = Math.min(max, Math.max(min, min + Math.round(((max - min) * fraction) / step) * step));
    Object.getOwnPropertyDescriptor(winOf(el).HTMLInputElement.prototype, "value").set.call(el, String(value));
    el.dispatchEvent(new (winOf(el).Event)("input", { bubbles: true }));
    el.dispatchEvent(new (winOf(el).Event)("change", { bubbles: true }));
  };
  /** The nearest ancestor (or the page) that can scroll by (dx, dy). */
  const scroller = (el, dx, dy) => {
    const can = (n) => {
      const style = winOf(n).getComputedStyle(n);
      const y = /(auto|scroll|overlay)/.test(style.overflowY) && n.scrollHeight > n.clientHeight && (dy > 0 ? n.scrollTop + n.clientHeight < n.scrollHeight - 1 : dy < 0 ? n.scrollTop > 0 : false);
      const x = /(auto|scroll|overlay)/.test(style.overflowX) && n.scrollWidth > n.clientWidth && (dx > 0 ? n.scrollLeft + n.clientWidth < n.scrollWidth - 1 : dx < 0 ? n.scrollLeft > 0 : false);
      return x || y;
    };
    for (let n = el; n && n !== el.ownerDocument.documentElement && n !== el.ownerDocument.body; n = n.parentElement) if (can(n)) return n;
    return el.ownerDocument.scrollingElement || el.ownerDocument.documentElement;
  };

  /** What `ref` looks like now, or null when it is gone. */
  const stateOf = (ref) => {
    const el = refs.get(String(ref))?.deref();
    if (!el || !attached(el)) return null;
    return describe(el);
  };

  window.__evo = {
    /**
     * The page, as `format`: "outline" (headings, text with filter "all", and
     * interactive elements with refs), "text" (the readable text, one line per
     * block) or "markdown" (headings, lists, links, tables, code fences).
     * `frame` is an id from `frames()` (default: the top page). `filter` only
     * shapes the outline. Output starts with the URL and title.
     */
    readPage({ filter = "all", maxChars = 30000, format = "outline", frame } = {}) {
      if (!["outline", "text", "markdown"].includes(format)) throw new Error(`Unknown format "${format}"; use outline, text or markdown`);
      const doc = docOf(frame);
      const w = doc.defaultView || window;
      const root = doc.documentElement;
      const head = [
        `URL: ${doc.location.href}`,
        frame && frame !== "top" ? `Frame: ${frame}` : "",
        `Title: ${doc.title || "(none)"}`,
        format === "outline" ? `Viewport: ${w.innerWidth}x${w.innerHeight}, scrolled ${Math.round(w.scrollY)} of ${Math.max(0, (root?.scrollHeight || 0) - w.innerHeight)}` : "",
        doc.readyState !== "complete" ? `Still loading (${doc.readyState})` : "",
        "",
      ].filter((l, i, all) => l || i === all.length - 1);
      const room = maxChars - head.join("\n").length;
      if (format !== "outline") {
        const { lines, full } = extract(doc, format === "markdown", room);
        if (full) lines.push(`… (truncated at ${maxChars} characters; raise maxChars, or use find or scroll with text)`);
        if (!lines.length) lines.push("(the page shows no text)");
        return [...head, ...lines].join("\n");
      }
      const { lines, full } = outline(doc, filter, room);
      if (full) lines.push(`… (truncated at ${maxChars} characters; raise maxChars, or use find, scroll with text, or filter "interactive")`);
      if (!lines.length) lines.push(filter === "interactive" ? "(no interactive elements)" : "(the page shows no text)");
      return [...head, ...lines].join("\n");
    },
    /** The iframes of the page, nested ones too: id, URL, and whether script can reach inside. */
    frames() {
      return allFrames().map((f) => ({ id: f.id, url: f.url, sameOrigin: !!f.doc }));
    },
    /**
     * Elements whose role, name or text contains `query`. `frame`: a frame id,
     * or "all" for the page and every same-origin frame (default: the top page).
     */
    find(query, limit = 20, frame) {
      const needle = String(query).toLowerCase();
      const hits = [];
      const seen = new Set();
      for (const doc of docsFor(frame)) {
        for (const el of doc.querySelectorAll(INTERACTIVE)) {
          if (hits.length >= limit) break;
          if (!shown(el)) continue;
          if (`${roleOf(el)} ${nameOf(el)} ${el.value || ""}`.toLowerCase().includes(needle)) {
            seen.add(el);
            hits.push(describe(el));
          }
        }
        for (const { parent } of textMatches(doc, needle, limit * 3)) {
          if (hits.length >= limit) break;
          const target = parent.closest(INTERACTIVE) || parent;
          if (seen.has(target)) continue;
          seen.add(target);
          hits.push(target.matches(INTERACTIVE) ? describe(target) : `text "${clean(target.innerText, 160)}" [${refOf(target)}]`);
        }
      }
      return hits.length ? hits.join("\n") : `Nothing on the page matches "${query}".`;
    },
    /**
     * Scroll `ref` into view; its centre in the top page's viewport CSS pixels
     * (also for an element in a frame), and whether a click there reaches it
     * (or what covers it).
     */
    point(ref) {
      const el = byRef(ref);
      const local = localPoint(el);
      const doc = el.ownerDocument;
      let { x, y, width, height } = local;
      const hit = doc.elementFromPoint(x, y);
      const label = hit?.closest("label");
      let hits = !!hit && (hit === el || el.contains(hit) || (hit.contains(el) && hit.tagName === "LABEL") || (label && label.control === el));
      let covered = hits || !hit ? null : describe(hit);
      let d = doc;
      while (d && d !== document) {
        const frameEl = d.defaultView?.frameElement;
        if (!frameEl) break;
        const o = frameOrigin(frameEl);
        x = o.ox + x * o.sx;
        y = o.oy + y * o.sy;
        width *= o.sx;
        height *= o.sy;
        d = frameEl.ownerDocument;
        if (hits) {
          // The frame itself must be what sits at that point of its page.
          const outer = d.elementFromPoint(x, y);
          if (outer && outer !== frameEl && !frameEl.contains(outer)) {
            hits = false;
            covered = describe(outer);
          }
        }
      }
      return { x, y, width, height, hits, covered };
    },
    /** What `ref` looks like now (after a click), or that it is gone. */
    state(ref) {
      return stateOf(ref);
    },
    /** For script fallbacks: did the element take the action? */
    took(ref, action) {
      const el = byRef(ref);
      const active = el.ownerDocument.activeElement;
      if (action === "check") return el.checked === true;
      if (action === "focus") return active === el || el.contains(active);
      return true;
    },
    /** Whether a click is expected to check or focus `ref`. */
    clickEffect(ref) {
      const el = byRef(ref);
      if (el.disabled) return "none";
      if (el.tagName === "INPUT" && el.type === "radio") return "check";
      if (el.tagName === "INPUT" && ["text", "search", "email", "url", "tel", "password", "number", "date", "time", "datetime-local", "month", "week"].includes(el.type)) return "focus";
      if (el.tagName === "TEXTAREA" || el.isContentEditable) return "focus";
      return "none";
    },
    focus(ref) {
      const el = byRef(ref);
      el.scrollIntoView({ block: "center", behavior: "instant" });
      el.focus();
      const active = el.ownerDocument.activeElement;
      if (active !== el && !el.contains(active)) throw new Error(`${ref} (${describe(el)}) cannot take focus`);
      // Type at the end of what is there.
      if (typeof el.setSelectionRange === "function" && /^(text|search|url|tel|password|email)?$/.test(el.type || "") || el.tagName === "TEXTAREA") {
        try {
          el.setSelectionRange(el.value.length, el.value.length);
        } catch {}
      }
      return true;
    },
    /** Click without real input (platforms without the DevTools protocol, or a fallback). */
    click(ref) {
      const el = byRef(ref);
      el.scrollIntoView({ block: "center", behavior: "instant" });
      el.focus?.();
      el.click();
      return true;
    },
    /**
     * Hover without real input: pointer and mouse events over `ref`'s centre
     * (leave / out for what the pointer left, over / enter for what it
     * entered, then move). Returns the element's state text.
     */
    hover(ref) {
      const el = byRef(ref);
      const { x, y } = localPoint(el);
      hoverAt(targetAt(el, x, y), x, y);
      return stateOf(ref) ?? "(no longer on the page)";
    },
    /**
     * Drag without real input. `from` is a ref; `to` a ref or { x, y } in the
     * top page's viewport. Mouse events (pointer down, a few moves, up); for an
     * element with draggable="true" the drag events too (dragstart, drag,
     * dragenter, dragover, drop, dragend); a range input is set to the value
     * under the end point. Returns the source's state text.
     */
    dragTo(from, to) {
      const src = byRef(from);
      const a = localPoint(src, "nearest");
      const doc = src.ownerDocument;
      const start = toTop(doc, a.x, a.y);
      let end;
      if (typeof to === "string") {
        const dst = byRef(to);
        const b = localPoint(dst, "nearest");
        end = toTop(dst.ownerDocument, b.x, b.y);
      } else if (to && typeof to.x === "number" && typeof to.y === "number") {
        end = { x: to.x, y: to.y };
      } else {
        throw new Error("dragTo needs a ref or { x, y } to drag to");
      }
      const at = (t) => deepFromPoint(start.x + (end.x - start.x) * t, start.y + (end.y - start.y) * t);
      const last = deepFromPoint(end.x, end.y);
      if (!last) throw new Error(`Nothing at ${Math.round(end.x)},${Math.round(end.y)} to drag to`);
      const w = winOf(src);
      const down = { button: 0, buttons: 1 };
      hoverAt(targetAt(src, a.x, a.y), a.x, a.y);
      fire(src, "pointerdown", a.x, a.y, down);
      fire(src, "mousedown", a.x, a.y, down);
      if (src.tagName === "INPUT" && src.type === "range") {
        dragRange(src, end.x);
        fire(last.el, "pointerup", last.x, last.y);
        fire(last.el, "mouseup", last.x, last.y);
        return stateOf(from) ?? "(no longer on the page)";
      }
      const dragSrc = src.closest('[draggable="true"]');
      let transfer = null;
      if (dragSrc) {
        try {
          transfer = new w.DataTransfer();
        } catch {}
      }
      const STEPS = 6;
      if (dragSrc && transfer) {
        const drag = (el, type, x, y) => {
          const event = new (winOf(el).DragEvent)(type, { bubbles: true, cancelable: true, composed: true, view: winOf(el), clientX: x, clientY: y, dataTransfer: transfer });
          return !el.dispatchEvent(event) || event.defaultPrevented; // true when the page cancelled it
        };
        const sx = a.x;
        const sy = a.y;
        if (!drag(dragSrc, "dragstart", sx, sy)) {
          let over = null;
          let accepts = false;
          for (let i = 1; i <= STEPS; i++) {
            const p = i === STEPS ? last : at(i / STEPS);
            if (!p) continue;
            drag(dragSrc, "drag", p.x, p.y);
            if (p.el !== over) {
              if (over) drag(over, "dragleave", p.x, p.y);
              drag(p.el, "dragenter", p.x, p.y);
              over = p.el;
            }
            accepts = drag(p.el, "dragover", p.x, p.y);
          }
          if (accepts && over) drag(over, "drop", last.x, last.y);
          drag(dragSrc, "dragend", last.x, last.y);
        } else {
          // dragstart was cancelled: the page refuses the drag.
          drag(dragSrc, "dragend", sx, sy);
        }
      } else {
        for (let i = 1; i <= STEPS; i++) {
          const p = i === STEPS ? last : at(i / STEPS);
          if (!p) continue;
          fire(p.el, "pointermove", p.x, p.y, down);
          fire(p.el, "mousemove", p.x, p.y, down);
        }
        fire(last.el, "pointerup", last.x, last.y);
        fire(last.el, "mouseup", last.x, last.y);
        if (last.el === src || src.contains(last.el)) fire(last.el, "click", last.x, last.y);
      }
      hovered = last.el;
      return stateOf(from) ?? "(no longer on the page)";
    },
    /**
     * A mouse wheel turn over `refOrPoint` (a ref, or { x, y } in the top
     * page's viewport): the wheel event, then, unless the page cancelled it,
     * the nearest scrollable ancestor (or the page) scrolls by (dx, dy).
     */
    wheel(refOrPoint, dx = 0, dy = 0) {
      let el;
      let x;
      let y;
      if (typeof refOrPoint === "string") {
        const target = byRef(refOrPoint);
        ({ x, y } = localPoint(target, "nearest"));
        el = targetAt(target, x, y);
      } else if (refOrPoint && typeof refOrPoint.x === "number" && typeof refOrPoint.y === "number") {
        const hit = deepFromPoint(refOrPoint.x, refOrPoint.y);
        if (!hit) throw new Error(`Nothing at ${Math.round(refOrPoint.x)},${Math.round(refOrPoint.y)} to scroll`);
        ({ el, x, y } = hit);
      } else {
        throw new Error("wheel needs a ref or { x, y }");
      }
      const w = winOf(el);
      const event = new w.WheelEvent("wheel", { bubbles: true, cancelable: true, composed: true, view: w, clientX: x, clientY: y, deltaX: dx, deltaY: dy, deltaMode: 0 });
      const label = `wheel dx=${dx} dy=${dy}`;
      if (!el.dispatchEvent(event)) return `${label}: the page handled it`;
      const target = scroller(el, dx, dy);
      const before = [target.scrollLeft, target.scrollTop];
      target.scrollBy({ left: dx, top: dy, behavior: "instant" });
      const after = [target.scrollLeft, target.scrollTop];
      const doc = el.ownerDocument;
      const what = target === doc.scrollingElement || target === doc.documentElement ? "the page" : describe(target);
      if (before[0] === after[0] && before[1] === after[1]) return `${label}: ${what} did not scroll (already at its end)`;
      return `${label}: ${what} scrolled to ${Math.round(after[0])},${Math.round(after[1])}`;
    },
    /** The element typing goes to: its ref, description, and whether keys can fill it. */
    typingTarget(ref) {
      let el = ref ? byRef(ref) : document.activeElement;
      // Focus inside a same-origin iframe: the element in it, not the frame.
      while (!ref && el && (el.tagName === "IFRAME" || el.tagName === "FRAME")) {
        const inner = innerDoc(el);
        if (!inner || !inner.activeElement) break;
        el = inner.activeElement;
      }
      if (!el || el === el.ownerDocument.body || el === el.ownerDocument.documentElement) return null;
      const type = el.tagName === "INPUT" ? (el.type || "text").toLowerCase() : "";
      const kind =
        ["date", "time", "datetime-local", "month", "week", "color", "range"].includes(type) || el.tagName === "SELECT"
          ? "set"
          : ["text", "search", "email", "url", "tel", "password", "number"].includes(type) || el.tagName === "TEXTAREA" || el.isContentEditable
            ? "keys"
            : "other";
      return { ref: refOf(el), kind, value: el.isContentEditable ? el.textContent : "value" in el ? String(el.value ?? "") : "" };
    },
    /** Insert text at the caret the way an editor does (fallback when real input is lost). */
    insert(ref, value) {
      const el = byRef(ref);
      el.focus();
      if (!el.ownerDocument.execCommand("insertText", false, value)) {
        if (el.isContentEditable) el.textContent += value;
        else {
          const w = winOf(el);
          const proto = el.tagName === "TEXTAREA" ? w.HTMLTextAreaElement.prototype : w.HTMLInputElement.prototype;
          Object.getOwnPropertyDescriptor(proto, "value").set.call(el, `${el.value}${value}`);
          el.dispatchEvent(new w.Event("input", { bubbles: true }));
        }
      }
      return true;
    },
    /** Set a field's value the way typing would (input + change events). */
    setValue(ref, value) {
      const el = byRef(ref);
      const w = winOf(el);
      el.scrollIntoView({ block: "center", behavior: "instant" });
      el.focus();
      if (el.tagName === "SELECT") {
        const option = [...el.options].find((o) => o.value === value || clean(o.text) === value);
        if (!option) throw new Error(`No option "${value}"; options: ${[...el.options].map((o) => clean(o.text)).join(", ")}`);
        el.value = option.value;
      } else if (el.type === "checkbox" || el.type === "radio") {
        el.checked = value === true || value === "true" || value === "on";
      } else if (el.isContentEditable) {
        el.textContent = value;
      } else {
        const proto = el.tagName === "TEXTAREA" ? w.HTMLTextAreaElement.prototype : w.HTMLInputElement.prototype;
        Object.getOwnPropertyDescriptor(proto, "value").set.call(el, value);
        if (value && el.value === "" && el.tagName === "INPUT") throw new Error(`"${value}" is not a valid value for ${describe(el)}`);
      }
      el.dispatchEvent(new w.Event("input", { bubbles: true }));
      el.dispatchEvent(new w.Event("change", { bubbles: true }));
      return describe(el);
    },
    /**
     * Scroll an element or the first text matching `query` to the middle of
     * the view. `frame` limits the text search to one frame, or "all" (the
     * default is the top page).
     */
    scrollTo({ ref, query, frame }) {
      let target;
      let label;
      if (ref) {
        target = byRef(ref);
        label = describe(target);
      } else {
        let hit;
        for (const doc of docsFor(frame)) {
          [hit] = textMatches(doc, String(query).toLowerCase(), 1);
          if (hit) break;
        }
        if (!hit) throw new Error(`No text on the page matches "${query}"`);
        target = hit.parent;
        label = `text "${clean(target.innerText, 160)}" [${refOf(target)}]`;
      }
      target.scrollIntoView({ block: "center", inline: "nearest", behavior: "instant" });
      return label;
    },
    text(maxChars = 30000) {
      return (document.body?.innerText || "").slice(0, maxChars);
    },
    /**
     * Where the shell cannot hold dialogs: the answer the next ones get. `{
     * accept, text }` (text answers prompts); null returns to the defaults
     * (alert returns, confirm false, prompt null). Stays until changed.
     */
    dialogPolicy(policy) {
      dialogAnswer = policy && typeof policy === "object" ? { accept: !!policy.accept, text: policy.text } : null;
      return dialogAnswer;
    },
    /** Dialogs the page opened while the fallback was on: kind, message, time, answer. */
    dialogLog(clear = false) {
      const log = dialogs.slice();
      if (clear) dialogs.length = 0;
      return log;
    },
    logSeq() {
      return logSeq;
    },
    console(limit = 100, onlyErrors = false, after = 0) {
      return logs.filter((l) => l.seq > after && (!onlyErrors || l.level === "error")).slice(-limit);
    },
    network(limit = 100) {
      return performance
        .getEntriesByType("navigation")
        .concat(performance.getEntriesByType("resource"))
        .slice(-limit)
        .map((e) => ({
          url: e.name,
          type: e.initiatorType || e.entryType,
          status: e.responseStatus || undefined,
          ms: Math.round(e.duration),
          bytes: e.transferSize || undefined,
        }));
    },
  };
})();
