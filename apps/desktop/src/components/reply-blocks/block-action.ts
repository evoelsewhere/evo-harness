// What a click on a reply block does (docs/features/rich-replies/spec.md,
// FR-22, NFR-1): send a message as the person, or open a file in the chat's
// folder or an https link. Nothing else, and only on a click.

import { notify } from "@/features/notifications/notify";
import { openLink } from "@/features/shell/app-commands";
import { resolveOpenTarget } from "@/lib/ui-blocks/open-target";
import type { Action } from "@/lib/ui-blocks/types";
import { sendOrSteer, useEngineStore } from "@/stores/engine-store";
import { openFile } from "@/stores/files-store";

/** A short description of what the action does, for a tooltip. */
export function actionHint(action: Action): string {
  return "send" in action ? `Sends: ${action.send}` : `Opens ${action.open}`;
}

export function runBlockAction(action: Action) {
  if ("send" in action) {
    sendOrSteer(action.send);
    return;
  }
  const target = resolveOpenTarget(action.open, useEngineStore.getState().cwd);
  if (target.kind === "file") void openFile(target.path, target.line);
  else if (target.kind === "link") openLink(target.url);
  else notify({ source: "chat", level: "warning", title: "Couldn't open that", body: target.reason, system: false });
}
