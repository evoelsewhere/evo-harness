// The API a `run_code` program sees (docs/features/code-mode/spec.md, FR-5,
// FR-6, FR-8). Runs before the program, over the only two host functions:
// __evo_call(name, argsJson) -> Promise<replyJson> and __evo_log(stream, text).
// The host sets __evo_catalog (the tools the program may call) first.
"use strict";
(() => {
  const hostCall = globalThis.__evo_call;
  const hostLog = globalThis.__evo_log;
  const catalog = Array.isArray(globalThis.__evo_catalog) ? globalThis.__evo_catalog : [];
  delete globalThis.__evo_call;
  delete globalThis.__evo_log;
  delete globalThis.__evo_catalog;

  const names = new Set(catalog.map((tool) => tool.name));
  const show = (value) => {
    if (typeof value === "string") return value;
    if (value instanceof Error) return value.stack || String(value);
    try {
      const text = JSON.stringify(value);
      return text === undefined ? String(value) : text;
    } catch {
      return String(value);
    }
  };
  const print = (stream) => (...values) => hostLog(stream, values.map(show).join(" "));
  globalThis.console = Object.freeze({
    log: print(1),
    info: print(1),
    debug: print(1),
    warn: print(2),
    error: print(2),
  });

  /** A tool call that failed, was blocked or was declined. Its stack is
   * where the program made the call: QuickJS keeps no frames across await. */
  class ToolError extends Error {
    constructor(tool, message, reply, site) {
      super(message);
      this.name = "ToolError";
      this.tool = tool;
      this.reply = reply;
      if (site) this.stack = `ToolError: ${message}\n${site}`;
    }
  }
  globalThis.ToolError = ToolError;
  /** The caller's frames, taken while the call is still synchronous. */
  const callSite = () => {
    const stack = new Error().stack || "";
    return stack
      .split("\n")
      .filter((line) => line.includes("code.js"))
      .join("\n");
  };

  async function raw(name, args) {
    if (args === undefined) args = {};
    if (args === null || typeof args !== "object" || Array.isArray(args)) {
      throw new TypeError(`${name}: arguments must be an object`);
    }
    return JSON.parse(await hostCall(String(name), JSON.stringify(args)));
  }

  function invoke(name, args) {
    const site = callSite();
    return raw(name, args).then((reply) => {
      if (!reply.ok) throw new ToolError(name, reply.text, reply, site);
      return reply.text;
    });
  }

  const list = () => catalog.map(({ name, description }) => ({ name, description }));
  const describe = (name) => {
    const tool = catalog.find((entry) => entry.name === name);
    if (!tool) throw new Error(`No tool named ${name} in this chat`);
    return tool;
  };
  globalThis.tools = new Proxy(Object.freeze(Object.create(null)), {
    get(_, prop) {
      if (prop === "list") return list;
      if (prop === "describe") return describe;
      if (prop === "call") return invoke;
      if (typeof prop !== "string" || prop === "then") return undefined;
      return (args) => invoke(prop, args);
    },
  });

  // ── Shell commands ────────────────────────────────────────────────────────
  const shell = names.has("bash") ? "bash" : names.has("powershell") ? "powershell" : null;
  const quote = (value) => {
    const text = String(value);
    if (shell === "powershell") return `'${text.replace(/'/g, "''")}'`;
    if (text !== "" && /^[A-Za-z0-9_\/.,:=@%+-]+$/.test(text)) return text;
    return `'${text.replace(/'/g, `'\\''`)}'`;
  };
  const exitCode = (reply) => {
    if (reply.ok) return 0;
    const match = /Command exited with code (-?\d+)\s*$/.exec(reply.text);
    return match ? Number(match[1]) : null;
  };
  const withoutStatus = (text) =>
    text.replace(/(\n\n)?Command exited with code -?\d+\s*$/, "").replace(/^\(no output\)$/, "");

  /** A shell command; it starts at once and is awaited like a promise of its text. */
  class Command {
    constructor(command, options) {
      if (!shell) throw new Error("This chat has no shell tool (bash or powershell).");
      this.command = command;
      this.site = callSite();
      const args = { command };
      if (options && options.timeout !== undefined) args.timeout = options.timeout;
      this.run = raw(shell, args);
    }
    async nothrow() {
      const reply = await this.run;
      return {
        exitCode: exitCode(reply),
        output: withoutStatus(reply.text),
        ok: reply.ok,
        // The shell tool cut its output: what is here is not all of it.
        truncated: Boolean(reply.details && reply.details.truncation && reply.details.truncation.truncated),
      };
    }
    async text() {
      const reply = await this.run;
      if (!reply.ok) throw new ToolError(shell, reply.text, reply, this.site);
      return withoutStatus(reply.text);
    }
    async json() {
      return JSON.parse(await this.text());
    }
    async lines() {
      return (await this.text()).split(/\r?\n/).filter((line) => line.length > 0);
    }
    then(resolve, reject) {
      return this.text().then(resolve, reject);
    }
    catch(reject) {
      return this.text().catch(reject);
    }
    finally(done) {
      return this.text().finally(done);
    }
  }

  const tag = (options) => (strings, ...values) => {
    if (!Array.isArray(strings) || !strings.raw) return new Command(String(strings), options);
    let command = strings[0];
    values.forEach((value, i) => {
      command += (Array.isArray(value) ? value.map(quote).join(" ") : quote(value)) + strings[i + 1];
    });
    return new Command(command, options);
  };
  const $ = tag(undefined);
  /** `$.timeout(600)\`npm test\``: the shell tool's timeout, in seconds. */
  $.timeout = (seconds) => tag({ timeout: seconds });
  $.shell = shell;
  globalThis.$ = $;

  /** Files under `dir` (relative paths, sorted), optionally ending in `ext`: one shell call. */
  globalThis.files = async (dir = ".", ext) => {
    const listing =
      shell === "powershell"
        ? await new Command(`Get-ChildItem -Recurse -File -Path ${quote(dir)} | Resolve-Path -Relative`)
        : await new Command(`find ${quote(dir)} -type f`);
    return listing
      .split(/\r?\n/)
      .filter(Boolean)
      .map((path) => path.replace(/\\/g, "/").replace(/^\.\//, ""))
      .filter((path) => !ext || path.endsWith(ext))
      .sort();
  };

  // ── File shortcuts: each is exactly one ordinary tool call ───────────────
  // read(path) gives the whole file. The read tool cuts a file at one page
  // (2000 lines or 50 KB) and ends the text with a "Use offset=N to continue"
  // notice; handing that back as the file would let write(path, await read(path))
  // save a cut file. So read on, page by page, and join the pages. A file that
  // cannot be read whole rejects. With offset or limit the program asked for a
  // range, and gets what the tool answers (FR-6).
  const MAX_PAGES = 64;
  const NOTICE = /\n\n\[[^\]\n]*Use offset=(\d+) to continue\.\]$/;
  async function readWhole(path, options) {
    const site = callSite();
    const fail = (message) => {
      throw new ToolError("read", message, null, site);
    };
    const pages = [];
    let offset = 1;
    for (let page = 1; page <= MAX_PAGES; page++) {
      const args = offset === 1 ? { ...options, path } : { ...options, path, offset };
      const reply = await raw("read", args);
      if (!reply.ok) throw new ToolError("read", reply.text, reply, site);
      const cut = reply.details && reply.details.truncation;
      if (cut && cut.first_line_exceeds_limit) {
        fail(
          `${path} cannot be read whole: line ${offset} is longer than one page of the read tool. ` +
            `Use $\`sed -n\` or read(path, { offset, limit }) for the rest.`,
        );
      }
      const next = cut && cut.truncated ? NOTICE.exec(reply.text) : null;
      if (!next) {
        pages.push(reply.text);
        return pages.join("\n");
      }
      pages.push(reply.text.slice(0, next.index));
      const following = Number(next[1]);
      if (!(following > offset)) fail(`${path} cannot be read whole: the read tool did not advance past line ${offset}.`);
      offset = following;
    }
    return fail(
      `${path} cannot be read whole: it is longer than ${MAX_PAGES} pages of the read tool. ` +
        `Read a range with read(path, { offset, limit }) or search it with $\`rg\`.`,
    );
  }
  globalThis.read = (path, options) => {
    const given = options || {};
    if (given.offset !== undefined || given.limit !== undefined) return invoke("read", { ...given, path });
    return readWhole(path, given);
  };
  globalThis.write = (path, content) => invoke("write", { path, content: String(content) });
  globalThis.edit = (path, oldText, newText) =>
    invoke("edit", {
      path,
      edits: Array.isArray(oldText) ? oldText : [{ oldText, newText }],
    });
  const needs = (tool, hint) => {
    if (!names.has(tool)) throw new Error(`${tool} is not a tool in this chat; ${hint}`);
  };
  globalThis.grep = (pattern, options) => {
    needs("grep", "use $`rg ...` instead");
    return invoke("grep", { ...(options || {}), pattern });
  };
  globalThis.glob = (pattern, options) => {
    needs("find", "use $`rg --files ...` instead");
    return invoke("find", { ...(options || {}), pattern });
  };
})();
