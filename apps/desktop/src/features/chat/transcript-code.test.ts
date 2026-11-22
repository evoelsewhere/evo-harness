// The Code row of a `run_code` call (docs/features/code-mode/spec.md, FR-15):
// its nested calls from the result and from live progress, its status after
// a restart, and the reducer keeping progress while it runs.
import { describe, expect, test } from "bun:test";
import { codeCalls, codeRunMeta, codeRunSummary, withLiveCall } from "@/components/ai-elements/code-run-model";
import { ENGINE_EXITED, initialTranscript, transcriptFromMessages, transcriptReducer, type ToolPart, type TranscriptAction, type TranscriptState } from "./transcript";

const engine = (message: Record<string, any>): TranscriptAction => ({ type: "engine", message: message as any });
const run = (actions: TranscriptAction[], from: TranscriptState = initialTranscript) => actions.reduce(transcriptReducer, from);

const part = (extra: Partial<ToolPart> = {}): ToolPart => ({
  kind: "tool",
  key: "call:c1",
  callId: "c1",
  name: "run_code",
  args: { code: "await tools.read({ path: 'a.txt' })", description: "Rename the option" },
  argsText: "",
  phase: "done",
  output: "ok",
  startedAt: 1000,
  endedAt: 2200,
  ...extra,
});

const finished = {
  code: "await tools.read({ path: 'a.txt' })",
  result: "ok",
  toolsUsed: ["read", "edit", "bash"],
  calls: [
    { name: "read", ok: true, ms: 5, args: { path: "a.txt" }, output: "one", details: null },
    { name: "edit", ok: true, ms: 9, args: { path: "a.txt", edits: [{ oldText: "a", newText: "b" }] }, output: "Edited", details: { diff: "-a\n+b" } },
    { name: "bash", ok: false, ms: 40, args: { command: "npm test" }, output: "Command exited with code 1", details: null },
  ],
};

describe("code row", () => {
  test("a finished program's calls become ordinary tool parts, in order", () => {
    const calls = codeCalls(part({ details: finished }));
    expect(calls.map((call) => [call.name, call.phase])).toEqual([
      ["read", "done"],
      ["edit", "done"],
      ["bash", "error"],
    ]);
    expect(calls[0].args).toEqual({ path: "a.txt" });
    expect(calls[1].details).toEqual({ diff: "-a\n+b" });
    expect(calls[2].output).toBe("Command exited with code 1");
    expect(calls[2].endedAt).toBe(1040);
    expect(new Set(calls.map((call) => call.key)).size).toBe(3);
  });

  test("the summary says what the program was for, its calls and how long it took", () => {
    const summary = codeRunSummary(part({ details: finished }));
    expect(summary).toMatchObject({ description: "Rename the option", status: "done", calls: 3, failed: 1, durationMs: 1200 });
    expect(codeRunMeta(summary)).toBe("3 calls · 1 failed · 1.2 s");
  });

  test("failed, stopped and interrupted runs", () => {
    expect(codeRunSummary(part({ phase: "error", details: { ...finished, error: "Error: boom", line: 3 } })).status).toBe("failed");
    expect(codeRunSummary(part({ phase: "error", details: { ...finished, error: "The user stopped the code." } })).status).toBe("stopped");
    expect(codeRunSummary(part({ phase: "error", details: { ...finished, error: "The program was stopped.", limit: "cancelled" } })).status).toBe(
      "stopped",
    );
    const interrupted = codeRunSummary(part({ phase: "error", output: ENGINE_EXITED, details: undefined }));
    expect(interrupted.status).toBe("interrupted");
    expect(codeRunMeta(interrupted)).toBe("1.2 s · interrupted");
  });

  test("while it runs, progress updates show each call's latest phase", () => {
    let details: ToolPart["details"] = undefined;
    details = withLiveCall(details, { toolProgram: { requestId: "tp-1", tool: "read", phase: "started" } });
    details = withLiveCall(details, { toolProgram: { requestId: "tp-2", tool: "bash", phase: "started" } });
    details = withLiveCall(details, { toolProgram: { requestId: "tp-1", tool: "read", phase: "completed" } });
    details = withLiveCall(details, { other: true });
    const calls = codeCalls(part({ phase: "running", endedAt: undefined, details }));
    expect(calls.map((call) => [call.name, call.phase])).toEqual([
      ["read", "done"],
      ["bash", "running"],
    ]);
    expect(codeRunSummary(part({ phase: "running", endedAt: undefined, details })).status).toBe("running");
  });

  test("the reducer keeps progress while the program runs and the result once it ends", () => {
    const state = run([
      { type: "prompt", text: "go" } as TranscriptAction,
      engine({ type: "tool_execution_start", toolCallId: "c1", toolName: "run_code", args: { code: "1", description: "Count" } }),
      engine({ type: "tool_execution_update", toolCallId: "c1", partialResult: { details: { toolProgram: { requestId: "tp-1", tool: "read", phase: "started" } } } }),
      engine({ type: "tool_execution_update", toolCallId: "c1", partialResult: { details: { toolProgram: { requestId: "tp-2", tool: "edit", phase: "started" } } } }),
    ]);
    const running = state.turns.at(-1)!.parts.find((p): p is ToolPart => p.kind === "tool")!;
    expect(codeCalls(running).map((call) => call.name)).toEqual(["read", "edit"]);
    const ended = run(
      [engine({ type: "tool_execution_end", toolCallId: "c1", isError: false, result: { content: [{ type: "text", text: "ok" }], details: finished } })],
      state,
    );
    const done = ended.turns.at(-1)!.parts.find((p): p is ToolPart => p.kind === "tool")!;
    expect(codeCalls(done).map((call) => call.name)).toEqual(["read", "edit", "bash"]);
    expect(done.details?.live).toBeUndefined();
  });

  test("a reopened chat rebuilds the row and its calls from the saved messages", () => {
    const saved: any[] = [
      { role: "user", content: "rename it", timestamp: 10 },
      { role: "assistant", content: [{ type: "toolCall", id: "c1", name: "run_code", arguments: { code: "1", description: "Rename the option" } }], timestamp: 11 },
      { role: "toolResult", toolCallId: "c1", toolName: "run_code", content: [{ type: "text", text: "ok" }], details: finished, isError: false, timestamp: 12 },
    ];
    const state = transcriptFromMessages(saved, {});
    const rebuilt = state.turns.flatMap((turn) => turn.parts).find((p): p is ToolPart => p.kind === "tool")!;
    expect(rebuilt.name).toBe("run_code");
    expect(codeRunSummary(rebuilt)).toMatchObject({ description: "Rename the option", status: "done", calls: 3 });
    expect(codeCalls(rebuilt).map((call) => call.name)).toEqual(["read", "edit", "bash"]);
  });
});
