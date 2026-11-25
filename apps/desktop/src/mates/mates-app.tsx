// Evo Mates' window: the messaging layout of docs/features/evo-mates/ux.md
// section 2 — a sidebar with the roster, the conversation area — in a
// frameless window on the system material, like the main window.
// Phase 1 (tasks T3): the frame, the switch to Evo Harness and an empty
// roster; mates, conversations and channels come with phases 2 and 3.

import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import { QuitDialog } from "@/features/shell/quit-dialog";
import { WindowControls } from "@/features/shell/window-controls";
import { MatesEmpty, MatesSidebar } from "@/features/mates/mates-shell";
import { installErrorLogging } from "@/lib/db";
import { inTauri, isMac } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { setNativeMaterial } from "@/stores/appearance-store";

export function MatesApp() {
  React.useEffect(() => {
    installErrorLogging();
    if (!inTauri) return;
    invoke<"mica" | "vibrancy" | "none">("window_material")
      .then(setNativeMaterial)
      .catch(() => setNativeMaterial("none"));
  }, []);

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
        <div data-tauri-drag-region className="drag-region flex h-10 shrink-0 items-center justify-end">
          <WindowControls />
        </div>
        <main className="min-h-0 flex-1 overflow-hidden bg-background min-[900px]:rounded-tl-xl">
          <MatesEmpty />
        </main>
      </div>
      {/* "N chats are working. Quit anyway?" when the shell asks in this window. */}
      <QuitDialog />
    </div>
  );
}
