// Run with `bun test` from apps/desktop. Spec: docs/features/session-panels (T7).
import { beforeEach, describe, expect, test } from "bun:test";

// A minimal Web Storage for the modules under test (bun has none).
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

const { useUiStore } = await import("./ui-store");
const { applyPanelEvent, panelReports } = await import("./agent-events");
const { forgetTabChat, recalledTabChat, rememberTabChat } = await import("@/lib/tab-chats");

const reset = () => useUiStore.setState({ openTabs: [], activePanel: null, rightPanelOpen: false, panelChat: null, panelsByChat: {} });

describe("tabs per chat", () => {
  beforeEach(reset);

  test("a chat that never opened a tab shows none, with the panel closed", () => {
    const ui = useUiStore.getState();
    ui.enterChat("a");
    ui.openPanel("terminal");
    ui.openPanel("browser");
    useUiStore.getState().enterChat("b");
    const state = useUiStore.getState();
    expect(state.openTabs).toEqual([]);
    expect(state.activePanel).toBeNull();
    expect(state.rightPanelOpen).toBe(false);
  });

  test("showing a chat again restores its tabs, visible tab and panel", () => {
    const ui = useUiStore.getState();
    ui.enterChat("a");
    ui.openPanel("terminal");
    ui.openPanel("browser");
    useUiStore.getState().enterChat("b");
    useUiStore.getState().openPanel("files");
    useUiStore.getState().enterChat("a");
    const state = useUiStore.getState();
    expect(state.openTabs).toEqual(["terminal", "browser"]);
    expect(state.activePanel).toBe("browser");
    expect(state.rightPanelOpen).toBe(true);
    expect(state.panelsByChat.b.openTabs).toEqual(["files"]);
  });

  test("tabs saved before chats existed go to the first chat shown", () => {
    useUiStore.setState({ openTabs: ["terminal"], activePanel: "terminal", rightPanelOpen: true, panelChat: null });
    useUiStore.getState().enterChat("first");
    expect(useUiStore.getState().openTabs).toEqual(["terminal"]);
    useUiStore.getState().enterChat("second");
    expect(useUiStore.getState().openTabs).toEqual([]);
  });

  test("an agent in a chat that is not showing opens its tab in that chat only", () => {
    const ui = useUiStore.getState();
    ui.enterChat("a");
    useUiStore.getState().openPanel("browser", "b");
    expect(useUiStore.getState().openTabs).toEqual([]);
    expect(useUiStore.getState().rightPanelOpen).toBe(false);
    useUiStore.getState().enterChat("b");
    const state = useUiStore.getState();
    expect(state.openTabs).toEqual(["browser"]);
    expect(state.activePanel).toBe("browser");
    expect(state.rightPanelOpen).toBe(true);
  });

  test("a chat that is gone drops its saved tabs", () => {
    const ui = useUiStore.getState();
    ui.enterChat("a");
    ui.openPanel("terminal");
    useUiStore.getState().enterChat("b");
    expect("a" in useUiStore.getState().panelsByChat).toBe(true);
    useUiStore.getState().forgetChat("a");
    expect("a" in useUiStore.getState().panelsByChat).toBe(false);
  });
});

describe("tab ownership across a reload", () => {
  test("remembers and forgets which chat a page or shell belongs to", () => {
    rememberTabChat("browser", "browser-1", "chat-a");
    rememberTabChat("terminal", 7, "chat-b");
    expect(recalledTabChat("browser", "browser-1")).toBe("chat-a");
    expect(recalledTabChat("terminal", 7)).toBe("chat-b");
    // The same id in the other kind is another tab.
    expect(recalledTabChat("terminal", "browser-1")).toBeNull();
    forgetTabChat("browser", "browser-1");
    expect(recalledTabChat("browser", "browser-1")).toBeNull();
    rememberTabChat("terminal", 7, null);
    expect(recalledTabChat("terminal", 7)).toBeNull();
  });
});

