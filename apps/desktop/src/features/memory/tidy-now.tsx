import * as React from "react";
import { SparklesIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { useLearningStore, type TidyResult } from "@/stores/learning-store";

// **Tidy now**: the tidy-up for these memory owners at once — ended facts to
// the archive, repeated ones gone and, with a model, outdated ones struck.
// Spec: docs/features/learning-layers/spec.md (FR-6, FR-24).

/** What a run did, in a few words. */
export function tidySummary(results: TidyResult[]): string {
  const errors = results.map((r) => r.error).filter(Boolean);
  if (errors.length) return `Could not tidy: ${errors[0]}`;
  const archived = results.reduce((sum, r) => sum + (r.archived ?? 0), 0);
  const struck = results.reduce((sum, r) => sum + (r.struck ?? 0), 0);
  const changed = results.reduce((sum, r) => sum + (r.changed?.length ?? 0), 0);
  const parts: string[] = [];
  if (archived) parts.push(`${archived} ended ${archived === 1 ? "fact" : "facts"} archived`);
  if (struck) parts.push(`${struck} outdated ${struck === 1 ? "fact" : "facts"} struck`);
  if (!parts.length && changed) parts.push(`${changed} ${changed === 1 ? "topic" : "topics"} tidied`);
  const done = parts.length ? `${parts.join(", ")}.` : "Nothing to tidy.";
  const noModel = results.some((r) => r.skipped === "contradictions need a model");
  return noModel ? `${done} Finding outdated facts needs a model: choose a Background model in Settings › Memory.` : done;
}

export function TidyNow({ owners, onDone }: { owners: string[]; onDone?(): void }) {
  const tidyNow = useLearningStore((s) => s.tidyNow);
  const [running, setRunning] = React.useState(false);
  const [said, setSaid] = React.useState<string | null>(null);
  const run = async () => {
    setRunning(true);
    setSaid(null);
    try {
      setSaid(tidySummary(await tidyNow(owners)));
      onDone?.();
    } catch (e) {
      setSaid(`Could not tidy: ${String(e).replace(/^Error:\s*/, "")}`);
    } finally {
      setRunning(false);
    }
  };
  return (
    <div className="flex flex-col items-start gap-1">
      <Button size="sm" variant="ghost" className="h-6 gap-1 px-2 text-xs" disabled={running || owners.length === 0} onClick={() => void run()}>
        {running ? <Spinner className="size-3" /> : <SparklesIcon className="size-3" />} Tidy now
      </Button>
      {said && <p className="px-2 text-xs text-muted-foreground">{said}</p>}
    </div>
  );
}
