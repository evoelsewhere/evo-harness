import * as React from "react";
import { ArrowUpRightIcon, MessagesSquareIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { ToolPart } from "@/features/chat/transcript";
import { chatWindowId } from "@/lib/windows";
import { useEngineStore } from "@/stores/engine-store";
import { useSessionStore } from "@/stores/session-store";
import { useUiStore } from "@/stores/ui-store";

// A `start_session` call's card. By default the agent only proposes a nested
// session: the person starts it here (or dismisses it). Started ones link to
// the session. The proposal lives in the call's result
// (`details.externalToolResult.structuredContent`) and its brief in the call's
// arguments, so the card survives a reload. The shell's mesh hub keeps what
// became of a proposal (started once, or dismissed); the outcome is also kept
// here, so the card draws without asking it.

interface Proposal {
  status: "proposed" | "started";
  proposalId?: string;
  sessionId?: string;
  title: string;
  /** Only in chats saved by earlier builds: the brief is the call's `prompt` argument. */
  prompt?: string;
  context?: "fresh" | "fork";
  worktree?: boolean;
}

export const proposalOf = (part: ToolPart): Proposal | undefined => {
  // A refused call (`isError`) proposed nothing.
  if (part.details?.externalToolResult?.isError) return undefined;
  const structured = part.details?.externalToolResult?.structuredContent;
  return structured && typeof structured === "object" && typeof structured.title === "string"
    ? (structured as Proposal)
    : undefined;
};

/** The session's brief: the call's `prompt` argument. */
const briefOf = (part: ToolPart, proposal: Proposal): string => {
  const prompt = (part.args as { prompt?: unknown } | null | undefined)?.prompt;
  return typeof prompt === "string" && prompt ? prompt : (proposal.prompt ?? "");
};

/** How long a started session's row may take to reach this window before the archive is searched. */
const ROW_WAIT_MS = 1500;
let archiveLookup: Promise<void> | undefined;
/** Load the archived chats (one read for every card that asks at once). */
const lookUpArchived = () =>
  (archiveLookup ??= useSessionStore
    .getState()
    .loadArchived()
    .finally(() => {
      archiveLookup = undefined;
    }));

export function SessionProposal({ part }: { part: ToolPart }) {
  const proposal = proposalOf(part);
  const chat = useEngineStore((s) => s.activeId);
  const startChildSession = useEngineStore((s) => s.startChildSession);
  const dismissChildProposal = useEngineStore((s) => s.dismissChildProposal);
  const selectSession = useEngineStore((s) => s.selectSession);
  const outcome = useUiStore((s) => (proposal?.proposalId ? s.proposals[proposal.proposalId] : undefined));
  const settle = useUiStore((s) => s.settleProposal);
  const sessionId = proposal?.status === "started" ? proposal.sessionId : outcome?.sessionId;
  // The session's row: among the live chats, or archived (loaded on demand).
  const row = useSessionStore((s) =>
    sessionId ? (s.sessions.find((x) => x.id === sessionId) ?? s.archived.find((x) => x.id === sessionId)) : undefined,
  );
  const loaded = useSessionStore((s) => s.loaded);
  const [looked, setLooked] = React.useState<string | null>(null);
  const [starting, setStarting] = React.useState(false);
  const [expanded, setExpanded] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const missing = Boolean(sessionId) && loaded && !row;
  // Not among the live chats: it may be archived, or it was deleted.
  React.useEffect(() => {
    if (!missing || !sessionId || looked === sessionId) return;
    const timer = window.setTimeout(() => void lookUpArchived().finally(() => setLooked(sessionId)), ROW_WAIT_MS);
    return () => window.clearTimeout(timer);
  }, [missing, sessionId, looked]);
  if (!proposal) return null;

  const started = Boolean(sessionId);
  const dismissed = !started && outcome?.status === "dismissed";
  const gone = missing && looked === sessionId;
  const brief = briefOf(part, proposal);

  async function start() {
    if (!chat || !proposal?.proposalId) return;
    setStarting(true);
    setError(null);
    const id = await startChildSession(
      chat,
      { title: proposal.title, prompt: brief, context: proposal.context, worktree: proposal.worktree },
      proposal.proposalId,
    )
      .catch((error: unknown) => {
        // The hub says why: the limit of running sessions, or dismissed in another window.
        setError(error instanceof Error ? error.message : String(error));
        return null;
      })
      .finally(() => setStarting(false));
    if (id) settle(proposal.proposalId, { status: "started", sessionId: id });
  }

  function dismiss() {
    if (!proposal?.proposalId) return;
    settle(proposal.proposalId, { status: "dismissed" });
    if (chat) void dismissChildProposal(chat, proposal.proposalId);
  }

  return (
    <div className="flex max-w-xl flex-col gap-2.5 rounded-xl bg-card px-4 py-3.5 shadow-border-small">
      <div className="flex items-start gap-2.5">
        <MessagesSquareIcon className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
        <div className="min-w-0 flex-1">
          <p className="text-sm font-medium">{row?.title ?? proposal.title}</p>
          <p className="text-xs text-muted-foreground">
            {started
              ? `Nested session${row?.archivedAt != null ? " · archived" : ""}`
              : dismissed
                ? "Dismissed"
                : `Proposed session${proposal.context === "fork" ? " · forks this conversation" : ""}${
                    proposal.worktree ? " · in its own worktree" : ""
                  }`}
          </p>
        </div>
      </div>
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        className="text-left text-[13px] whitespace-pre-wrap text-muted-foreground hover:text-foreground"
      >
        <span className={expanded ? undefined : "line-clamp-3"}>{brief}</span>
      </button>
      {error && !started && <p className="text-[13px] text-destructive">{error}</p>}
      {started ? (
        gone ? (
          <p className="text-[13px] text-muted-foreground">This session was deleted.</p>
        ) : chatWindowId ? (
          // A chat window shows only its own chat.
          <p className="text-[13px] text-muted-foreground">Open it from the main window's sidebar.</p>
        ) : (
          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              className="h-7 gap-1.5"
              disabled={!row}
              onClick={() => void selectSession(sessionId!)}
            >
              Open session
              <ArrowUpRightIcon className="size-3.5" />
            </Button>
          </div>
        )
      ) : (
        !dismissed && (
          <div className="flex items-center gap-2">
            <Button size="sm" className="h-7" disabled={starting || !chat || !proposal.proposalId} onClick={() => void start()}>
              {starting ? "Starting…" : "Start session"}
            </Button>
            <Button variant="ghost" size="sm" className="h-7" disabled={starting} onClick={dismiss}>
              Dismiss
            </Button>
          </div>
        )
      )}
    </div>
  );
}
