// A plugin's AI asks before one of evo's tools runs
// (docs/features/plugin-ai/spec.md, FR-16): the same approval as a chat's
// agent gets, in the main window, naming the plugin. Closing it denies.

import * as React from "react";
import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { SparklesIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { inTauri } from "@/lib/platform";
import { plugins, type AiQuestion } from "@/lib/plugins";

const useQuestions = create<{ queue: AiQuestion[]; set(list: AiQuestion[]): void; push(q: AiQuestion): void; drop(id: string): void }>((set) => ({
  queue: [],
  set: (list) => set({ queue: list }),
  push: (q) => set((s) => (s.queue.some((x) => x.id === q.id) ? s : { queue: [...s.queue, q] })),
  drop: (id) => set((s) => ({ queue: s.queue.filter((q) => q.id !== id) })),
}));

export function AiQuestionDialog() {
  const question = useQuestions((s) => s.queue[0]);
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  React.useEffect(() => {
    if (!inTauri) return;
    void plugins.aiQuestions().then((list) => useQuestions.getState().set(list)).catch(() => undefined);
    const offs = [
      listen<AiQuestion>("plugin-ai://question", ({ payload }) => useQuestions.getState().push(payload)),
      listen<{ id: string }>("plugin-ai://question-closed", ({ payload }) => useQuestions.getState().drop(payload.id)),
    ];
    return () => offs.forEach((off) => void off.then((stop) => stop()));
  }, []);
  async function answer(index: number) {
    if (!question) return;
    setBusy(true);
    setError(null);
    try {
      await plugins.aiAnswer(question.id, index);
    } catch (e) {
      setError(String(e));
    } finally {
      useQuestions.getState().drop(question.id);
      setBusy(false);
    }
  }
  const deny = question ? question.options.length - 1 : -1;
  const [why, ...rest] = (question?.detail ?? "").split("\n");
  return (
    <Dialog open={!!question} onOpenChange={(open) => !open && !busy && void answer(deny)}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{question?.title}</DialogTitle>
          <DialogDescription className="flex items-start gap-2">
            <SparklesIcon className="mt-0.5 size-3.5 shrink-0" />
            <span className="min-w-0 break-all">{why}</span>
          </DialogDescription>
        </DialogHeader>
        {rest.length > 0 && <pre className="max-h-48 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-muted px-3 py-2 font-mono text-[12px]">{rest.join("\n")}</pre>}
        {error && <p className="text-xs text-destructive">{error}</p>}
        <DialogFooter>
          {question?.options
            .map((label, index) => ({ label, index }))
            .reverse()
            .map(({ label, index }) => (
              <Button key={label} variant={index === deny ? "ghost" : index === 0 ? "default" : "outline"} disabled={busy} onClick={() => void answer(index)}>
                {label}
              </Button>
            ))}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
