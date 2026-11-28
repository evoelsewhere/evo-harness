import { composerDraft } from "@/features/chat";
import { SuggestionRow, scheduleRequest, skillRequest, useSuggestions } from "@/features/memory";
import type { Mate } from "@/lib/mates";

// A mate brings up what its dream noticed as a suggestion in its
// conversation: **Set it up** puts the request in the message box for the
// person to send (nothing is set up before that), **No thanks** never
// shows it again (docs/features/learning-layers/spec.md, FR-14).

export function MateSuggestions({ mate }: { mate: Mate }) {
  const { list, answer } = useSuggestions([`mate:${mate.id}`]);
  if (list.length === 0) return null;
  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-1.5 px-4 pb-2">
      <p className="text-xs text-muted-foreground">{mate.name} noticed something it could take off your hands:</p>
      {list.map((suggestion) => (
        <SuggestionRow
          key={suggestion.id}
          suggestion={suggestion}
          onDecline={() => void answer(suggestion.id, false)}
          onSetUp={() => {
            void answer(suggestion.id, true);
            composerDraft.set(suggestion.kind === "schedule" ? scheduleRequest(suggestion.text) : skillRequest(suggestion.text));
          }}
        />
      ))}
    </div>
  );
}
