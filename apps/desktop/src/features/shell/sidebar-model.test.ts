// docs/features/workspace-rail/spec.md (FR-2, FR-7, FR-9, FR-10; NFR-2).
import { describe, expect, test } from "bun:test";
import { foldRuns, groupByTime, initials, needsYou, rollUp, timeGroup, type ChatState } from "./sidebar-model";

const DAY = 86_400_000;
// Noon, local time, so "yesterday" is a whole day back whatever the zone.
const now = new Date(2026, 10, 23, 12, 0, 0).getTime();

interface Chat {
  id: string;
  title: string;
  updatedAt: number;
  unread?: boolean;
  draft?: boolean;
  parentChatId?: string | null;
  scheduled?: boolean;
}
const chat = (id: string, extra: Partial<Chat> = {}): Chat => ({ id, title: id, updatedAt: now, ...extra });

describe("initials", () => {
  test("first letters of the first two words", () => {
    expect(initials("evo-harness")).toBe("EH");
    expect(initials("teams_check")).toBe("TC");
    expect(initials("my project folder")).toBe("MP");
    expect(initials("evoWorkspace")).toBe("EW");
  });
  test("one word: its first two letters", () => {
    expect(initials("harness")).toBe("HA");
    expect(initials("x")).toBe("X");
  });
  test("skips leading punctuation; empty stays empty", () => {
    expect(initials(".dotfiles")).toBe("DO");
    expect(initials("")).toBe("");
  });
});

describe("timeGroup", () => {
  test("today, yesterday, previous 7 days, older by calendar day", () => {
    const midnight = new Date(2026, 10, 23).getTime();
    expect(timeGroup(now, now)).toBe("today");
    expect(timeGroup(midnight, now)).toBe("today");
    expect(timeGroup(midnight - 1, now)).toBe("yesterday");
    expect(timeGroup(now - 2 * DAY, now)).toBe("week");
    expect(timeGroup(midnight - 7 * DAY, now)).toBe("week");
    expect(timeGroup(midnight - 7 * DAY - 1, now)).toBe("older");
  });
  test("a time in the future counts as today", () => {
    expect(timeGroup(now + DAY, now)).toBe("today");
  });
  test("groupByTime keeps order and drops empty groups", () => {
    const chats = [chat("a"), chat("b", { updatedAt: now - 30 * DAY }), chat("c", { updatedAt: now - 3 * DAY })];
    const groups = groupByTime(chats, (c) => c.updatedAt, now);
    expect(groups.map((g) => [g.group, g.items.map((c) => c.id)])).toEqual([
      ["today", ["a"]],
      ["week", ["c"]],
      ["older", ["b"]],
    ]);
  });
});

describe("foldRuns", () => {
  const isRun = (c: Chat) => Boolean(c.scheduled);
  test("runs of one task fold under the newest, in its place", () => {
    const chats = [
      chat("brief-3", { title: "Morning briefing", scheduled: true }),
      chat("x"),
      chat("brief-2", { title: "Morning briefing", scheduled: true }),
      chat("other-run", { title: "Weekly report", scheduled: true }),
      chat("brief-1", { title: "Morning briefing", scheduled: true }),
    ];
    const folded = foldRuns(chats, isRun);
    expect(folded.map((f) => [f.chat.id, f.runs.map((r) => r.id)])).toEqual([
      ["brief-3", ["brief-2", "brief-1"]],
      ["x", []],
      ["other-run", []],
    ]);
  });
  test("a chat that is not a run never folds, even with the same title", () => {
    const chats = [chat("a", { title: "Same" }), chat("b", { title: "Same" })];
    expect(foldRuns(chats, isRun).map((f) => f.runs.length)).toEqual([0, 0]);
  });
});

describe("needsYou", () => {
  test("waiting, then unread, then running; newest first in each", () => {
    const chats = [
      chat("idle"),
      chat("run-old", { updatedAt: now - 10 }),
      chat("run-new"),
      chat("unread", { unread: true }),
      chat("wait", { updatedAt: now - 100 }),
      chat("draft", { draft: true }),
    ];
    const states: Record<string, ChatState> = { "run-old": "running", "run-new": "running", wait: "waiting", draft: "running" };
    expect(needsYou(chats, states).map((c) => c.id)).toEqual(["wait", "unread", "run-new", "run-old"]);
  });
  test("a nested session shows only while it waits", () => {
    const chats = [
      chat("child-wait", { parentChatId: "p" }),
      chat("child-run", { parentChatId: "p" }),
      chat("child-unread", { parentChatId: "p", unread: true }),
    ];
    const states: Record<string, ChatState> = { "child-wait": "waiting", "child-run": "running" };
    expect(needsYou(chats, states).map((c) => c.id)).toEqual(["child-wait"]);
  });
  test("a waiting chat that is also unread counts once, as waiting", () => {
    const chats = [chat("a", { unread: true })];
    expect(needsYou(chats, { a: "waiting" }).map((c) => c.id)).toEqual(["a"]);
  });
});

describe("rollUp", () => {
  test("counts waiting, running and unread chats, nested ones included", () => {
    const chats = [
      chat("a"),
      chat("b", { unread: true }),
      chat("c", { parentChatId: "a" }),
      chat("d", { unread: true }),
    ];
    expect(rollUp(chats, { a: "running", c: "waiting", d: "running" })).toEqual({ waiting: 1, running: 2, unread: 2 });
  });
});
