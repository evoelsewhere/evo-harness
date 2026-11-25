import { describe, expect, test } from "bun:test";
import type { Turn } from "@/features/chat";
import { stepsLine, threadRows } from "./thread-rows";

const turn = (over: Partial<Turn>): Turn =>
  ({
    id: "t",
    prompt: "hi",
    parts: [],
    status: "done",
    startedAt: 1_000,
    meta: {},
    ...over,
  }) as Turn;

const text = (t: string) => ({ kind: "text" as const, key: t, text: t, streaming: false });

// docs/features/evo-mates/spec.md (FR-58, FR-59).
describe("mate thread rows", () => {
  test("a turn is the person's bubble, then the mate's, with one time line", () => {
    const rows = threadRows([turn({ id: "a", parts: [text("hello"), text("how can I help?")] })], false);
    expect(rows.map((r) => r.kind)).toEqual(["time", "bubble", "bubble", "bubble"]);
    const bubbles = rows.filter((r) => r.kind === "bubble");
    expect(bubbles.map((r) => (r.kind === "bubble" ? [r.bubble.side, r.first, r.last] : null))).toEqual([
      ["me", true, true],
      ["mate", true, false],
      ["mate", false, true],
    ]);
  });

  test("a gap of more than 15 minutes adds a time line", () => {
    const rows = threadRows(
      [turn({ id: "a", startedAt: 0, parts: [text("x")] }), turn({ id: "b", startedAt: 16 * 60_000, parts: [text("y")] })],
      false,
    );
    expect(rows.filter((r) => r.kind === "time")).toHaveLength(2);
  });

  test("a working turn with no text yet shows typing", () => {
    const rows = threadRows([turn({ status: "running" })], true);
    expect(rows.at(-1)?.kind).toBe("typing");
  });

  test("a failed attempt the harness retries shows typing after the error", () => {
    const rows = threadRows([turn({ status: "error", error: "Could not reach the provider" })], false, true);
    expect(rows.map((r) => r.kind).slice(-2)).toEqual(["error", "typing"]);
    const last = rows.at(-1);
    expect(last?.kind === "typing" ? last.step : null).toBe("Trying again…");
  });

  test("a turn the engine retries says which try it is", () => {
    const rows = threadRows([turn({ status: "running", retry: { attempt: 2, maxRetries: 10, delayMs: 4000, error: "408" } })], true);
    const last = rows.at(-1);
    expect(last?.kind === "typing" ? last.step : null).toBe("Trying again (2 of 10)…");
  });

  test("steps read in plain words", () => {
    const tool = (name: string) => ({ kind: "tool" as const, key: name, callId: name, name, args: {}, argsText: "", phase: "done" as const, output: "", startedAt: 0 });
    expect(stepsLine({ parts: [tool("read"), tool("read"), tool("web_search")] })).toBe("Read 2 files · Searched the web");
    expect(stepsLine({ parts: [tool("read")] })).toBe("Read 1 file");
    expect(stepsLine({ parts: [tool("memory")] })).toBeNull();
  });
});
