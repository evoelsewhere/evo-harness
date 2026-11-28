import { describe, expect, test } from "bun:test";
import { panelDocks, placeKey, railForced, sidebarForm, sidebarOrder, stepPlace } from "./sidebar-layout";

// docs/features/evo-mates-ui-v2/spec.md (FR-1, FR-8, FR-31).
describe("the window's layout", () => {
  test("the sidebar is a rail under 760 px or when folded", () => {
    expect(sidebarForm(1280, false)).toBe("full");
    expect(sidebarForm(1280, true)).toBe("rail");
    expect(sidebarForm(720, false)).toBe("rail");
    expect(railForced(720)).toBe(true);
    expect(railForced(760)).toBe(false);
  });

  test("the profile docks from 1100 px", () => {
    expect(panelDocks(1440)).toBe(true);
    expect(panelDocks(1100)).toBe(true);
    expect(panelDocks(1000)).toBe(false);
  });
});

describe("walking the sidebar", () => {
  const mates = [
    { id: "a", chatId: "ca", pinned: false, hidden: false },
    { id: "b", chatId: "cb", pinned: true, hidden: false },
    { id: "h", chatId: "ch", pinned: false, hidden: true },
  ];
  const channels = [{ id: "x", chatId: "cx", hidden: false }];
  const order = sidebarOrder(mates, channels);

  test("Today, Files, pinned, mates, channels; hidden left out", () => {
    expect(order.map(placeKey)).toEqual(["today", "files", "mate:b", "mate:a", "channel:x"]);
  });

  test("Alt+↓ and Alt+↑ wrap round", () => {
    expect(placeKey(stepPlace(order, "mate:b", 1)!)).toBe("mate:a");
    expect(placeKey(stepPlace(order, "channel:x", 1)!)).toBe("today");
    expect(placeKey(stepPlace(order, "today", -1)!)).toBe("channel:x");
    expect(placeKey(stepPlace(order, null, 1)!)).toBe("today");
  });

  test("Alt+Shift skips to the next unread or waiting place", () => {
    const attention = (chat: string) => chat === "cx" || chat === "cb";
    expect(placeKey(stepPlace(order, "today", 1, attention)!)).toBe("mate:b");
    expect(placeKey(stepPlace(order, "mate:b", 1, attention)!)).toBe("channel:x");
    expect(placeKey(stepPlace(order, "channel:x", 1, attention)!)).toBe("mate:b");
    expect(stepPlace(order, "today", 1, () => false)).toBeNull();
  });
});
