// A terminal tab's output as plain text, for the agent's tools. No React, no
// Tauri: pure so it can be tested. Spec: docs/features/agent-terminal (FR-6,
// FR-8, FR-9, FR-16, NFR-3, NFR-4, NFR-5).
//
// Raw pty data goes in through `feed`; escape sequences are removed (a
// sequence split across two chunks is carried over), and OSC 133 shell
// integration marks are stored as markers at the current offset. Offsets are
// running character counts, so a cursor stays valid while the ring drops its
// oldest text.

/** Plain output kept per tab. */
export const LOG_MAX = 1024 * 1024;
/** The ring is trimmed down this much below the limit, so it is not trimmed per chunk. */
const TRIM_SLACK = 64 * 1024;
/** Without seeing the typed command echoed, a pattern wait scans everything after this long. */
const ECHO_PATIENCE_MS = 1500;
/** A pattern wait re-tests this much text before the new data (a match may straddle chunks). */
const PATTERN_OVERLAP = 4096;

export type MarkerKind = "A" | "B" | "C" | "D";

export interface Marker {
  kind: MarkerKind;
  /** Offset in the log where it was seen. */
  offset: number;
  /** Order of arrival over the tab's life; unlike `offset`, two markers never share one. */
  seq: number;
  /** `D` only: the command's exit code, when the shell reports one. */
  exit?: number;
}

export type WaitReason = "matched" | "idle" | "command-end" | "marker" | "exit" | "timeout" | "aborted";

export interface WaitResult {
  reason: WaitReason;
  /** The text a pattern matched. */
  match?: string;
  elapsedMs: number;
}

export interface WaitOptions {
  /** Only output after this offset counts (default: now). */
  from?: number;
  pattern?: RegExp;
  /** Return when nothing arrived for this long. */
  idleMs?: number;
  /** For `idleMs`: wait for some output first (default true). */
  needOutput?: boolean;
  /** Return when the command typed after `fromSeq` ends (see `commandSpan`). */
  commandEnd?: boolean;
  /** Return at the first marker of this kind seen since `fromSeq`. */
  marker?: MarkerKind;
  /** Only markers that arrived at or after this sequence number count (default: now, `markerSeq`). */
  fromSeq?: number;
  /**
   * For `pattern`: the command text that was typed. The shell echoes it, and the
   * echo must not satisfy the pattern, so scanning starts on the line after it.
   */
  afterEcho?: string;
  timeoutMs: number;
  signal?: AbortSignal;
}

export interface ReadResult {
  text: string;
  /** Characters between the cursor and the oldest kept text (NFR-3). */
  dropped: number;
  /** The cursor was beyond the log (the log restarted, e.g. after a reload). */
  ahead: boolean;
  cursor: number;
}

type Listener = () => void;

export class OutputLog {
  private text = "";
  /** Offset of `text[0]`. */
  private base = 0;
  private markerList: Marker[] = [];
  private nextSeq = 0;
  private listeners = new Set<Listener>();
  private exitCode: number | null | undefined;
  // Parser state, carried across chunks.
  private mode: "text" | "esc" | "csi" | "osc" | "oscEsc" | "str" | "strEsc" = "text";
  private oscBuffer = "";
  private lastWasCr = false;
  private csiParams = "";

  /** Offset after the last character. */
  get end() {
    return this.base + this.text.length;
  }

  /** Offset of the oldest kept character. */
  get start() {
    return this.base;
  }

  get exited() {
    return this.exitCode !== undefined;
  }

  get markers(): readonly Marker[] {
    return this.markerList;
  }

  /** The sequence number the next marker will get: a snapshot to tell new markers from old ones. */
  get markerSeq() {
    return this.nextSeq;
  }

  /** The shell has shown a marker at all: its integration works in this tab. */
  get hasMarkers() {
    return this.markerList.length > 0;
  }

  /** The text in `[from, to)` (clamped to what is kept). */
  slice(from: number, to = this.end) {
    return this.text.slice(Math.max(0, from - this.base), Math.max(0, to - this.base));
  }

