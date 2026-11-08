// Run with `bun test` from apps/desktop. Spec: docs/features/floating-cards (T9).
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

// Another test file may have left a bare `document` behind; the modules below touch the DOM when one exists, so load them without it.
const savedDocument = (globalThis as { document?: unknown }).document;
Reflect.deleteProperty(globalThis, "document");

const { useUiStore } = await import("./ui-store");
const { useBrowserStore } = await import("./browser-store");
await import("./cards-sync");
const { useCardsStore, clampRect, defaultSize, slotRect, split, MAX_VISIBLE } = await import("./cards-store");

if (savedDocument !== undefined) Object.defineProperty(globalThis, "document", { value: savedDocument, configurable: true, writable: true });

const reset = () => {
  useCardsStore.setState({ cards: {} });
  useUiStore.setState({ agentCards: true });
};
const cards = (chat: string) => useCardsStore.getState().cards[chat] ?? [];
const ensure = (chat: string, id: string) => useCardsStore.getState().ensure(chat, "browser", id);

describe("cards per chat", () => {
  beforeEach(reset);

  test("an agent's tab gets a shown card in its own chat only", () => {
    ensure("a", "browser-1");
    expect(cards("a").map((c) => [c.key, c.mode])).toEqual([["browser:browser-1", "card"]]);
    expect(cards("b")).toEqual([]);
  });

  test("asking again for the same tab makes no second card", () => {
    ensure("a", "browser-1");
    ensure("a", "browser-1");
    expect(cards("a")).toHaveLength(1);
  });

  test("a fourth tab is a chip, and the setting off makes every card a chip", () => {
    for (const id of ["1", "2", "3", "4"]) ensure("a", `browser-${id}`);
    expect(split(cards("a")).shown).toHaveLength(MAX_VISIBLE);
    expect(split(cards("a")).chips.map((c) => c.id)).toEqual(["browser-4"]);
    useUiStore.setState({ agentCards: false });
    ensure("b", "browser-9");
    expect(cards("b")[0].mode).toBe("chip");
  });

  test("minimizing makes a chip, showing it again takes the place of the least recently raised card", () => {
    for (const id of ["1", "2", "3", "4"]) ensure("a", `browser-${id}`);
    const store = useCardsStore.getState();
    store.raise("a", "browser:browser-1");
    store.setMode("a", "browser:browser-4", "card");
    const shown = split(cards("a")).shown.map((c) => c.id);
    expect(shown).toHaveLength(MAX_VISIBLE);
    expect(shown).toContain("browser-4");
    expect(shown).toContain("browser-1");
    expect(shown).not.toContain("browser-2");
    store.setMode("a", "browser:browser-4", "chip");
    expect(split(cards("a")).chips.map((c) => c.id)).toContain("browser-4");
  });

  test("moving a card keeps its place and size; a card nobody moved has none", () => {
    ensure("a", "browser-1");
    expect(cards("a")[0].rect).toBeNull();
    useCardsStore.getState().place("a", "browser:browser-1", { x: 10, y: 20, w: 400, h: 300 });
    expect(cards("a")[0].rect).toEqual({ x: 10, y: 20, w: 400, h: 300 });
    expect(cards("a")[0].size).toEqual({ w: 400, h: 300 });
  });

  test("the activity line is set and cleared", () => {
    ensure("a", "browser-1");
    useCardsStore.getState().setActivity("a", "browser:browser-1", "Clicking ref_4");
    expect(cards("a")[0].activity).toBe("Clicking ref_4");
    useCardsStore.getState().setActivity("a", "browser:browser-1", null);
    expect(cards("a")[0].activity).toBeNull();
    useCardsStore.getState().setActivity("a", "browser:missing", "x");
    expect(cards("a")).toHaveLength(1);
  });

  test("a tab that closes takes its card and chip with it, in every chat", () => {
    ensure("a", "browser-1");
    ensure("a", "browser-2");
    ensure("b", "browser-1");
    useCardsStore.getState().removeTab("browser", "browser-1");
    expect(cards("a").map((c) => c.id)).toEqual(["browser-2"]);
    expect(cards("b")).toEqual([]);
  });

  test("closing a page in its store removes its card", () => {
    const tab = { id: "browser-7", url: "http://x.test/", title: "", loading: false, chat: "a" };
    useBrowserStore.setState({ all: [tab] });
    ensure("a", "browser-7");
    useBrowserStore.setState({ all: [] });
    expect(cards("a")).toEqual([]);
    // A tab that was never there (a reload adopts them later) does not remove a restored card.
    ensure("a", "browser-8");
    useBrowserStore.setState({ all: [] });
    expect(cards("a")).toHaveLength(1);
  });

  test("a chat that is gone forgets its cards", () => {
    ensure("a", "browser-1");
    useCardsStore.getState().forgetChat("a");
    expect(useCardsStore.getState().cards).toEqual({});
  });
});

describe("placement", () => {
  const layer = { w: 900, h: 700 };

  test("cards start at the top right and cascade, inside the layer", () => {
    const size = defaultSize("browser");
    const first = slotRect(0, size, layer);
    const second = slotRect(1, size, layer);
    expect(first.x + first.w).toBe(layer.w - 16);
    expect(first.y).toBe(16);
    expect(second.x).toBeLessThan(first.x);
    expect(second.y).toBeGreaterThan(first.y);
  });

  test("a card is kept inside the layer when the window gets smaller", () => {
    const small = { w: 400, h: 300 };
    const rect = clampRect({ x: 700, y: 650, w: 360, h: 261 }, small);
    expect(rect.x + rect.w).toBeLessThanOrEqual(small.w);
    expect(rect.y + rect.h).toBeLessThanOrEqual(small.h);
    expect(rect.x).toBeGreaterThanOrEqual(0);
    expect(clampRect({ x: -50, y: -50, w: 360, h: 261 }, layer)).toMatchObject({ x: 0, y: 0 });
  });

  test("a page card is 16:10 under its bar", () => {
    const page = defaultSize("browser");
    expect(Math.round(((page.h - 32) / page.w) * 160)).toBe(100);
  });
});
