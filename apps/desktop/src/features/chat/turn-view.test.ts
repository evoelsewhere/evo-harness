// docs/features/quick-entry/spec.md (FR-13; tasks T17): a step in the quick
// entry box is one line.
import { describe, expect, test } from "bun:test";
import type { ReasoningPart, ToolPart } from "./transcript";
import { stepLine } from "./turn-view";

const tool = (extra: Partial<ToolPart>): ToolPart => ({
  kind: "tool",
  key: "k",
  callId: "c",
  name: "bash",
  args: { command: "bun test" },
  argsText: "",
  phase: "done",
  output: "",
  ...extra,
});

const reasoning = (streaming: boolean): ReasoningPart => ({ kind: "reasoning", key: "r", text: "hm", streaming, startedAt: 0 });

describe("a step as one line", () => {
  test("a finished tool call reads as its own line, without a state", () => {
    const line = stepLine(tool({}), false);
    expect(line?.label.length).toBeGreaterThan(0);
    expect(line?.status).toBeUndefined();
  });

  test("a running call is running, a failed one is an error", () => {
    expect(stepLine(tool({ phase: "running" }), true)?.status).toBe("running");
    expect(stepLine(tool({ phase: "error", output: "exit code 1" }), false)?.status).toBe("error");
  });

  test("a call left running when the turn ended reads as stopped, not running", () => {
    expect(stepLine(tool({ phase: "running" }), false)?.status).toBeUndefined();
  });

  test("thinking shows only while it streams", () => {
    expect(stepLine(reasoning(true), true)).toEqual({ label: "Thinking", status: "running" });
    expect(stepLine(reasoning(false), false)).toBeNull();
  });
});