  /** Raw pty output. */
  feed(data: string) {
    if (!data) return;
    let out = "";
    const flush = () => {
      if (out) {
        this.text += out;
        out = "";
      }
    };
    for (let i = 0; i < data.length; i++) {
      const ch = data[i];
      switch (this.mode) {
        case "text": {
          if (ch === "\x1b") this.mode = "esc";
          else if (ch === "\r") this.lastWasCr = true;
          else {
            if (this.lastWasCr) {
              this.lastWasCr = false;
              // A bare carriage return redraws the line: keep it as a line break.
              if (ch !== "\n" && (out.at(-1) ?? this.text.at(-1) ?? "\n") !== "\n") out += "\n";
            }
            if (ch === "\n" || ch === "\t" || ch >= " ") {
              if (ch !== "\x7f") out += ch;
            }
          }
          break;
        }
        case "esc":
          if (ch === "[") {
            this.mode = "csi";
            this.csiParams = "";
          }
          else if (ch === "]") {
            this.mode = "osc";
            this.oscBuffer = "";
          } else if (ch === "P" || ch === "X" || ch === "^" || ch === "_") this.mode = "str";
          else if (ch >= " " && ch <= "/") this.mode = "esc"; // intermediate bytes (ESC ( B …)
          else this.mode = "text";
          break;
        case "csi":
          if (ch >= "@" && ch <= "~") {
            this.mode = "text";
            // Command Prompt moves to the next row with an absolute position
            // (`ESC[11;1H`) where other shells send a line break.
            if ((ch === "H" || ch === "f") && this.movesToNewRow()) {
              const last = out.at(-1) ?? this.text.at(-1);
              if (last !== undefined && last !== "\n") out += "\n";
            }
          } else if (this.csiParams.length < 32) this.csiParams += ch;
          break;
        case "osc":
          if (ch === "\x07") {
            flush();
            this.finishOsc();
          } else if (ch === "\x1b") this.mode = "oscEsc";
          else if (this.oscBuffer.length < 512) this.oscBuffer += ch;
          break;
        case "oscEsc":
          if (ch === "\\") {
            flush();
            this.finishOsc();
          } else {
            // Not a terminator: the sequence was cut short by a new escape.
            this.mode = "esc";
            i--;
          }
          break;
        case "str":
          if (ch === "\x07") this.mode = "text";
          else if (ch === "\x1b") this.mode = "strEsc";
          break;
        case "strEsc":
          this.mode = ch === "\\" ? "text" : "str";
          break;
      }
    }
    flush();
    this.trim();
    this.notify();
  }

  /** The pending cursor position names a row and column 1 (or no column). */
  private movesToNewRow() {
    const [row, column] = this.csiParams.split(";");
    return row !== "" && (column === undefined || column === "" || column === "1");
  }

  private finishOsc() {
    this.mode = "text";
    const body = this.oscBuffer;
    this.oscBuffer = "";
    const match = /^133;([ABCD])(?:;(-?\d+))?/.exec(body);
    if (!match) return;
    const marker: Marker = { kind: match[1] as MarkerKind, offset: this.end, seq: this.nextSeq++ };
    if (match[2] !== undefined) marker.exit = Number(match[2]);
    this.markerList.push(marker);
    if (this.markerList.length > 2000) this.markerList.splice(0, this.markerList.length - 1000);
  }

  private trim() {
    // Never keep more than LOG_MAX; when over, cut down to a little below it.
    if (this.text.length <= LOG_MAX) return;
    const cut = this.text.length - (LOG_MAX - TRIM_SLACK);
    this.text = this.text.slice(cut);
    this.base += cut;
    while (this.markerList.length && this.markerList[0].offset < this.base) this.markerList.shift();
  }

  /** The tab's shell was restarted: waits no longer end at the old exit. */
  reopen() {
    this.exitCode = undefined;
  }

  /** The pty ended. */
  close(code: number | null) {
    this.exitCode = code;
    this.notify();
  }

  /** The exit code once `exited`. */
  get exit() {
    return this.exitCode ?? null;
  }

  /** What was written after `since`; without it, nothing (use `end` as a fresh cursor). */
  read(since: number): ReadResult {
    const ahead = since > this.end;
    const from = ahead ? this.base : Math.max(since, this.base);
    return { text: this.text.slice(from - this.base), dropped: Math.max(0, this.base - since), ahead, cursor: this.end };
  }

  /** First marker of `kind` at or after `offset`. */
  markerAfter(kind: MarkerKind, offset: number) {
    return this.markerList.find((marker) => marker.kind === kind && marker.offset >= offset);
  }

