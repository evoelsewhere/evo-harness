// Who answers in a chat, when it is not "the agent": Evo Mates shows each
// reply as the mate's, in a bubble tinted with its colour, with its face
// and name (docs/features/evo-mates/spec.md, FR-30, FR-35; ux.md 3).

import * as React from "react";
import type { Turn } from "./transcript";

export interface ChatPersona {
  name: string;
  /** The face beside each reply. */
  avatar: React.ReactNode;
  /** The bubble's background. */
  tint: string;
  /** Show the face and name beside each reply (channels); a direct chat does not. */
  showAuthor?: boolean;
  /** A one-line message box with no model or approval menus: they live in the header and profile. */
  compactComposer?: boolean;
}

export const ChatPersonaContext = React.createContext<ChatPersona | null>(null);

export function useChatPersona(): ChatPersona | null {
  return React.useContext(ChatPersonaContext);
}

/** The turn saved or changed a memory topic: its "Saved to memory" chip (FR-35). */
export function savedMemory(turn: Pick<Turn, "parts">): boolean {
  return turn.parts.some((part) => {
    if (part.kind !== "tool" || part.name !== "memory" || part.phase !== "done") return false;
    const action = (part.args as { action?: string } | null)?.action;
    return (action === "save" || action === "update") && !part.details?.isError;
  });
}
