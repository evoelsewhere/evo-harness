import * as React from "react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { CopyIcon, FolderIcon, GitBranchIcon, SquareIcon } from "lucide-react";
import { Markdown } from "@/components/markdown";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { ChildRun } from "@/engine/mesh";
import { openInApp, useOpenApps, type OpenApp } from "@/lib/open-apps";
import { cn } from "@/lib/utils";
import { worktrees } from "@/lib/worktrees";
import { usePaneEngine } from "@/stores/engine-store";
import { useUiStore, worktreeKey } from "@/stores/ui-store";
import { WorktreeReview } from "./worktree-review";
import { Trace } from "@/components/ui/trace";
import type { ToolPart } from "@/features/chat/transcript";
import { Tool } from "./tool";

// The body of an `agent` or `message_agent` tool call: what the subagent is
// doing (its own tool calls as nested rows, the end of its latest text), how
// much it has done, and its report. The engine streams this as
// `details.child` (crates/evo-engine/src/tools/agent.rs) and keeps it on the
// result, so a reopened chat shows it too
// (docs/features/subagents-and-nested-sessions/spec.md FR-5, FR-13).

export type { ChildRun };

export const childRun = (part: ToolPart): ChildRun | undefined =>
  part.details && typeof part.details === "object" ? (part.details.child as ChildRun | undefined) : undefined;

/**
 * A subagent's kept worktree, outside its agent view: a folded turn keeps it
 * in sight (`chat-turn.tsx`), since its changes wait for the person. Nothing
 * once they were applied or discarded.
 */
export function KeptWorktree({ part }: { part: ToolPart }) {
  const worktree = childRun(part)?.worktree;
  const repo = usePaneEngine((s) => s.cwd);
  const settled = useUiStore((s) => (worktree?.path ? s.settledWorktrees[worktreeKey(worktree.path)] : undefined));
  if (!worktree?.kept || settled) return null;
  return <WorktreeBadge worktree={worktree} repo={repo} live={false} />;
}

/** Rows shown while running; the rest fold into "N earlier steps". */
const LIVE_ROWS = 6;
/** A report longer than this (characters or lines) starts collapsed. */
const LONG_REPORT_CHARS = 1200;
const LONG_REPORT_LINES = 16;
/** How the shell refuses a path it no longer manages (crates/evo-worktrees `managed`). */
const NOT_MANAGED = "is not a managed worktree";

function seconds(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  return s < 60 ? `${s}s` : `${Math.floor(s / 60)}m ${s % 60}s`;
}

