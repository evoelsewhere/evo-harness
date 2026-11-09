import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import { isWindows, inTauri } from "@/lib/platform";
import { useBrowserStore } from "@/stores/browser-store";
import { useCardsStore } from "@/stores/cards-store";
import { useUiStore } from "@/stores/ui-store";
import { useChatCards } from "./use-cards";

/** Frames per second asked for a page something shows (about what a person follows without cost). */
const FPS = 10;
/** An app's pictures are captured, not pushed: a little fewer (FR-10: 6–10). */
const APP_FPS = 8;

/**
 * Tells the shell which staged pages to stream: the pages of the showing chat's
 * shown cards, and the page the side panel shows when it is one the agent uses
 * (it draws the same picture). Pages nothing shows stay on the stage without
 * frames. Renders nothing. Spec: docs/features/floating-cards (FR-8, FR-9).
 */
export function StageController({ chat }: { chat: string | null }) {
  const { shown } = useChatCards(chat);
  const all = useCardsStore((s) => (chat ? s.cards[chat] : undefined));
  const panelOpen = useUiStore((s) => s.rightPanelOpen);
  const activePanel = useUiStore((s) => s.activePanel);
  const panelChat = useUiStore((s) => s.panelChat);
  const activeTab = useBrowserStore((s) => s.activeId);

  const wanted = React.useMemo(() => {
    const ids = new Set(shown.filter((c) => c.kind === "browser").map((c) => c.id));
    const panelTab = panelOpen && activePanel === "browser" && panelChat === chat ? activeTab : null;
    if (panelTab && all?.some((c) => c.kind === "browser" && c.id === panelTab)) ids.add(panelTab);
    return ids;
  }, [shown, all, panelOpen, activePanel, panelChat, chat, activeTab]);

  // The chat's app streams while its card shows (docs/features/computer-app-control FR-10, NFR-8).
  const appShown = !!chat && shown.some((c) => c.kind === "app");
  const appStreaming = React.useRef<string | null>(null);
  React.useEffect(() => {
    if (!inTauri || !isWindows) return;
    const want = appShown ? chat : null;
    if (appStreaming.current && appStreaming.current !== want) void invoke("computer_stage", { chat: appStreaming.current, on: false }).catch(() => undefined);
    if (want && appStreaming.current !== want) void invoke("computer_stage", { chat: want, on: true, fps: APP_FPS }).catch(() => undefined);
    appStreaming.current = want;
  }, [appShown, chat]);
  React.useEffect(
    () => () => {
      if (appStreaming.current) void invoke("computer_stage", { chat: appStreaming.current, on: false }).catch(() => undefined);
      appStreaming.current = null;
    },
    [],
  );

  const streaming = React.useRef(new Set<string>());
  React.useEffect(() => {
    if (!inTauri || !isWindows) return;
    for (const tab of wanted) {
      if (!streaming.current.has(tab)) void invoke("browser_stage", { tab, on: true, fps: FPS }).catch(() => undefined);
    }
    for (const tab of streaming.current) {
      if (!wanted.has(tab)) void invoke("browser_stage", { tab, on: true }).catch(() => undefined);
    }
    streaming.current = new Set(wanted);
  }, [wanted]);

  // Nothing is shown any more (the chat window goes away): stop the frames.
  React.useEffect(
    () => () => {
      for (const tab of streaming.current) void invoke("browser_stage", { tab, on: true }).catch(() => undefined);
      streaming.current = new Set();
    },
    [],
  );
  return null;
}
