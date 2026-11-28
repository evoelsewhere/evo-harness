import { describe, expect, test } from "bun:test";
import { greeting, todaySections, waitingKind, type Place } from "./today-sections";

// docs/features/evo-mates/spec.md (FR-73).
const place = (id: string, kind: Place["kind"] = "mate", hidden = false): Place => ({ kind, id, name: id, chatId: `chat-${id}`, hidden });

describe("today", () => {
  const places = [place("ada"), place("bo"), place("cy"), place("launch", "channel"), place("ghost", "mate", true)];
  const rows = [
    { id: "chat-ada", unread: true, preview: "Here is the draft", updatedAt: 10 },
    { id: "chat-launch", unread: true, preview: "Writer: done", updatedAt: 30 },
    { id: "chat-bo", unread: true, preview: "<evo-mates-heartbeat at=\"x\">", updatedAt: 40 },
    { id: "chat-cy", unread: false, preview: "Old", updatedAt: 50 },
    { id: "chat-ghost", unread: true, preview: "Hidden", updatedAt: 60 },
  ];
  const sections = todaySections({
    places,
    rows,
    work: { "chat-cy": "waiting", "chat-ada": "working", "chat-ghost": "waiting" },
    coming: [
      { id: "i2", mateId: "bo", kind: "checkin", text: "Brief", dueAt: 200, repeat: null, status: "open", chatId: null, createdAt: 0, firedAt: null },
      { id: "i1", mateId: "ada", kind: "event", text: "Interview", dueAt: 100, repeat: null, status: "open", chatId: null, createdAt: 0, firedAt: null },
      { id: "i3", mateId: "ghost", kind: "event", text: "Gone", dueAt: 50, repeat: null, status: "open", chatId: null, createdAt: 0, firedAt: null },
    ],
    team: [
      { mateId: "ada", replies: 2, costUsd: 0.1 },
      { mateId: "bo", replies: 5, costUsd: 0.3 },
      { mateId: "deleted", replies: 9, costUsd: 1 },
    ],
  });

  test("needs you lists waiting places, never hidden ones", () => {
    expect(sections.needsYou.map((p) => p.id)).toEqual(["cy"]);
  });

  test("new lists unread places newest first, with a shown line", () => {
    expect(sections.fresh.map((f) => [f.place.id, f.line])).toEqual([
      ["bo", ""],
      ["launch", "Writer: done"],
      ["ada", "Here is the draft"],
    ]);
  });

  test("coming up is in time order with its mate", () => {
    expect(sections.coming.map((c) => [c.item.id, c.mate.id])).toEqual([
      ["i1", "ada"],
      ["i2", "bo"],
    ]);
  });

  test("the team's day lists known mates, busiest first", () => {
    expect(sections.team.map((t) => [t.mate.id, t.replies])).toEqual([
      ["bo", 5],
      ["ada", 2],
    ]);
    expect(sections.totalCost).toBeCloseTo(0.4);
  });
});

// docs/features/evo-mates-ui-v2/spec.md (FR-25, FR-26).
describe("today v2", () => {
  const p = (id: string, kind: Place["kind"] = "mate", seats: string[] = []): Place => ({ kind, id, name: id, chatId: `chat-${id}`, hidden: false, seats });
  const places = [p("ava", "mate", ["seat-ava"]), p("kai", "mate", ["seat-kai"]), p("lin"), p("launch", "channel", ["seat-ava", "seat-kai"])];

  test("a channel whose seat waits needs you; oldest first", () => {
    const s = todaySections({
      places,
      rows: [],
      work: { "seat-kai": "waiting", "chat-lin": "waiting" },
      coming: [],
      team: [],
      waitingSince: { "seat-kai": 500, "chat-lin": 100 },
    });
    expect(s.needsYou.map((x) => x.id)).toEqual(["lin", "kai", "launch"]);
  });

  test("working now counts mates working anywhere", () => {
    const s = todaySections({ places, rows: [], work: { "seat-ava": "working", "chat-lin": "working" }, coming: [], team: [] });
    expect(s.working.map((x) => x.id)).toEqual(["ava", "lin"]);
  });

  test("what a place waits on, in plain words", () => {
    expect(waitingKind([{ kind: "permission", title: "Edit notes.md" }])).toBe("Approval");
    expect(waitingKind([{ kind: "questions", title: "A few questions" }])).toBe("Question");
    expect(waitingKind([{ kind: "choice", title: "Enter a secret for GitHub" }])).toBe("Secret");
    expect(waitingKind([{ kind: "choice", title: "Connect Gmail" }])).toBe("Sign-in");
    expect(waitingKind([])).toBe("Waiting for you");
  });

  test("greeting by the time of day", () => {
    const at = (h: number) => new Date(2026, 10, 28, h);
    expect(greeting(at(8))).toBe("Good morning");
    expect(greeting(at(14))).toBe("Good afternoon");
    expect(greeting(at(20))).toBe("Good evening");
  });
});
