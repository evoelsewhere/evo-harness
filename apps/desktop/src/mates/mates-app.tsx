// Evo Mates' window: the messaging layout of docs/features/evo-mates/ux.md
// section 2 — the sidebar with the roster, the conversation with its bar,
// the mate's panel — in a frameless window on the system material,
// like the main window. A mate's conversation is its chat, shown by the
// engine store as any chat is; this window lists only Evo Mates' chats
// (spec FR-24).

import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ChatModelPill } from "@/features/chat";
import { MateConversation } from "@/features/mates/conversation";
import { Gallery } from "@/features/mates/gallery";
import { MateBar, MateGreeting, MatePanel, useShownMate } from "@/features/mates/mate-panel";
import { MatesEmpty, MatesSidebar } from "@/features/mates/mates-shell";
import { startShellNotices } from "@/features/notifications/notify";
import { QuitDialog } from "@/features/shell/quit-dialog";
import { WindowControls } from "@/features/shell/window-controls";
import { installErrorLogging, log } from "@/lib/db";
import { inTauri, isMac } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { setNativeMaterial } from "@/stores/appearance-store";
import { useEngineStore } from "@/stores/engine-store";
import { useMatesStore } from "@/stores/mates-store";

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

function Conversation() {
  const mate = useShownMate();
  const view = useMatesStore((s) => s.view);
  const panelOpen = useMatesStore((s) => s.panelOpen);
  const shown = useEngineStore((s) => s.activeId);
  if (view.kind === "gallery") return <Gallery />;
  if (!mate) return <MatesEmpty />;
  return (
    <div className="flex h-full min-h-0">
      <div className="relative flex min-w-0 flex-1 flex-col">
        {/* The conversation's bar, in its own row under the title row (ux.md 2). */}
        <div className="flex h-12 shrink-0 items-center border-b border-border pr-2">
          <MateBar mate={mate} modelPill={shown === mate.chatId ? <ChatModelPill /> : null} />
        </div>
        {/* The mate's thread, once the engine store shows its chat. */}
        <div className="min-h-0 flex-1">
          {shown === mate.chatId ? <MateConversation mate={mate} empty={<MateGreeting mate={mate} />} /> : null}
        </div>
      </div>
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
  return (
    <div className="flex h-full min-h-0">
      <MatesSidebar
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
    </div>
  );
}