  /**
   * The command typed after the marker snapshot `fromSeq`: where its output
   * starts (`C`) and the `D` that ends it. A shell that reports `C` ends a
   * command only at a `D` after its own `C`, so the end of an earlier command
   * that finishes after `fromSeq` is not taken for it; Command Prompt reports no
   * `C`, so any later `D` counts.
   */
  commandSpan(fromSeq: number): { started?: Marker; done?: Marker } {
    const fresh = this.markerList.filter((marker) => marker.seq >= fromSeq);
    const started = fresh.find((marker) => marker.kind === "C");
    const reportsStart = started !== undefined || this.markerList.some((marker) => marker.kind === "C");
    const done = fresh.find((marker) => marker.kind === "D" && (!reportsStart || (started !== undefined && marker.seq > started.seq)));
    return { started, done };
  }

  /** Where the pattern may start matching: after the echoed command line; `undefined`: not yet. */
  private echoEnd(from: number, echo: string, since: number): number | undefined {
    const region = this.slice(from);
    const at = region.indexOf(echo);
    if (at < 0) return Date.now() - since > ECHO_PATIENCE_MS ? Math.max(from, this.base) : undefined;
    const lineEnd = region.indexOf("\n", at + echo.length);
    return lineEnd < 0 ? undefined : Math.max(from, this.base) + lineEnd + 1;
  }

  private notify() {
    for (const listener of [...this.listeners]) listener();
  }

  /**
   * Wait until the first of: the pattern shows in new output, the output has
   * been quiet for `idleMs`, the command ends, the pty exits, the time runs
   * out, or `signal` aborts. Waiting never blocks anything else (NFR-5).
   */
  wait(options: WaitOptions): Promise<WaitResult> {
    const from = options.from ?? this.end;
    const startedAt = Date.now();
    const needOutput = options.needOutput ?? true;
    const fromSeq = options.fromSeq ?? this.markerSeq;
    return new Promise((resolve) => {
      let timer: ReturnType<typeof setTimeout> | undefined;
      let idleTimer: ReturnType<typeof setTimeout> | undefined;
      let echoTimer: ReturnType<typeof setTimeout> | undefined;
      let scanned = Math.max(from, this.base);
      let done = false;
      const finish = (reason: WaitReason, match?: string) => {
        if (done) return;
        done = true;
        clearTimeout(timer);
        clearTimeout(idleTimer);
        clearTimeout(echoTimer);
        this.listeners.delete(check);
        options.signal?.removeEventListener("abort", onAbort);
        resolve({ reason, match, elapsedMs: Date.now() - startedAt });
      };
      const onAbort = () => finish("aborted");
      const armIdle = () => {
        if (options.idleMs === undefined) return;
        clearTimeout(idleTimer);
        if (needOutput && this.end <= from) return;
        idleTimer = setTimeout(() => finish("idle"), options.idleMs);
      };
      const check = () => {
        if (done) return;
        if (options.pattern) {
          let begin = Math.max(from, this.base, scanned - PATTERN_OVERLAP);
          const echoed = options.afterEcho ? this.echoEnd(from, options.afterEcho, startedAt) : from;
          if (echoed !== undefined) {
            begin = Math.max(begin, echoed);
            const found = options.pattern.exec(this.slice(begin));
            scanned = this.end;
            if (found) return finish("matched", found[0]);
          }
        }
        if (options.marker && this.markerList.some((marker) => marker.kind === options.marker && marker.seq >= fromSeq)) return finish("marker");
        if (options.commandEnd && this.commandSpan(fromSeq).done) return finish("command-end");
        if (this.exited) return finish("exit");
        armIdle();
      };
      if (options.signal?.aborted) return finish("aborted");
      options.signal?.addEventListener("abort", onAbort, { once: true });
      timer = setTimeout(() => finish("timeout"), Math.max(0, options.timeoutMs));
      this.listeners.add(check);
      // Re-test once the echo has had its time, in case no more output comes.
      if (options.afterEcho) echoTimer = setTimeout(check, ECHO_PATIENCE_MS + 50);
      // Output that arrived between `from` and now counts too.
      check();
    });
  }
}

/** The last `max` characters of `text`, with a note when something was cut (NFR-4). */
export function tail(text: string, max = 20_000): string {
  if (text.length <= max) return text;
  const cut = text.length - max;
  const rest = text.slice(cut);
  // Start on a line when one begins soon, so the first line is whole.
  const newline = rest.indexOf("\n");
  const kept = newline >= 0 && newline < 200 ? rest.slice(newline + 1) : rest;
  return `… ${text.length - kept.length} characters cut (showing the last ${kept.length})\n${kept}`;
}