describe("agent panel requests", () => {
  beforeEach(reset);

  test("closeChatPanel closes the showing chat's tab and moves to its neighbour", () => {
    const ui = useUiStore.getState();
    ui.enterChat("a");
    ui.openPanel("terminal");
    useUiStore.getState().openPanel("files");
    useUiStore.getState().closeChatPanel("a", "files");
    const state = useUiStore.getState();
    expect(state.openTabs).toEqual(["terminal"]);
    expect(state.activePanel).toBe("terminal");
  });

  test("closeChatPanel edits another chat's saved tabs only", () => {
    const ui = useUiStore.getState();
    ui.enterChat("a");
    ui.openPanel("terminal");
    useUiStore.getState().enterChat("b");
    useUiStore.getState().openPanel("files");
    useUiStore.getState().enterChat("a");
    useUiStore.getState().openPanel("browser", "b");
    useUiStore.getState().closeChatPanel("b", "files");
    const state = useUiStore.getState();
    expect(state.openTabs).toEqual(["terminal"]);
    expect(state.panelsByChat.b.openTabs).toEqual(["browser"]);
    expect(state.panelsByChat.b.activePanel).toBe("browser");
    // Closing a tab that is not open changes nothing; an unknown chat is ignored.
    const before = useUiStore.getState().panelsByChat;
    useUiStore.getState().closeChatPanel("b", "memory");
    useUiStore.getState().closeChatPanel("nobody", "files");
    expect(useUiStore.getState().panelsByChat).toBe(before);
  });

  test("an agent://panel event opens or closes a tab of its chat, in its window only", () => {
    useUiStore.getState().enterChat("a");
    applyPanelEvent({ chat: "a", panel: "files", action: "open", window: "main" }, "main");
    expect(useUiStore.getState().openTabs).toEqual(["files"]);
    expect(useUiStore.getState().rightPanelOpen).toBe(true);
    applyPanelEvent({ chat: "a", panel: "terminal", action: "open", window: "other" }, "main");
    expect(useUiStore.getState().openTabs).toEqual(["files"]);
    applyPanelEvent({ chat: "a", panel: "plugins", action: "open" }, "main");
    expect(useUiStore.getState().openTabs).toEqual(["files"]);
    applyPanelEvent({ chat: "b", panel: "browser", action: "open", window: "main" }, "main");
    expect(useUiStore.getState().openTabs).toEqual(["files"]);
    expect(useUiStore.getState().panelsByChat.b.openTabs).toEqual(["browser"]);
    applyPanelEvent({ chat: "a", panel: "files", action: "close", window: "main" }, "main");
    expect(useUiStore.getState().openTabs).toEqual([]);
  });

  test("panel reports cover the showing chat, saved chats and forgotten ones", () => {
    const snap = () => {
      const { panelChat, openTabs, activePanel, rightPanelOpen, panelsByChat } = useUiStore.getState();
      return { panelChat, openTabs, activePanel, rightPanelOpen, panelsByChat };
    };
    const ui = useUiStore.getState();
    ui.enterChat("a");
    ui.openPanel("terminal");
    const first = snap();
    expect(panelReports(first)).toEqual([{ chat: "a", openTabs: ["terminal"], activePanel: "terminal", showing: true, panelOpen: true }]);
    expect(panelReports(first, first)).toEqual([]);
    useUiStore.getState().enterChat("b");
    const second = snap();
    const reports = panelReports(second, first);
    expect(reports).toContainEqual({ chat: "b", openTabs: [], activePanel: null, showing: true, panelOpen: false });
    expect(reports).toContainEqual({ chat: "a", openTabs: ["terminal"], activePanel: "terminal", showing: false, panelOpen: true });
    useUiStore.getState().forgetChat("a");
    expect(panelReports(snap(), second)).toEqual([{ chat: "a", openTabs: [], activePanel: null, showing: false, panelOpen: false }]);
  });
});
