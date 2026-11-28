import * as React from "react";
import { listen } from "@tauri-apps/api/event";
import { CalendarClockIcon, LightbulbIcon, ScrollTextIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { db, log, type LearningSuggestion } from "@/lib/db";

// Ideas the dream offers: things the agent could set up so the person stops
// asking by hand. Offered, never acted on: **Set it up** opens an editor
// filled in (nothing is created until the person saves or sends it), and
// **No thanks** means it never shows again.
// Spec: docs/features/learning-layers/spec.md (FR-14).

/** The pending ideas of these owners, kept fresh. */
export function useSuggestions(owners: string[]) {
  const [list, setList] = React.useState<LearningSuggestion[]>([]);
  const key = owners.join("|");
  const load = React.useCallback(() => {
    if (!key) return;
    void db
      .learningSuggestions(key.split("|"))
      .then(setList)
      .catch(() => setList([]));
  }, [key]);
  React.useEffect(() => {
    load();
    const stop = listen("learning://changed", load);
    return () => void stop.then((off) => off());
  }, [load]);
  const answer = React.useCallback(
    async (id: number, accepted: boolean) => {
      setList((current) => current.filter((s) => s.id !== id));
      await db.learningSuggestionAnswer(id, accepted).catch((error) => log("warn", "ui", `could not answer a suggestion: ${String(error)}`));
    },
    [],
  );
  return { list, answer };
}

/** What a skill idea asks the agent, put in the message box for the person to send. */
export const skillRequest = (text: string) => `Please write a skill for this, and show it to me before you save it: ${text}`;
/** What a schedule idea asks a mate, put in its message box. */
export const scheduleRequest = (text: string) => `Please set up a scheduled task for this, and check the details with me first: ${text}`;

export function SuggestionRow({
  suggestion,
  onSetUp,
  onDecline,
}: {
  suggestion: LearningSuggestion;
  onSetUp(): void;
  onDecline(): void;
}) {
  const Icon = suggestion.kind === "schedule" ? CalendarClockIcon : ScrollTextIcon;
  return (
    <div className="flex flex-col gap-1.5 rounded-lg bg-muted/40 px-2.5 py-2 dark:bg-accent/30">
      <div className="flex items-start gap-2">
        <Icon className="mt-0.5 size-3.5 shrink-0 text-brand" />
        <p className="text-[13px] leading-5">{suggestion.text}</p>
      </div>
      <div className="flex justify-end gap-1">
        <Button size="sm" variant="ghost" className="h-6 px-2 text-xs" onClick={onDecline}>
          No thanks
        </Button>
        <Button size="sm" variant="outline" className="h-6 px-2 text-xs" onClick={onSetUp}>
          Set it up
        </Button>
      </div>
    </div>
  );
}

/** The Memory panel's **Suggestions** (Evo Harness). */
export function Suggestions({ owners, onSetUp }: { owners: string[]; onSetUp(suggestion: LearningSuggestion): void }) {
  const { list, answer } = useSuggestions(owners);
  if (list.length === 0) return null;
  return (
    <section className="mb-3 flex flex-col gap-1">
      <p className="flex items-center gap-1.5 px-2 pb-1 text-[11px] font-medium text-muted-foreground">
        <LightbulbIcon className="size-3" />
        <span>Suggestions</span>
        <span className="font-normal opacity-70">From your recent chats</span>
      </p>
      {list.map((suggestion) => (
        <SuggestionRow
          key={suggestion.id}
          suggestion={suggestion}
          onDecline={() => void answer(suggestion.id, false)}
          onSetUp={() => {
            void answer(suggestion.id, true);
            onSetUp(suggestion);
          }}
        />
      ))}
    </section>
  );
}
