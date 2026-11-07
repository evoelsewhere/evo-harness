// The side panel's tabs, browser pages and terminals belong to a chat: showing
// another chat swaps them (a chat that never opened any shows none), and the
// pages and shells of the chat that went out of sight keep running.
// Spec: docs/features/session-panels.

import { invoke } from "@tauri-apps/api/core";
import { startAgentEvents } from "./agent-events";
import { useBrowserStore } from "./browser-store";
import { useEngineStore } from "./engine-store";
import { useSessionStore } from "./session-store";
import { useTerminalStore } from "./terminal-store";
import { useUiStore } from "./ui-store";

/** Tell every store which chat shows. The tabs go first: the browser reads them. */
export function enterChat(chat: string | null) {
  // The shell's tools create pages and shells in the window showing the chat.
  void invoke("agent_window_chat", { chat }).catch(() => undefined);
  useUiStore.getState().enterChat(chat);
  useTerminalStore.getState().setChat(chat);
  useBrowserStore.getState().setChat(chat);
}

/** A chat is gone: its shells and pages end with it, and its saved tabs are dropped. */
function releaseChat(chat: string) {
  useTerminalStore.getState().closeAll(chat);
  const browser = useBrowserStore.getState();
  for (const tab of browser.all.filter((t) => t.chat === chat)) browser.closeTab(tab.id);
  useUiStore.getState().forgetChat(chat);
}

let started = false;

/** Follow the chat showing, from now on. */
export function startChatScope() {
  if (started) return;
  started = true;
  enterChat(useEngineStore.getState().activeId);
  startAgentEvents();
  useEngineStore.subscribe((state, previous) => {
    if (state.activeId !== previous.activeId) enterChat(state.activeId);
  });
  useSessionStore.subscribe((state, previous) => {
    if (state.sessions === previous.sessions) return;
    const alive = new Set(state.sessions.map((chat) => chat.id));
    for (const chat of previous.sessions) if (!alive.has(chat.id)) releaseChat(chat.id);
  });
}
