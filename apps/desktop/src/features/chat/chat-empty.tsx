import * as React from "react";
import { ArrowUpRightIcon, FolderIcon } from "lucide-react";
import { EvoLogo } from "@/components/icons/evo-logo";
import { cn } from "@/lib/utils";
import { git } from "@/lib/git";
import { inTauri } from "@/lib/platform";
import { usePaneChat, usePaneEngine } from "@/stores/engine-store";
import { useSessionStore } from "@/stores/session-store";
import { composerDraft } from "./chat-composer";
import { suggestionsFor } from "./suggestions";

/** First screen of a chat: where the agent works, and one-click starters (AI Elements · Suggestion). */
export function ChatEmpty() {
  // The chat of this pane (split view), and what its folder is: suggestions fit it (ux-polish FR-8).
  const chat = usePaneChat();
  const workspace = useSessionStore((s) => s.workspaceOf(chat));
  const cwd = usePaneEngine((s) => s.cwd);
  const [repo, setRepo] = React.useState<{ changed: number } | null>(null);
  React.useEffect(() => {
    setRepo(null);
    if (!workspace || !cwd || !inTauri) return;
    let live = true;
    git
      .status(cwd)
      .then((status) => live && setRepo(status ? { changed: status.files.length } : null))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [workspace, cwd]);
  const suggestions = suggestionsFor({ workspace: Boolean(workspace), repo: repo !== null, changed: repo?.changed ?? 0 });

  return (
    <div className="flex h-full min-w-0 flex-col items-center justify-center px-6 py-12">
      <div className="flex w-full max-w-xl flex-col items-center text-center">
        <EvoLogo className="size-11 rounded-xl shadow-sm ring-1 ring-border" />

        {workspace && (
          <span className="mt-5 inline-flex max-w-full items-center gap-1.5 rounded-full border bg-background px-2.5 py-0.5 text-xs text-muted-foreground">
            <FolderIcon className="size-3.5 shrink-0" />
            <span className="truncate">{workspace.name}</span>
          </span>
        )}

        <h1 className={cn("text-2xl font-semibold tracking-tight", workspace ? "mt-3" : "mt-5")}>
          What should we work on?
        </h1>
        <p className="mt-2 text-sm text-balance text-muted-foreground">
          {workspace
            ? "The agent can read this folder, run commands and make changes."
            : "Open a folder from the header to work on a project, or just ask."}
        </p>

        <div className="mt-8 grid w-full grid-cols-1 gap-2 sm:grid-cols-2">
          {suggestions.map((s) => (
            <button
              key={s.label}
              type="button"
              onClick={() => composerDraft.set(s.prompt)}
              className="group flex items-start gap-3 rounded-xl border bg-background p-3 text-left transition-colors outline-none hover:border-foreground/20 hover:bg-accent/50 focus-visible:ring-2 focus-visible:ring-ring/50"
            >
              <span className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-muted text-muted-foreground transition-colors group-hover:bg-background group-hover:text-foreground">
                <s.icon className="size-4" />
              </span>
              <span className="min-w-0 flex-1">
                <span className="block text-sm font-medium">{s.label}</span>
                <span className="block truncate text-xs text-muted-foreground">{s.hint}</span>
              </span>
              <ArrowUpRightIcon className="size-3.5 shrink-0 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100 group-focus-visible:opacity-100" />
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
