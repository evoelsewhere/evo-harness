// Run with `bun test` from apps/desktop. Spec: docs/features/agent-terminal (T5).
import { describe, expect, test } from "bun:test";
import { LOG_MAX, OutputLog, tail } from "./output-log";

describe("OutputLog", () => {
  test("strips escape sequences, also when split across chunks", () => {
    const log = new OutputLog();
    log.feed("\x1b[31mred\x1b[0m \x1b]0;title\x07ok \x1b[");
    log.feed("1;2Hmore\r\nnext\x1b]8;;http://x\x1b\\");
    expect(log.slice(0)).toBe("red ok more\nnext");
  });

  test("cursor returns only new output", () => {
    const log = new OutputLog();
    log.feed("one\n");
    const first = log.read(0);
    expect(first.text).toBe("one\n");
    log.feed("two\n");
    const second = log.read(first.cursor);
    expect(second.text).toBe("two\n");
    expect(second.cursor).toBe(log.end);
    expect(log.read(second.cursor).text).toBe("");
  });

  test("ring drops old text and reports it for a stale cursor", () => {
    const log = new OutputLog();
    const line = `${"x".repeat(1023)}\n`;
    for (let i = 0; i < 1300; i++) log.feed(line);
    expect(log.end).toBe(1300 * 1024);
    expect(log.end - log.start).toBeLessThanOrEqual(LOG_MAX);
    const read = log.read(0);
    expect(read.dropped).toBe(log.start);
    expect(read.text.length).toBe(log.end - log.start);
  });

  test("a cursor beyond the end is flagged", () => {
    const log = new OutputLog();
    log.feed("abc");
    const read = log.read(999);
    expect(read.ahead).toBe(true);
    expect(read.text).toBe("abc");
  });

  test("OSC 133 markers are stored at their offset", () => {
    const log = new OutputLog();
    log.feed("\x1b]133;A\x07$ \x1b]133;B\x07");
    const from = log.end;
    log.feed("ls\r\n\x1b]133;C\x07file\r\n\x1b]133;D;3\x07\x1b]133;A\x1b\\");
    const c = log.markerAfter("C", from)!;
    const d = log.markerAfter("D", from)!;
    expect(d.exit).toBe(3);
    expect(log.slice(c.offset, d.offset)).toBe("file\n");
  });

  test("waits for a pattern in new output only", async () => {
    const log = new OutputLog();
    log.feed("ready on 1\n");
    const waiting = log.wait({ pattern: /ready on (\d+)/, timeoutMs: 1000 });
    setTimeout(() => log.feed("ready on 3000\n"), 20);
    const result = await waiting;
    expect(result.reason).toBe("matched");
    expect(result.match).toBe("ready on 3000");
  });

  test("a pattern split over two chunks matches", async () => {
    const log = new OutputLog();
    const waiting = log.wait({ pattern: /ready on/, timeoutMs: 1000 });
    log.feed("rea");
    setTimeout(() => log.feed("dy on"), 10);
    expect((await waiting).reason).toBe("matched");
  });

  test("idle wait returns after quiet, timeout returns timeout", async () => {
    const log = new OutputLog();
    const idle = log.wait({ idleMs: 30, timeoutMs: 1000 });
    log.feed("a");
    expect((await idle).reason).toBe("idle");
    expect((await log.wait({ pattern: /never/, timeoutMs: 30 })).reason).toBe("timeout");
  });

  test("exit and abort end a wait", async () => {
    const log = new OutputLog();
    const waiting = log.wait({ pattern: /x/, timeoutMs: 1000 });
    log.close(2);
    expect((await waiting).reason).toBe("exit");
    const controller = new AbortController();
    const other = new OutputLog().wait({ pattern: /x/, timeoutMs: 1000, signal: controller.signal });
    controller.abort();
    expect((await other).reason).toBe("aborted");
  });

  test("command-end wait sees the next D marker", async () => {
    const log = new OutputLog();
    const waiting = log.wait({ commandEnd: true, timeoutMs: 1000 });
    log.feed("\x1b]133;C\x07hi\n\x1b]133;D;0\x07");
    expect((await waiting).reason).toBe("command-end");
  });
});

