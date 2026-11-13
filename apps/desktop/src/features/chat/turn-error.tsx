import * as React from "react";
import { ChevronRightIcon, CircleAlertIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { useEngineStore } from "@/stores/engine-store";
import { useProviderStore } from "@/stores/provider-store";
import { useUiStore } from "@/stores/ui-store";
import { explainError } from "./explain-error";

/**
 * A failed turn: what went wrong in one sentence, what to do about it, and
 * the text as it came under Details (docs/features/ux-polish/spec.md, FR-1).
 */
export function TurnError({ error, provider, prompt }: { error: string; provider?: string; prompt?: string }) {
  const label = useProviderStore((s) => s.providers.find((p) => p.id === provider)?.label);
  const name = label ?? (provider ? provider.charAt(0).toUpperCase() + provider.slice(1) : undefined);
  const explained = explainError(error, name);
  const [open, setOpen] = React.useState(false);
  const engine = useEngineStore.getState;

  const action =
    explained.kind === "auth" ? (
      <Button size="xs" variant="outline" onClick={() => useUiStore.getState().openSettings("providers")}>
        Open Providers
      </Button>
    ) : explained.kind === "context" ? (
      <Button size="xs" variant="outline" onClick={() => void engine().compact()}>
        Compact
      </Button>
    ) : explained.kind !== "other" && prompt ? (
      <Button size="xs" variant="outline" onClick={() => void engine().sendPrompt(prompt)}>
        Retry
      </Button>
    ) : null;
  const details = explained.detail.trim() !== explained.title.trim();

  return (
    <div className="flex flex-col gap-1.5" role="alert">
      <div className="flex items-start gap-2 text-sm text-destructive" data-selectable>
        <CircleAlertIcon className="mt-0.5 size-4 shrink-0" />
        <span>{explained.title}</span>
      </div>
      {(action || details) && (
        <div className="flex items-center gap-1.5 pl-6">
          {action}
          {details && (
            <Button size="xs" variant="ghost" className="gap-1 text-muted-foreground" aria-expanded={open} onClick={() => setOpen((v) => !v)}>
              <ChevronRightIcon className={cn("size-3 transition-transform", open && "rotate-90")} />
              Details
            </Button>
          )}
        </div>
      )}
      {open && (
        <pre
          className="ml-6 max-h-48 overflow-auto rounded-md bg-muted/60 px-2.5 py-2 font-mono text-xs break-all whitespace-pre-wrap text-muted-foreground"
          data-selectable
        >
          {explained.detail}
        </pre>
      )}
    </div>
  );
}
