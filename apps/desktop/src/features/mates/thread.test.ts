import { describe, expect, test } from "bun:test";
import type { Turn } from "@/features/chat";
import { GREETING_PROMPT, dayLabel, isGreeting, isHeartbeat, isLong, isSilent, stepItems, stepsLine, threadRows } from "./thread-rows";

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
    expect(rows.map((r) => r.kind)).toEqual(["day", "time", "bubble", "bubble", "bubble"]);
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

// docs/features/evo-mates/spec.md (FR-34; qa-2026-11-28, A1): a new mate
// speaks first; the prompt that wakes it is not the person's message.
describe("a new mate's greeting", () => {
  test("the wake-up prompt shows no bubble of the person", () => {
    expect(isGreeting(GREETING_PROMPT)).toBe(true);
    const rows = threadRows([turn({ id: "g", prompt: GREETING_PROMPT, parts: [text("Hi, I'm Mai.")] })], false);
    expect(rows.filter((r) => r.kind === "bubble").map((r) => (r.kind === "bubble" ? r.bubble.side : null))).toEqual(["mate"]);
    expect(rows.some((r) => r.kind === "delivered")).toBe(false);
  });
});

// docs/features/evo-mates-ui-v2/spec.md (FR-11, FR-14, FR-17, FR-18).
describe("v2 thread rows", () => {
  const at = (daysAgo: number, hour: number) => {
    const d = new Date();
    d.setDate(d.getDate() - daysAgo);
    d.setHours(hour, 0, 0, 0);
    return d.getTime();
  };

  test("each day opens with its separator, once", () => {
    const rows = threadRows(
      [
        turn({ id: "a", startedAt: at(2, 9), parts: [text("x")] }),
        turn({ id: "b", startedAt: at(2, 10), parts: [text("y")] }),
        turn({ id: "c", startedAt: at(0, 9), parts: [text("z")] }),
      ],
      false,
    );
    expect(rows.filter((r) => r.kind === "day")).toHaveLength(2);
  });

  test("day labels: Today, Yesterday, then the date", () => {
    const now = at(0, 12);
    expect(dayLabel(at(0, 8), now)).toBe("Today");
    expect(dayLabel(at(1, 8), now)).toBe("Yesterday");
    expect(dayLabel(at(3, 8), now)).not.toMatch(/Today|Yesterday/);
  });

  test("a stopped reply ends with a Stopped line", () => {
    const rows = threadRows([turn({ status: "aborted", parts: [text("half a")] })], false);
    expect(rows.at(-1)?.kind).toBe("stopped");
    expect(rows.some((r) => r.kind === "delivered")).toBe(false);
  });

  test("steps carry their turn, for the chip's list", () => {
    const rows = threadRows([turn({ parts: [tool("read"), text("done")] })], false);
    const steps = rows.find((r) => r.kind === "steps");
    expect(steps?.kind === "steps" ? steps.turn.parts.length : 0).toBe(2);
  });

  test("step items say what each step touched, never the tool's name", () => {
    const read = { ...tool("read"), args: { path: "C:\\work\\docs\\notes.md" } };
    const search = { ...tool("web_search"), args: { query: "release checklist" } };
    const denied = { ...tool("edit"), phase: "error" as const, output: "The person denied this", args: { path: "a/b.md" } };
    const odd = tool("sessions_search");
    expect(stepItems({ parts: [read, search, denied, odd, tool("memory")] })).toEqual([
      { key: "read", word: "Read", target: "notes.md", status: "done" },
      { key: "web_search", word: "Searched the web for", target: "“release checklist”", status: "done" },
      { key: "edit", word: "Edited", target: "b.md", status: "denied" },
      { key: "sessions_search", word: "Used a tool", target: null, status: "done" },
    ]);
  });

  test("long messages fold", () => {
    expect(isLong("x".repeat(1201))).toBe(true);
    expect(isLong(Array.from({ length: 17 }, () => "line").join("\n"))).toBe(true);
    expect(isLong("short")).toBe(false);
  });
});

// docs/features/evo-mates-messaging/spec.md (FR-2).
describe("message status", () => {
  test("Sent, Seen while the mate works, Queued while it waits for room, nothing once it replied", () => {
    const word = (rows: ReturnType<typeof threadRows>) => {
      const row = rows.find((r) => r.kind === "delivered");
      return row?.kind === "delivered" ? row.word : null;
    };
    expect(word(threadRows([turn({ status: "done" })], false))).toBe("Sent");
    expect(word(threadRows([turn({ status: "running" })], true))).toBe("Seen");
    expect(word(threadRows([turn({ status: "done" })], false, false, true))).toBe("Queued");
    expect(word(threadRows([turn({ parts: [text("hi")] })], false))).toBeNull();
  });
});

// docs/features/evo-mates-messaging/spec.md (FR-10).
describe("a forwarded message", () => {
  test("shows where it came from, apart from the note", () => {
    const prompt = '<evo-mates-forward from="Writer">\nThe intro is ready.\n</evo-mates-forward>\ncheck this';
    const rows = threadRows([turn({ prompt, parts: [text("On it")] })], false);
    const mine = rows.find((r) => r.kind === "bubble" && r.bubble.side === "me");
    expect(mine?.kind === "bubble" ? [mine.bubble.text, mine.bubble.forwarded?.from] : null).toEqual(["check this", "Writer"]);
  });
});

// docs/features/evo-mates-computer/spec.md (FR-2).
describe("steps at the computer", () => {
  test("browser and app steps read in plain words", () => {
    const step = (name: string, args: Record<string, unknown>) => ({ ...tool(name), args });
    const items = stepItems({
      parts: [
        step("browser_navigate", { url: "https://example.com/" }),
        step("browser_click", { element: "Sign in" }),
        step("browser_type", { element: "Search", text: "evo" }),
        step("browser_screenshot", {}),
        step("computer_app", { app: "Notepad" }),
      ],
    });
    expect(items.map((i) => [i.word, i.target])).toEqual([
      ["Opened", "example.com"],
      ["Clicked", "“Sign in”"],
      ["Typed in", "“Search”"],
      ["Took a screenshot", null],
      ["Used an app", "Notepad"],
    ]);
  });
});
