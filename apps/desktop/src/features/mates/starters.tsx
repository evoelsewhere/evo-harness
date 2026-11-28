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

export function startersFor(mate: Pick<Mate, "template">, templates: Pick<MateTemplate, "id" | "starters">[]): string[] {
  const role = mate.template ? templates.find((t) => t.id === mate.template) : undefined;
  return role?.starters?.length ? role.starters : SCRATCH_STARTERS;
}

export function Starters({ starters }: { starters: string[] }) {
  const send = (text: string) => {
    // "…" asks the person to go on: put it in the box instead of sending.
    if (text.endsWith("…")) {
      composerDraft.set(text.slice(0, -1) + " ");
      return;
    }
    void useEngineStore
      .getState()
      .sendPrompt(text, [])
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
