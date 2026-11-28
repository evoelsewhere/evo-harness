// Evo Mates' window: the messaging layout of docs/features/evo-mates/ux.md
// section 2 — the sidebar with the roster, the conversation with its bar,
// the mate's panel — in a frameless window on the system material,
// like the main window. A mate's conversation is its chat, shown by the
// engine store as any chat is; this window lists only Evo Mates' chats
// (spec FR-24).

import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ArrowLeftIcon } from "lucide-react";
import { MateFace } from "@/components/mate-face/mate-face";
import { Button } from "@/components/ui/button";
import { CustomizePage, MATES_SECTIONS } from "@/features/customize/customize-page";
import { MateModelPicker } from "@/features/mates/model-picker";
import { ChannelBar, ChannelView } from "@/features/mates/channel-view";
import { MateConversation } from "@/features/mates/conversation";
import { Gallery } from "@/features/mates/gallery";
import { MateGreeting, MatePanel, useShownMate } from "@/features/mates/mate-panel";
import { MateHeader } from "@/features/mates/mate-header";
import { MatesEmpty, MatesSidebar } from "@/features/mates/mates-shell";
import { FilesPage } from "@/features/mates/files-page";
import { QuickSwitcher } from "@/features/mates/quick-switcher";
import { ShortcutsSheet } from "@/features/mates/shortcuts-sheet";
import { panelDocks, railForced, sidebarForm } from "@/features/mates/sidebar-layout";
import { useMatesKeys } from "@/features/mates/use-mates-keys";
import { useMatesUiStore } from "@/stores/mates-ui-store";
import { TodayPage } from "@/features/mates/today";
import { startShellNotices } from "@/features/notifications/notify";
import { Toaster } from "@/features/notifications/toaster";
import { SettingsDialog } from "@/features/settings/settings-view";
import { QuitDialog } from "@/features/shell/quit-dialog";
import { WindowControls } from "@/features/shell/window-controls";
import { installErrorLogging, log } from "@/lib/db";
import { inTauri, isMac } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { setNativeMaterial } from "@/stores/appearance-store";
import { useEngineStore } from "@/stores/engine-store";
import { seenNow, useMatesStore } from "@/stores/mates-store";
import { useSessionStore } from "@/stores/session-store";
import { useUiStore } from "@/stores/ui-store";