function tokens(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(n >= 10_000 ? 0 : 1)}k` : String(n);
}

/** Re-render every second while running, for the elapsed time. */
function useTick(active: boolean) {
  const [, setTick] = React.useState(0);
  React.useEffect(() => {
    if (!active) return;
    const timer = window.setInterval(() => setTick((t) => t + 1), 1000);
    return () => window.clearInterval(timer);
  }, [active]);
}

/** The report as Markdown; a long one starts collapsed behind "Show more". */
function Report({ text }: { text: string }) {
  const long = text.length > LONG_REPORT_CHARS || text.split("\n").length > LONG_REPORT_LINES;
  const [open, setOpen] = React.useState(false);
  const id = React.useId();
  return (
    <div className="rounded-lg bg-muted/40 px-3.5 py-3 shadow-hairline">
      <div
        id={id}
        className={cn(
          long && !open && "max-h-48 overflow-hidden [mask-image:linear-gradient(to_bottom,black_65%,transparent)]",
        )}
      >
        <Markdown>{text}</Markdown>
      </div>
      {long && (
        <button
          type="button"
          aria-expanded={open}
          aria-controls={id}
          onClick={() => setOpen((value) => !value)}
          className="mt-2 text-xs text-muted-foreground hover:text-foreground"
        >
          {open ? "Show less" : "Show more"}
        </button>
      )}
    </div>
  );
}

/**
 * A kept worktree: its branch and what to do with it (FR-13): open the
 * folder in another app, show the changes (the review dialog, which can apply
 * them), or discard it.
 */
function WorktreeBadge({
  worktree,
  repo,
  live,
}: {
  worktree: NonNullable<ChildRun["worktree"]>;
  /** The repository the worktree belongs to: this chat's folder. */
  repo: string | undefined;
  live: boolean;
}) {
  const path = worktree.path ?? "";
  const branch = worktree.branch;
  const settled = useUiStore((s) => (path ? s.settledWorktrees[worktreeKey(path)] : undefined));
  const settle = useUiStore((s) => s.settleWorktree);
  const apps = useOpenApps();
  const [reviewing, setReviewing] = React.useState(false);
  const [confirming, setConfirming] = React.useState(false);
  const [busy, setBusy] = React.useState(false);
  // Applied or discarded somewhere else (Settings → Sandbox, another window).
  const [gone, setGone] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const places = apps.filter((app) => app.kind !== "terminal");
  const terminals = apps.filter((app) => app.kind === "terminal");

  function openIn(app: OpenApp) {
    if (!path) return;
    setError(null);
    openInApp(app.id, path).catch((e: unknown) => setError(`Could not open in ${app.name}: ${String(e)}`));
  }

  async function showChanges() {
    if (!repo || !path) return;
    setBusy(true);
    setError(null);
    try {
      await worktrees.changes(repo, path);
      setReviewing(true);
    } catch (e) {
      if (String(e).includes(NOT_MANAGED)) setGone(true);
      // Anything else: the dialog says what went wrong.
      else setReviewing(true);
    } finally {
      setBusy(false);
    }
  }

  async function discard() {
    if (!repo || !path) return;
    setBusy(true);
    setError(null);
    try {
      await worktrees.discard(repo, path);
      settle(path, "discarded");
    } catch (e) {
      if (String(e).includes(NOT_MANAGED)) setGone(true);
      else setError(String(e));
    } finally {
      setBusy(false);
      setConfirming(false);
    }
  }

  const appItem = (app: OpenApp) => (
    <DropdownMenuItem key={app.id} onClick={() => openIn(app)}>
      {app.icon ? (
        <img src={app.icon} alt="" draggable={false} className="size-4 shrink-0 object-contain" />
      ) : (
        <FolderIcon className="text-muted-foreground" />
      )}
      <span className="truncate">{app.name}</span>
    </DropdownMenuItem>
  );
  const actions = !live && !settled && !gone && Boolean(repo) && path !== "";

  return (
    <div className="flex min-w-0 flex-col gap-1 text-xs text-muted-foreground">
      <div className="flex min-w-0 flex-wrap items-center gap-2">
        <GitBranchIcon className="size-3.5 shrink-0" />
        {settled || gone ? (
          <span>
            {settled ? `Changes ${settled}` : "Worktree already removed"} · <code className="font-mono">{branch}</code>
          </span>
        ) : (
          <span className="min-w-0 truncate" title={path}>
            Changes kept on <code className="font-mono">{branch}</code>
          </span>
        )}
        {actions && !confirming && (
          <>
            <DropdownMenu>
              <DropdownMenuTrigger render={<Button variant="outline" size="sm" className="h-6 shrink-0 px-2 text-xs" />}>
                Open in…
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start" className="w-56">
                <DropdownMenuGroup>
                  <DropdownMenuLabel>Open in</DropdownMenuLabel>
                  {places.length > 0 ? (
                    places.map(appItem)
                  ) : (
                    <DropdownMenuItem onClick={() => void revealItemInDir(path).catch((e: unknown) => setError(String(e)))}>
                      <FolderIcon />
                      File manager
                    </DropdownMenuItem>
                  )}
                </DropdownMenuGroup>
                {terminals.length > 0 && (
                  <>
                    <DropdownMenuSeparator />
                    <DropdownMenuGroup>{terminals.map(appItem)}</DropdownMenuGroup>
                  </>
                )}
                <DropdownMenuSeparator />
                <DropdownMenuItem onClick={() => void navigator.clipboard?.writeText(path).catch(() => undefined)}>
                  <CopyIcon />
                  Copy path
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
            <Button
              variant="outline"
              size="sm"
              className="h-6 shrink-0 px-2 text-xs"
              disabled={busy}
              onClick={() => void showChanges()}
            >
              Show changes
            </Button>
            <Button
              variant="ghost"
              size="sm"
              className="h-6 shrink-0 px-2 text-xs text-destructive hover:text-destructive"
              disabled={busy}
              onClick={() => setConfirming(true)}
            >
              Discard
            </Button>
          </>
        )}
        {actions && confirming && (
          <>
            <span>Throw these changes and the worktree away?</span>
            <Button variant="outline" size="sm" className="h-6 shrink-0 px-2 text-xs" disabled={busy} onClick={() => setConfirming(false)}>
              Keep
            </Button>
            <Button variant="destructive" size="sm" className="h-6 shrink-0 px-2 text-xs" disabled={busy} onClick={() => void discard()}>
              {busy ? "Discarding…" : "Discard"}
            </Button>
          </>
        )}
      </div>
      {error && <p className="whitespace-pre-wrap text-destructive">{error}</p>}
      {repo && path && (
        <WorktreeReview open={reviewing} onOpenChange={setReviewing} repo={repo} path={path} branch={branch} />
      )}
    </div>
  );
}

export function AgentRun({ part, live }: { part: ToolPart; live: boolean }) {
  const child = childRun(part);
  useTick(live);
  const chat = usePaneEngine((s) => s.activeId);
  const cancel = usePaneEngine((s) => s.cancelBackgroundAgent);
  // The repository the worktree belongs to: this chat's folder.
  const repo = usePaneEngine((s) => s.cwd);
  // A background agent's call ended with "started": only the agent's own state
  // and report count, never the call's output. The report is the whole one
  // (`report`); `text` is only the end of what it wrote last.
  const background = Boolean(child?.background);
  const failed = background ? child?.status === "error" || child?.status === "cancelled" : part.phase === "error";
  const pending = background && child?.status === "done" && child.reportPending === true;
  const report = background
    ? child?.status === "done" && !pending
      ? (child.report ?? child.text)?.trim()
      : undefined
    : part.output;
  const problem = !failed
    ? undefined
    : child?.status === "cancelled"
      ? "Cancelled"
      : background
        ? child?.error || child?.report || "The agent stopped without a report."
        : (child?.error ?? part.output);
  // What a `message_agent` call told the agent it continues.
  const message =
    part.name === "message_agent" && typeof (part.args as { message?: unknown } | undefined)?.message === "string"
      ? ((part.args as { message: string }).message)
      : undefined;
  const rows = child?.tools ?? [];
  const [all, setAll] = React.useState(false);
  const earlier = live ? Math.max(0, rows.length - LIVE_ROWS) : 0;
  const shown = rows.slice(all ? 0 : earlier);
  // The snapshot's own times: one run of the agent, not the whole call.
  const end = child?.endedAt ?? (live ? Date.now() : background ? undefined : part.endedAt);
  const elapsed = child?.startedAt && end ? Math.max(0, end - child.startedAt) : 0;
  const latest = child?.text?.trim().split("\n").filter(Boolean).at(-1);
  const facts = [
    child?.agentType,
    child?.toolUses ? `${child.toolUses} tool ${child.toolUses === 1 ? "use" : "uses"}` : undefined,
    child?.usage?.totalTokens ? `${tokens(child.usage.totalTokens)} tokens` : undefined,
    elapsed ? seconds(elapsed) : undefined,
    child?.model,
  ].filter(Boolean);

  return (
    <div className="flex flex-col gap-2.5">
      {message && (
        <p className="line-clamp-3 text-[13px] text-muted-foreground" title={message}>
          Message: {message}
        </p>
      )}
      {rows.length > 0 && (
        <Trace className="gap-1.5">
          {earlier > 0 && (
            <button
              type="button"
              aria-expanded={all}
              onClick={() => setAll((value) => !value)}
              className="w-fit text-xs text-muted-foreground hover:text-foreground"
            >
              {all ? "Hide earlier steps" : `${earlier} earlier ${earlier === 1 ? "step" : "steps"}`}
            </button>
          )}
          {shown.map((row) => (
            <Tool
              key={row.callId}
              lineOnly
              streaming={live}
              part={{
                kind: "tool",
                key: row.callId,
                callId: row.callId,
                name: row.name,
                args: row.args ?? {},
                argsText: "",
                phase: row.phase,
                output: "",
                startedAt: 0,
              }}
            />
          ))}
        </Trace>
      )}
      {live && latest && <p className="line-clamp-2 text-[13px] text-muted-foreground italic">{latest}</p>}
      {/* The worktree's place is the badge below, not part of the report. */}
      {!live && !failed && report && <Report text={report.replace(/\n\n\[Its changes are in the worktree [^\]]*\]/, "")} />}
      {!live && pending && (
        <p className="text-[13px] text-muted-foreground">Finished. Its report is delivered with your next message.</p>
      )}
      {!live && failed && problem && <p className="text-[13px] text-destructive">{problem}</p>}
      {live && child?.background && child.agentId && chat && (
        <Button
          variant="outline"
          size="sm"
          className="h-7 w-fit gap-1.5 text-xs"
          onClick={() => void cancel(chat, child.agentId!)}
        >
          <SquareIcon className="size-3" />
          Stop agent
        </Button>
      )}
      {child?.worktree?.kept && <WorktreeBadge worktree={child.worktree} repo={repo} live={live} />}
      {facts.length > 0 && <p className="text-xs text-muted-foreground tabular-nums">{facts.join(" · ")}</p>}
    </div>
  );
}
