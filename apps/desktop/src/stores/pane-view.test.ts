// docs/features/chat-split-view/spec.md (FR-3; T3).
import { describe, expect, test } from "bun:test";
import { paneView } from "./pane-view";

interface Runtime {
  status: string;
}
interface Store {
  activeId: string | null;
  runtimes: Record<string, Runtime | undefined>;
  status: string;
  other: number;
}
const EMPTY_STATE = {};
const mirror = (rt: Runtime | undefined) => ({ status: rt?.status ?? "idle", state: EMPTY_STATE });

describe("a pane's view of the engine store", () => {
  const store: Store = { activeId: "a", runtimes: { a: { status: "ready" }, b: { status: "starting" } }, status: "ready", other: 1 };

  test("the active chat's pane sees the store itself", () => {
    expect(paneView(store, "a", mirror)).toBe(store);
  });

  test("another chat's pane sees that chat as active, with its runtime mirrored", () => {
    const view = paneView(store, "b", mirror);
    expect(view.activeId).toBe("b");
    expect(view.status).toBe("starting");
    expect(view.other).toBe(1);
  });

  test("one view per store snapshot and chat, so selectors stay stable", () => {
    expect(paneView(store, "b", mirror)).toBe(paneView(store, "b", mirror));
    const next = { ...store, other: 2 };
    expect(paneView(next, "b", mirror)).not.toBe(paneView(store, "b", mirror));
    expect(paneView(next, "b", mirror).other).toBe(2);
  });

  test("a chat with no runtime here reads as idle", () => {
    expect(paneView(store, "c", mirror).status).toBe("idle");
  });
});
