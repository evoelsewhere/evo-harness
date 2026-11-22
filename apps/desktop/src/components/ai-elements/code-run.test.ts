// The Code row drawn by React (docs/features/code-mode/spec.md, FR-15): the
// line names the program, its calls and time; a stopped run opens on its
// code, its calls as their usual cards, and the error.
import { describe, expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { type ToolPart } from "@/features/chat/transcript";
import { CodeRun } from "./code-run";

const calls = [
  { name: "bash", ok: true, ms: 41, args: { command: "rg -l retryPolicy src" }, output: "src/net/fetch.ts", details: null },
  { name: "edit", ok: true, ms: 12, args: { path: "src/net/fetch.ts", edits: [{ oldText: "a", newText: "b" }] }, output: "Edited", details: { diff: " 12 x\n-13 a\n+13 b" } },
  { name: "bash", ok: false, ms: 5, args: { command: "git push" }, output: "The user stopped the code at this command.", details: null },
];

const part = (extra: Partial<ToolPart> = {}): ToolPart => ({
  kind: "tool",
  key: "call:k1",
  callId: "k1",
  name: "run_code",
  args: { description: "Rename retryPolicy", code: "const files = await $`rg -l retryPolicy src`.lines();" },
  argsText: "",
  phase: "done",
  output: '{"edited":1}',
  details: { calls, toolsUsed: ["bash", "edit"] },
  startedAt: 0,
  endedAt: 2400,
  ...extra,
});

describe("CodeRun", () => {
  test("the line names the program, its calls and its time", () => {
    const html = renderToStaticMarkup(createElement(CodeRun, { part: part(), streaming: false }));
    expect(html).toContain("Rename retryPolicy");
    expect(html).toContain("Code · 3 calls · 1 failed · 2.4 s");
  });

  test("a stopped run opens on its code, its calls as cards and the error", () => {
    const html = renderToStaticMarkup(
      createElement(CodeRun, {
        part: part({ phase: "error", output: "The user stopped the code.", details: { calls, error: "The user stopped the code." } }),
        streaming: false,
      }),
    );
    expect(html).toContain("stopped");
    expect(html).toContain("code.js");
    expect(html).toContain("rg -l retryPolicy src");
    expect(html).toContain('aria-label="Calls from code"');
    expect(html).toContain("src/net/fetch.ts");
    expect(html).toContain("git push");
    expect(html).toContain("The user stopped the code.");
  });

  test("while the model writes it, only the line shows", () => {
    const html = renderToStaticMarkup(createElement(CodeRun, { part: part({ phase: "preparing", details: undefined, endedAt: undefined }), streaming: true }));
    expect(html).toContain("Rename retryPolicy");
    expect(html).not.toContain("code.js");
  });
});