/** The window's width, kept current (FR-1, FR-31). */
function useWindowWidth(): number {
  const [width, setWidth] = React.useState(() => window.innerWidth);
  React.useEffect(() => {
    const onResize = () => setWidth(window.innerWidth);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  return width;
}

function useBootstrap() {
  React.useEffect(() => {
    installErrorLogging();
    if (inTauri) {
      invoke<"mica" | "vibrancy" | "none">("window_material")
        .then(setNativeMaterial)
        .catch(() => setNativeMaterial("none"));
      // The shell's notices about mates no window shows reach this window too.
      startShellNotices(getCurrentWindow().label);
    }
    void (async () => {
      try {
        await useEngineStore.getState().connect();
      } catch (error) {
        log("error", "mates", `could not connect: ${String(error)}`);
      }
      await useMatesStore.getState().load();
    })();
  }, []);
}

/** A channel: its bar, then its thread once the engine store shows its root chat. */
function ChannelPane({ id }: { id: string }) {
  const channel = useMatesStore((s) => s.channels.find((c) => c.id === id));
  const shown = useEngineStore((s) => s.activeId);
  if (!channel) return <MatesEmpty />;
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-12 shrink-0 items-center pr-2">
        <ChannelBar channel={channel} />
      </div>
      <div className="min-h-0 flex-1">
        {shown === channel.chatId ? (
          <ChannelView channel={channel} />
        ) : (
          <div className="flex h-full animate-in items-center justify-center text-sm text-muted-foreground delay-300 duration-500 fill-mode-both fade-in">
            Opening #{channel.name}…
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * Customize, the same page as in Evo Harness (docs/features/connected-apps/
 * spec.md, FR-3; evo-mates History 2026-11-28), with the sections about
 * mates only — instructions, skills, connectors (apps, providers, MCP
 * servers), plugins — and a row back to the conversation.
 */
function MatesCustomize() {
  const mate = useShownMate();
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-10 shrink-0 items-center border-b border-border/60 px-2">
        <Button variant="ghost" size="sm" className="gap-1.5 text-muted-foreground" onClick={() => useUiStore.getState().closeCustomize()}>
          <ArrowLeftIcon />
          {mate ? `Back to ${mate.name}` : "Back to Evo Mates"}
        </Button>
      </div>
      <div className="min-h-0 flex-1">
        <CustomizePage only={MATES_SECTIONS} />
      </div>
    </div>
  );
}

/**
 * The shown mate's reply that landed while this window was in the background
 * is read once the person is back on it (FR-6): clicking the mate already
 * shown selects nothing, so nothing else would clear it.
 */
function useReadOnSight(chat: string | null, shown: string | null) {
  const unread = useSessionStore((s) => (chat ? Boolean(s.sessions.find((c) => c.id === chat)?.unread) : false));
  React.useEffect(() => {
    if (!chat || !unread) return;
    const check = () => {
      if (seenNow({ chat, shown, unread, focused: document.hasFocus() })) useSessionStore.getState().markUnread(chat, false);
    };
    check();
    window.addEventListener("focus", check);
    return () => window.removeEventListener("focus", check);
  }, [chat, shown, unread]);
}

function Conversation() {
  const width = useWindowWidth();
  const mate = useShownMate();
  const view = useMatesStore((s) => s.view);
  const panelOpen = useMatesStore((s) => s.panelOpen);
  const shown = useEngineStore((s) => s.activeId);
  useReadOnSight(mate?.chatId ?? null, shown);
  const customizeOpen = useUiStore((s) => s.customizeOpen);
  // Opening a mate, a channel or the gallery leaves Customize.
  React.useEffect(() => {
    useUiStore.getState().closeCustomize();
  }, [view, shown]);
  if (customizeOpen) return <MatesCustomize />;
  if (view.kind === "gallery") return <Gallery />;
  if (view.kind === "channel") return <ChannelPane id={view.id} />;
  // With no conversation open, Evo Mates shows its home (FR-73).
  if (view.kind === "today" || view.kind === "empty") return <TodayPage />;
  if (view.kind === "files") return <FilesPage />;
  if (!mate) return <MatesEmpty />;
  return (
    <div className="relative flex h-full min-h-0">
      <div className="relative flex min-w-0 flex-1 flex-col">
        {/* The conversation's bar, in its own row under the title row (ux.md 2). */}
        <div className="flex h-14 shrink-0 items-center border-b border-border/50">
          <MateHeader mate={mate} modelPill={shown === mate.chatId ? <MateModelPicker mateId={mate.id} mateName={mate.name} /> : null} />
        </div>
        {/* The mate's thread, once the engine store shows its chat. */}
        <div className="min-h-0 flex-1">
          {shown === mate.chatId ? (
            <MateConversation mate={mate} empty={<MateGreeting mate={mate} />} />
          ) : (
            // While its chat loads (a window opened again reads it first): a
            // quiet line instead of an empty pane, shown only if it takes a while.
            <div className="flex h-full animate-in items-center justify-center gap-2 text-sm text-muted-foreground delay-300 duration-500 fill-mode-both fade-in">
              <MateFace avatar={mate.avatar} size={22} mood="working" live plate={false} label={mate.name} />
              Opening {mate.name}…
            </div>
          )}
        </div>
      </div>
      {/* Under 1100 px the profile floats over the conversation; a click beside it closes it (FR-31). */}
      {panelOpen && !panelDocks(width) && (
        <div
          aria-hidden
          onClick={() => useMatesStore.getState().setPanelOpen(false)}
          className="absolute inset-0 z-20 animate-in bg-black/15 duration-200 fade-in dark:bg-black/35"
        />
      )}
      {panelOpen && <MatePanel mate={mate} />}
    </div>
  );
}

/** The window's title row: only the window's buttons; the mate's bar sits below it. */
function TitleRow() {
  return (
    <div data-tauri-drag-region className="drag-region flex h-10 shrink-0 items-center justify-end">
      <WindowControls />
    </div>
  );
}

export function MatesApp() {
  useBootstrap();
  useMatesKeys();
  const width = useWindowWidth();
  const folded = useMatesUiStore((s) => s.sidebarFolded);
  const form = sidebarForm(width, folded);
  return (
    <div className="flex h-full min-h-0">
      <MatesSidebar
        form={form}
        forced={railForced(width)}
        titleBar={
          <div
            data-tauri-drag-region
            className={cn("drag-region h-10 shrink-0", isMac && inTauri && "pl-[76px]")}
          />
        }
      />
      <div className="flex min-w-0 flex-1 flex-col">
        <TitleRow />
        <main className="min-h-0 flex-1 overflow-hidden bg-background min-[900px]:rounded-tl-xl">
          <Conversation />
        </main>
      </div>
      {/* "N chats are working. Quit anyway?" when the shell asks in this window. */}
      <QuitDialog />
      {/* Settings, the same as Evo Harness's (FR-29: the sidebar's footer opens it). */}
      <SettingsDialog />
      {/* In-app notices ("File not found", …) show here too, as in Evo Harness. */}
      <Toaster />
      {/* Ctrl+K and Ctrl+/ (FR-7, FR-8). */}
      <QuickSwitcher />
      <ShortcutsSheet />
    </div>
  );
}
