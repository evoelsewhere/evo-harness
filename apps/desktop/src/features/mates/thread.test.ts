import { describe, expect, test } from "bun:test";
import type { Turn } from "@/features/chat";
import { isHeartbeat, isSilent, stepsLine, threadRows } from "./thread-rows";

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

const tool = (name: string) => ({ kind: "tool" as const, key: name, callId: name, name, args: {}, argsText: "", phase: "done" as const, output: "", startedAt: 0 });
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
    expect(stepsLine({ parts: [tool("read"), tool("read"), tool("web_search")] })).toBe("Read 2 files · Searched the web");
    expect(stepsLine({ parts: [tool("read")] })).toBe("Read 1 file");
    expect(stepsLine({ parts: [tool("memory")] })).toBeNull();
  });

  // Found playing a careful user (qa-2026-11-28): after Deny the line said
  // "Edited 1 file", and a follow-up noted read "Used 3 tools".
  test("a step that was denied or failed is not counted as done", () => {
    const denied = { ...tool("write"), phase: "error" as const };
    expect(stepsLine({ parts: [denied] })).toBeNull();
    expect(stepsLine({ parts: [denied, tool("read")] })).toBe("Read 1 file");
  });

  test("a mate's own steps read in plain words", () => {
    expect(stepsLine({ parts: [tool("mate_agenda"), tool("mate_agenda")] })).toBe("Noted to follow up");
    expect(stepsLine({ parts: [tool("sessions_search")] })).toBe("Used 1 tool");
  });

  test("a working step with no plain word reads as working", () => {
    const running = { ...tool("some_tool"), phase: "running" as const };
    const rows = threadRows([turn({ status: "running", parts: [running] })], true);
    const last = rows.at(-1);
    expect(last?.kind === "typing" ? last.step : null).toBe("Working…");
  });
});

// docs/features/evo-mates/spec.md (FR-64, FR-66).
describe("heartbeat turns", () => {
  const beat = "<evo-mates-heartbeat at=\"2026-11-26T11:00\" items=\"a\">\nDue now…\n</evo-mates-heartbeat>";

  test("a silent heartbeat turn shows nothing", () => {
    const rows = threadRows([turn({ id: "h", prompt: beat, parts: [text("NOTHING_TO_SAY")] })], false);
    expect(rows).toEqual([]);
  });

  test("a first message shows as the mate's, without the prompt", () => {
    const rows = threadRows([turn({ id: "h", prompt: beat, parts: [text("You had the interview this morning: how did it go?")] })], false);
    const bubbles = rows.filter((r) => r.kind === "bubble");
    expect(bubbles.map((r) => (r.kind === "bubble" ? r.bubble.side : null))).toEqual(["mate"]);
  });

  test("silence is read loosely", () => {
    expect(isSilent(" `nothing_to_say`. ")).toBe(true);
    expect(isSilent("How was it?")).toBe(false);
    expect(isHeartbeat(beat)).toBe(true);
    expect(isHeartbeat("hello")).toBe(false);
  });
});