describe("OutputLog commands and markers", () => {
  const C = "\x1b]133;C\x07";
  const D = (code: number) => `\x1b]133;D;${code}\x07`;
  const prompt = "\x1b]133;A\x07$ \x1b]133;B\x07";

  test("a command does not end at the previous command's D, which sits at the same offset", async () => {
    const log = new OutputLog();
    log.feed(prompt + C + "one\n" + D(0));
    // The first call returned at that D. The next one starts right there.
    const from = log.end;
    const fromSeq = log.markerSeq;
    expect(log.markerAfter("D", from)).toBeDefined(); // an offset-based test would call this "done"
    expect(log.commandSpan(fromSeq).done).toBeUndefined();
    const waiting = log.wait({ from, fromSeq, commandEnd: true, timeoutMs: 1000 });
    log.feed(prompt);
    log.feed("(exit 5)\n" + C + D(5));
    expect((await waiting).reason).toBe("command-end");
    expect(log.commandSpan(fromSeq).done?.exit).toBe(5);
  });

  test("with C markers, only a D after the command's own C ends it", () => {
    const log = new OutputLog();
    log.feed(prompt + C + "old\n");
    const fromSeq = log.markerSeq;
    // An earlier command finishes after ours was typed, before ours starts.
    log.feed(D(0) + prompt);
    expect(log.commandSpan(fromSeq).done).toBeUndefined();
    log.feed(C + "mine\n" + D(2));
    const span = log.commandSpan(fromSeq);
    expect(span.done?.exit).toBe(2);
    expect(log.slice(span.started!.offset, span.done!.offset)).toBe("mine\n");
  });

  test("a shell that reports no C (Command Prompt) ends at any later D", () => {
    const log = new OutputLog();
    log.feed("\x1b]133;D\x07\x1b]133;A\x07C:\\> \x1b]133;B\x07");
    const fromSeq = log.markerSeq;
    log.feed("hi\n\x1b]133;D\x07\x1b]133;A\x07");
    const span = log.commandSpan(fromSeq);
    expect(span.done).toBeDefined();
    expect(span.done?.exit).toBeUndefined();
    expect(span.started).toBeUndefined();
  });

  test("Command Prompt's move to the next row is a line break", () => {
    const log = new OutputLog();
    log.feed("echo again\r\n");
    log.feed("\x1b[?25lagain\x1b[10;1H\x1b[?25h\x1b]133;D\x1b\\\x1b]133;A\x1b\\C:\\> \x1b]133;B\x1b\\");
    expect(log.slice(0)).toBe("echo again\nagain\nC:\\> ");
    // A move to a column past 1, to the home position, or at a line start adds nothing.
    const other = new OutputLog();
    other.feed("ab\x1b[3;5Hcd\x1b[Hef\ngh\x1b[7;1Hij");
    expect(other.slice(0)).toBe("abcdef\ngh\nij");
  });

  test("waits for the first prompt end", async () => {
    const log = new OutputLog();
    const waiting = log.wait({ marker: "B", fromSeq: 0, timeoutMs: 1000 });
    log.feed("banner\n\x1b]133;A\x07");
    setTimeout(() => log.feed("$ \x1b]133;B\x07"), 20);
    expect((await waiting).reason).toBe("marker");
  });

  test("the typed command's echo does not satisfy a pattern", async () => {
    const log = new OutputLog();
    const from = log.end;
    const waiting = log.wait({ from, pattern: /ready on/, afterEcho: "node server --msg 'ready on'", timeoutMs: 1000 });
    log.feed("node server --msg 'ready on'");
    log.feed("\r\n");
    setTimeout(() => log.feed("starting\nready on 3000\n"), 30);
    const result = await waiting;
    expect(result.reason).toBe("matched");
    expect(result.elapsedMs).toBeGreaterThanOrEqual(25);
  });

  test("without a recognisable echo the pattern still matches after a while", async () => {
    const log = new OutputLog();
    const waiting = log.wait({ from: 0, pattern: /done/, afterEcho: "echo not-in-output", timeoutMs: 4000 });
    log.feed("something done\n");
    expect((await waiting).reason).toBe("matched");
  });
});

describe("tail", () => {
  test("keeps short text, cuts long text with a note", () => {
    expect(tail("short")).toBe("short");
    const long = Array.from({ length: 5000 }, (_, i) => `line ${i}`).join("\n");
    const cut = tail(long, 20_000);
    expect(cut.startsWith("… ")).toBe(true);
    expect(cut.length).toBeLessThan(20_200);
    expect(cut.endsWith("line 4999")).toBe(true);
  });
});
