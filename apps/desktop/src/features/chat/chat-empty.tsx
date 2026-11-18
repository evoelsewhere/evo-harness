import * as React from "react";
import { FolderIcon, FolderOpenIcon, GitBranchIcon } from "lucide-react";
import { EvoLogo } from "@/components/icons/evo-logo";
import { git } from "@/lib/git";
import { inTauri } from "@/lib/platform";
import { usePaneChat, usePaneEngine } from "@/stores/engine-store";
import { useSessionStore } from "@/stores/session-store";

/** Where a new chat works: its folder and, in a repository, the branch and changes. */
export interface ChatPlace {
  workspace: string | null;
  repo: { branch: string | null; changed: number } | null;
}

/** The new chat's folder and repository, read while the empty screen shows. */
export function useChatPlace(active: boolean): ChatPlace {
  // The chat of this pane (split view) and its folder.
  const chat = usePaneChat();
  const workspace = useSessionStore((s) => s.workspaceOf(chat));
  const cwd = usePaneEngine((s) => s.cwd);
  const [repo, setRepo] = React.useState<ChatPlace["repo"]>(null);
  React.useEffect(() => {
    setRepo(null);
    if (!active || !workspace || !cwd || !inTauri) return;
    let live = true;
    git
      .status(cwd)
      .then((status) => live && setRepo(status ? { branch: status.branch, changed: status.files.length } : null))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [active, workspace, cwd]);
  return { workspace: workspace?.name ?? null, repo };
}

/**
 * First screen of a chat, above the centred composer: the mark and the
 * question, then where the agent works (folder, branch) right over the field.
 */
export function ChatEmpty({ place }: { place: ChatPlace }) {
  return (
    <div className="mx-auto flex w-full max-w-(--chat-width) min-w-0 flex-col px-6">
      <div className="flex items-center justify-center gap-3">
        <EvoLogo className="size-9 shrink-0 rounded-lg shadow-sm ring-1 ring-border" />
        <h1 className="truncate text-2xl font-semibold tracking-tight">What should we work on?</h1>
      </div>

      <div className="mt-9 mb-2 flex min-w-0 items-center gap-4 px-1.5 text-xs text-muted-foreground">
        {place.workspace ? (
          <span className="inline-flex min-w-0 items-center gap-1.5">
            <FolderOpenIcon className="size-3.5 shrink-0" />
            <span className="truncate">{place.workspace}</span>
          </span>
        ) : (
          <span className="inline-flex min-w-0 items-center gap-1.5" title="Open a folder from the header to work on a project">
            <FolderIcon className="size-3.5 shrink-0" />
            <span className="truncate">No folder</span>
          </span>
        )}
        {place.repo?.branch && (
          <span className="inline-flex min-w-0 items-center gap-1.5">
            <GitBranchIcon className="size-3.5 shrink-0" />
            <span className="truncate">{place.repo.branch}</span>
            {place.repo.changed > 0 && <span className="shrink-0 tabular-nums">· {place.repo.changed} changed</span>}
          </span>
        )}
      </div>
    </div>
  );
}
