// Run with `bun test` from apps/desktop. Spec: docs/features/agent-tools-layers (FR-3, T9).
// The view's side of the shell's user tools: `agent://notify` and `agent://show-file`.
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
// `notify()` asks whether the window is in front; a window that is not keeps toasts away.
Object.defineProperty(globalThis, "document", { value: { visibilityState: "hidden", hasFocus: () => false }, configurable: true, writable: true });

const { useNotificationStore } = await import("./notification-store");
const { useUiStore } = await import("./ui-store");
const { useFilesStore } = await import("./files-store");
const { applyNotifyEvent, applyShowFileEvent, notifyMuted } = await import("./agent-events");

describe("agent://notify", () => {
  beforeEach(() => useNotificationStore.setState({ notifications: [], toasts: [], doNotDisturb: false, mutedSources: [] }));

  test("adds a bell entry that opens the chat", () => {
    applyNotifyEvent({ chat: "c1", title: "Build done", body: "3 warnings", window: "main" }, "main");
    const [entry] = useNotificationStore.getState().notifications;
    expect(entry).toMatchObject({ title: "Build done", body: "3 warnings", source: "agent", key: "agent:c1", target: { type: "session", id: "c1" }, read: false });
  });

  test("a body of null is no body, and a later call of the chat replaces the entry", () => {
    applyNotifyEvent({ chat: "c1", title: "One", body: null }, "main");
    applyNotifyEvent({ chat: "c1", title: "Two" }, "main");
    const { notifications } = useNotificationStore.getState();
    expect(notifications.map((n) => n.title)).toEqual(["Two"]);
    expect(notifications[0].body).toBeUndefined();
  });

  test("another window's event, or one without a chat or title, adds nothing", () => {
    applyNotifyEvent({ chat: "c1", title: "Hi", window: "chat-2" }, "main");
    applyNotifyEvent({ chat: "", title: "Hi" }, "main");
    applyNotifyEvent({ chat: "c1", title: "" }, "main");
    expect(useNotificationStore.getState().notifications).toEqual([]);
    applyNotifyEvent({ chat: "c1", title: "Hi" }, "main");
    expect(useNotificationStore.getState().notifications).toHaveLength(1);
  });

  test("the shell is told to keep the OS notification back when muted", () => {
    expect(notifyMuted({ doNotDisturb: false, mutedSources: ["engine"] })).toBe(false);
    expect(notifyMuted({ doNotDisturb: true, mutedSources: [] })).toBe(true);
    expect(notifyMuted({ doNotDisturb: false, mutedSources: ["agent"] })).toBe(true);
  });
});

describe("agent://show-file", () => {
  beforeEach(() => {
    useUiStore.setState({ openTabs: [], activePanel: null, rightPanelOpen: false, panelChat: null, panelsByChat: {} });
    useFilesStore.setState({ tabs: [], files: {}, active: null });
  });

  test("opens the file in the Files panel at the line", () => {
    applyShowFileEvent({ chat: "c1", path: "C:\\work\\a.rs", line: 42, window: "main" }, "main");
    const files = useFilesStore.getState();
    expect(files.tabs).toEqual(["C:\\work\\a.rs"]);
    expect(files.active).toBe("C:\\work\\a.rs");
    expect(files.reveal).toMatchObject({ path: "C:\\work\\a.rs", line: 42 });
    expect(useUiStore.getState().activePanel).toBe("files");
  });

  test("a chat that is not showing gets the Files tab marked for when it shows", () => {
    useUiStore.getState().enterChat("c1");
    applyShowFileEvent({ chat: "c2", path: "/w/b.rs", line: null }, "main");
    expect(useUiStore.getState().panelsByChat.c2.openTabs).toEqual(["files"]);
  });

  test("another window's event, or one without a path, opens nothing", () => {
    applyShowFileEvent({ chat: "c1", path: "/w/a.rs", window: "chat-2" }, "main");
    applyShowFileEvent({ chat: "c1", path: "" }, "main");
    expect(useFilesStore.getState().tabs).toEqual([]);
    expect(useUiStore.getState().activePanel).toBeNull();
  });
});
