import { describe, expect, test } from "bun:test";
import { CHECKIN_PRESETS, dueWords, scheduleOf } from "./following-up";

// docs/features/evo-mates/spec.md (FR-69, FR-70).
describe("following up", () => {
  test("a check-in's schedule is read for the editor", () => {
    expect(scheduleOf('{"kind":"weekly","time":"10:30","days":[1,3]}')).toEqual({ kind: "weekly", time: "10:30", days: [1, 3] });
    expect(scheduleOf('{"kind":"daily","time":"18:00"}')).toEqual({ kind: "daily", time: "18:00", days: [1] });
    expect(scheduleOf(null)).toEqual({ kind: "weekdays", time: "09:00", days: [1] });
    expect(scheduleOf("not json")).toEqual({ kind: "weekdays", time: "09:00", days: [1] });
  });

  test("due times read relative to today", () => {
    const now = new Date(2026, 10, 26, 12, 0).getTime();
    expect(dueWords(new Date(2026, 10, 26, 15, 30).getTime(), now)).toMatch(/^Today /);
    expect(dueWords(new Date(2026, 10, 27, 9, 0).getTime(), now)).toMatch(/^Tomorrow /);
  });

  // FR-79: the presets of Add check-in.
  test("the check-in presets are a weekday brief, an end of day and a Friday review", () => {
    expect(CHECKIN_PRESETS.map((p) => [p.label, p.schedule])).toEqual([
      ["Morning brief", { kind: "weekdays", time: "09:00" }],
      ["End of day", { kind: "weekdays", time: "17:30" }],
      ["Weekly review", { kind: "weekly", time: "16:00", days: [5] }],
    ]);
    expect(CHECKIN_PRESETS.every((p) => p.text.length > 10)).toBe(true);
  });
});
