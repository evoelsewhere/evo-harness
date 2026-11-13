// docs/features/ux-polish/spec.md (FR-5).
import { describe, expect, test } from "bun:test";
import { displayTitle } from "./chat-title";

const day = (ms: number) => new Date(ms).toISOString().slice(5, 10);
const tasks = ["Morning briefing"];

describe("a chat's shown title", () => {
  test("a scheduled run named after its task gets its run date", () => {
    expect(displayTitle({ title: "Morning briefing", scheduled: true, createdAt: Date.UTC(2026, 10, 12, 7) }, tasks, day)).toBe(
      "Morning briefing · 11-12",
    );
  });
  test("a scheduled run the person renamed keeps their title", () => {
    expect(displayTitle({ title: "Release notes", scheduled: true, createdAt: 0 }, tasks, day)).toBe("Release notes");
  });
  test("a run saved before runs were flagged, still named after its task, gets its date too", () => {
    expect(displayTitle({ title: "Morning briefing", scheduled: false, createdAt: Date.UTC(2026, 10, 9) }, tasks, day)).toBe(
      "Morning briefing · 11-09",
    );
  });
  test("any other chat is shown as it is", () => {
    expect(displayTitle({ title: "Fix the build", scheduled: false, createdAt: 0 }, tasks, day)).toBe("Fix the build");
    expect(displayTitle({ title: "Morning briefing", scheduled: false, createdAt: 0 }, null, day)).toBe("Morning briefing");
  });
  test("with the task list not read yet, a scheduled run still gets its date", () => {
    expect(displayTitle({ title: "Morning briefing", scheduled: true, createdAt: Date.UTC(2026, 10, 12) }, null, day)).toBe(
      "Morning briefing · 11-12",
    );
  });
});
