// The quick entry box as a viewer of one chat: start it, show it, follow up,
// let go, and hand it to the main window
// (docs/features/quick-entry/spec.md, FR-5, FR-11, FR-15, FR-16, FR-18, FR-19;
// plan 3.5). Loaded on the box's first keystroke, so the box itself still
// opens at once (NFR-1).

import { invoke } from "@tauri-apps/api/core";
import { installErrorLogging, log } from "@/lib/db";
import { useEngineStore, type OpenTarget } from "@/stores/engine-store";
import { useSessionStore } from "@/stores/session-store";
import { forwardPanelsTo } from "@/stores/ui-store";
import { answerable } from "./answerable";

let booted: Promise<void> | null = null;

/** Connect the stores once: chats, settings, the engine client. */
export function bootBox(): Promise<void> {
  if (!booted) {
    booted = (async () => {
      installErrorLogging();
      // What would open in a side panel opens in the main window (FR-12).
      forwardPanelsTo((panel, _chat, item) => {
        const chat = useEngineStore.getState().activeId;
        if (!chat) return false;
        openInEvo(chat, { kind: "panel", panel, item });
        return true;
      });
      await useEngineStore.getState().connect();
      // A reply ended: the box's chat was just used (FR-18).
      useEngineStore.subscribe((state, previous) => {
        if (!state.activeId || state.activeId !== previous.activeId) return;
        if (previous.transcript.busy && !state.transcript.busy) tellShell(state.activeId, true);
      });
    })();
    booted.catch(() => {
      booted = null;
    });
  }
  return booted;
}

/** The shell keeps the box's chat and its last use (FR-18). */
function tellShell(chat: string | null, used: boolean) {
  void invoke("quick_entry_chat", { chat, used }).catch(() => undefined);
}

/** A new chat with `text` as its first message, as **New chat** would; its id. */
export async function startChat(text: string): Promise<string | null> {
  await bootBox();
  // The main window may have shown another workspace since the box loaded.
  await useSessionStore.getState().reloadShownWorkspace();
  const engine = useEngineStore.getState();
  await engine.newChat();
  const chat = useEngineStore.getState().activeId;
  if (!chat) return null;
  tellShell(chat, true);
  await useEngineStore.getState().sendPrompt(text);
  return chat;
}

/** Show `chat` (the shell gave it on this opening). */
export async function showChat(chat: string): Promise<void> {
  await bootBox();
  if (useEngineStore.getState().activeId === chat) return;
  await useEngineStore.getState().selectSession(chat);
}

/** A follow-up in the box's chat (queued while a turn runs, as in the main window). */
export async function followUp(text: string): Promise<void> {
  const chat = useEngineStore.getState().activeId;
  if (!chat) return;
  tellShell(chat, true);
  await useEngineStore.getState().sendPrompt(text);
}

/** Empty the box: let go of its chat, which runs on while it has work. */
export function letGo(tell = true): void {
  useEngineStore.getState().letGo();
  if (tell) tellShell(null, false);
}

/** Hide the box and show `chat` in the main window, maybe at `target` (FR-15). */
export function openInEvo(chat: string, target?: OpenTarget): void {
  void invoke("quick_entry_open", { chat, target: target ?? null }).catch((error) =>
    log("warn", "quick-entry", `could not open the chat in Evo: ${String(error)}`),
  );
}

/**
 * A notification of the box's chat was clicked: the box shows it, unless
 * what the chat waits for is answered in the main window (FR-19).
 */
export function noticeClicked(chat: string): void {
  const state = useEngineStore.getState();
  const waiting = state.activeId === chat ? state.transcript.permissions?.[0] : undefined;
  if (waiting && answerable(waiting) === "elsewhere") {
    openInEvo(chat, { kind: "question", id: waiting.requestId });
    return;
  }
  void invoke("quick_entry_show").catch(() => undefined);
}

/** Whether the box's chat runs a turn now (the Stop button, FR-11). */
export function useRunning(chat: string): boolean {
  return useEngineStore((s) => s.activeId === chat && s.transcript.busy);
}

/** Stop the running turn, as the main window's Stop does. */
export function stop(): void {
  void useEngineStore.getState().abort();
}
