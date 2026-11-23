import * as React from "react";
import { useEngineStore } from "@/stores/engine-store";
import { useSessionStore } from "@/stores/session-store";
import type { ChatState } from "./sidebar-model";

/**
 * Every live chat that waits for the person or works, by id: from the engines
 * in this window and from the shell for chats with no window (shell-hosted
 * engines NFR-5). The same rules as a chat row's own status (`ChatItem`):
 * a run that waits for an approval is "waiting"
 * (docs/features/workspace-rail/spec.md, FR-2, FR-7).
 */
export function useChatStates(): Record<string, ChatState> {
  const work = useSessionStore((s) => s.work);
  const engines = useEngineStore((s) => {
    const marks: string[] = [];
    for (const [id, rt] of Object.entries(s.runtimes)) {
      if (rt.transcript.turns.at(-1)?.permission) marks.push(`${id}=w`);
      else if (rt.transcript.busy) marks.push(`${id}=r`);
    }
    for (const [id, busy] of Object.entries(s.busyElsewhere)) if (busy) marks.push(`${id}=r`);
    return marks.sort().join("|");
  });
  return React.useMemo(() => {
    const states: Record<string, ChatState> = {};
    for (const [id, state] of Object.entries(work)) states[id] = state === "waiting" ? "waiting" : "running";
    for (const mark of engines ? engines.split("|") : []) {
      const [id, kind] = mark.split("=");
      if (kind === "w") states[id] = "waiting";
      else if (states[id] !== "waiting") states[id] = "running";
    }
    return states;
  }, [work, engines]);
}
