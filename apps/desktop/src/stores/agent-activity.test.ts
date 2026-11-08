// Run with `bun test` from apps/desktop. Spec: docs/features/floating-cards (FR-2).
import { beforeEach, describe, expect, test } from "bun:test";

class MemoryStorage {
  private data = new Map<string, string>();
  getItem = (key: string) => this.data.get(key) ?? null;
  setItem = (key: string, value: string) => void this.data.set(key, value);
  removeItem = (key: string) => void this.data.delete(key);
  clear = () => this.data.clear();
}
for (const name of ["localStorage", "sessionStorage"]) {
  Object.defineProperty(globalThis, name, { value: new MemoryStorage(), configurable: true, writable: true });
}

const { useCardsStore } = await import("./cards-store");
const { applyActivityEvent } = await import("./agent-events");

const card = (key: string) => useCardsStore.getState().cards.a?.find((c) => c.key === key);
const event = (phase: "start" | "end", tab: string | null, tool: string, requestId: string, args: Record<string, unknown> = {}) => ({
  chat: "a",
  kind: "browser" as const,
  tool,
  phase,
  tab,
  requestId,
  args,
});

describe("what a card says the agent is doing", () => {
  beforeEach(() => useCardsStore.setState({ cards: {} }));

  test("a call marks its page busy and says what it does; the end clears the busy mark", () => {
    applyActivityEvent(event("start", "browser-1", "browser_click", "r1", { ref: "ref_4" }));
    expect(card("browser:browser-1")).toMatchObject({ activity: "Clicking ref_4", busy: true });
    applyActivityEvent(event("end", "browser-1", "browser_click", "r1", { ref: "ref_4" }));
    expect(card("browser:browser-1")).toMatchObject({ activity: "Clicking ref_4", busy: false });
  });

  test("a call that opens another page does not leave the page it started on busy", () => {
    applyActivityEvent(event("start", "browser-1", "browser_find", "r0", { query: "Go" }));
    applyActivityEvent(event("end", "browser-1", "browser_find", "r0", { query: "Go" }));
    // browser_navigate with a new tab starts on the visible page and ends on the new one.
    applyActivityEvent(event("start", "browser-1", "browser_navigate", "r1", { url: "http://x.test/" }));
    applyActivityEvent(event("end", "browser-2", "browser_navigate", "r1", { url: "http://x.test/" }));
    expect(card("browser:browser-1")).toMatchObject({ busy: false, activity: 'Finding "Go"' });
    expect(card("browser:browser-2")).toMatchObject({ busy: false, activity: "Opening http://x.test/" });
  });

  test("a terminal call has no card to mark", () => {
    applyActivityEvent({ chat: "a", kind: "terminal", tool: "terminal_run", phase: "start", tab: "t1", args: { command: "ls" } });
    expect(useCardsStore.getState().cards).toEqual({});
  });
});
