// Run with `bun test` from apps/desktop. Spec: docs/features/computer-app-control (FR-10, FR-12, T28).
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
const { useComputerStore } = await import("./computer-store");
const { useUiStore } = await import("./ui-store");
const { applyActivityEvent, applyComputerAttached, applyComputerReleased, applyComputerState } = await import("./agent-events");

const appCard = (chat: string) => useCardsStore.getState().cards[chat]?.find((c) => c.key === `app:${chat}`);

describe("the card of the app a chat's agent controls", () => {
  beforeEach(() => {
    useCardsStore.setState({ cards: {} });
    useComputerStore.setState({ apps: {} });
    useUiStore.setState({ agentCards: true });
  });

  test("attaching gives the chat an app card titled by its window", () => {
    applyComputerAttached({ chat: "a", app: "EXCEL.EXE", title: "Book1 - Excel", windowId: 42, web: false });
    expect(appCard("a")).toMatchObject({ kind: "app", id: "a", mode: "card" });
    expect(useComputerStore.getState().apps.a).toMatchObject({ app: "EXCEL.EXE", title: "Book1 - Excel", state: "active" });
    expect(appCard("b")).toBeUndefined();
  });

  test("releasing the app removes its card", () => {
    applyComputerAttached({ chat: "a", app: "notepad.exe", title: "a.txt" });
    applyComputerReleased({ chat: "a", reason: "turn" });
    expect(appCard("a")).toBeUndefined();
    expect(useComputerStore.getState().apps.a).toBeUndefined();
  });

  test("a stopped app keeps its card, so the person can allow it again", () => {
    applyComputerAttached({ chat: "a", app: "notepad.exe", title: "a.txt" });
    applyComputerState({ chat: "a", state: "stopped" });
    applyComputerReleased({ chat: "a", reason: "gone" });
    expect(appCard("a")).toBeDefined();
    expect(useComputerStore.getState().apps.a?.state).toBe("stopped");
    applyComputerState({ chat: "a", state: "active" });
    expect(useComputerStore.getState().apps.a?.state).toBe("active");
  });

  test("handing every app back removes every app card that is not stopped", () => {
    applyComputerAttached({ chat: "a", app: "x.exe" });
    applyComputerAttached({ chat: "b", app: "y.exe" });
    applyComputerState({ chat: "b", state: "stopped" });
    applyComputerReleased({ chat: null, reason: "all" });
    expect(appCard("a")).toBeUndefined();
    expect(appCard("b")).toBeDefined();
  });

  test("the agent's step shows on the app card while it runs", () => {
    applyComputerAttached({ chat: "a", app: "notepad.exe" });
    applyActivityEvent({ chat: "a", kind: "computer", tool: "computer_app", phase: "start", tab: null, requestId: "r1", args: { action: "click e12" } });
    expect(appCard("a")).toMatchObject({ activity: "Click e12", busy: true });
    applyActivityEvent({ chat: "a", kind: "computer", tool: "computer_app", phase: "end", tab: null, requestId: "r1", args: { action: "click e12" } });
    expect(appCard("a")).toMatchObject({ busy: false });
  });
});
