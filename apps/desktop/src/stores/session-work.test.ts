// Run with `bun test` from apps/desktop. Spec: docs/features/shell-hosted-engines
// (FR-11, FR-12, NFR-5; T33, T34): what the sidebar learns from the shell's
// working list, and the quit question's words.
import { describe, expect, test } from "bun:test";

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

const { useSessionStore, workByChat } = await import("./session-store");
const { quitTitle } = await import("@/features/shell/quit-dialog");

describe("chats working with no window (NFR-5)", () => {
  test("the shell's list becomes working and waiting marks by chat", () => {
    expect(
      workByChat([
        { chat: "a", title: "Fix the build", state: "waiting" },
        { chat: "b", title: "A chat", state: "working" },
      ]),
    ).toEqual({ a: "waiting", b: "working" });
    expect(workByChat([])).toEqual({});
    expect(workByChat(null)).toEqual({});
    // Anything else the shell might add is not drawn.
    expect(workByChat([{ chat: "c", title: "x", state: "other" as never }])).toEqual({});
  });

  test("the store starts with nothing working", () => {
    expect(useSessionStore.getState().work).toEqual({});
  });
});

describe("the quit question (FR-11)", () => {
  test("says how many chats work", () => {
    expect(quitTitle(1)).toBe("1 chat is working. Quit anyway?");
    expect(quitTitle(3)).toBe("3 chats are working. Quit anyway?");
  });
});
