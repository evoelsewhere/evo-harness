// Conversation starters (docs/features/evo-mates/spec.md, FR-74): a new
// conversation shows its role's requests, in the person's words, under the
// greeting; choosing one sends it as the person's first message.

import { composerDraft } from "@/features/chat";
import { log } from "@/lib/db";
import type { Mate, MateTemplate } from "@/lib/mates";
import { useEngineStore } from "@/stores/engine-store";

/** For a mate made from scratch: requests that help it settle in. */
export const SCRATCH_STARTERS = [
  "Here is what I need help with…",
  "Ask me what you need to know to help me",
  "What can you do with my files?",
];

/** Its role's starters: by the role it was made from, else by its role line (a team's mates). */
export function startersFor(mate: Pick<Mate, "template" | "role">, templates: Pick<MateTemplate, "id" | "role" | "starters">[]): string[] {
  const role =
    (mate.template ? templates.find((t) => t.id === mate.template) : undefined) ??
    (mate.role ? templates.find((t) => t.role === mate.role) : undefined);
  return role?.starters?.length ? role.starters : SCRATCH_STARTERS;
}

export function Starters({ starters }: { starters: string[] }) {
  const send = (text: string) => {
    // "…" asks the person to go on: put it in the box instead of sending.
    if (text.endsWith("…")) {
      composerDraft.set(text.slice(0, -1) + " ");
      return;
    }
    const engine = useEngineStore.getState();
    const chat = engine.activeId;
    void (chat ? engine.whenSetUp(chat) : Promise.resolve())
      .then(() => useEngineStore.getState().sendPrompt(text, []))
      .catch((error) => log("warn", "mates", `could not send a starter: ${String(error)}`));
  };
  return (
    <div className="flex max-w-lg flex-wrap justify-center gap-2 pt-2" aria-label="Conversation starters">
      {starters.map((text) => (
        <button
          key={text}
          type="button"
          onClick={() => send(text)}
          className="rounded-full bg-muted/70 px-3.5 py-1.5 text-[13px] text-foreground transition-colors hover:bg-muted"
        >
          {text}
        </button>
      ))}
    </div>
  );
}
