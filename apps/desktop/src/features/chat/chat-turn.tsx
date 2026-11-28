import * as React from "react";
import { CheckIcon, ChevronRightIcon, CopyIcon, MessagesSquareIcon } from "lucide-react";
import { MessageAction } from "@/components/ai-elements/message-action";
import { Reasoning, seconds } from "@/components/ai-elements/reasoning";
import { Tool } from "@/components/ai-elements/tool";
import { TurnError } from "./turn-error";
import { Markdown } from "@/components/markdown";
import { openFromChat } from "@/components/open-file-ref";
import { AgentIcon } from "@/components/icons/agent-icon";
import { Message, MessageContent, MessageFooter } from "@/components/ui/message";
import { Spinner } from "@/components/ui/spinner";
import {
  Trace,
  TraceCollapsible,
  TraceCollapsibleContent,
  TraceCollapsibleTrigger,
  TraceLine,
  TraceTitle,
} from "@/components/ui/trace";
import { cn } from "@/lib/utils";
import { useWaking } from "@/stores/engine-store";
import { TurnChanges } from "./turn-changes";
import { UserPrompt } from "./user-message";
import { MessageMenu } from "./message-menu";
import { replyMarkdown } from "./message-text";
import { SentFiles, type SentFile } from "@/components/ai-elements/sent-files";
import { KeptWorktree, childRun } from "@/components/ai-elements/agent-run";
import { noteKind, turnReply, type NoteKind, type ToolPart, type Turn } from "./transcript";
import { groupTitle, noteLabel, runningInBackground, runningStep, segments, stepLine, turnMetaLine, type Process } from "./turn-view";

// One turn, mapped the way DeepSeek Harness maps it, on shadcn/ui AI Elements:
// - the person's text is a Bubble (Message);
// - the agent's parts keep the model's order; consecutive reasoning and tool
//   rows form a process group on Trace whose title names what is happening
//   while it runs ("Running commands") and summarises it after ("Read files
//   and ran commands");
// - text parts are the reply, as markdown, always visible;
// - once the turn completes its process folds behind "Completed in 1m 12s";
// - a running turn starts with a live "Working for 12s" line, so it stays put
//   while the process and reply grow below it.

/**
 * An automated message as an event line: a background subagent's report, a
 * nested session's note, mail from an agent, or the wake that reads them.
 */
function BackgroundNote({ kind, text }: { kind: NoteKind; text: string }) {
  const agents = kind === "agent-report" || kind === "agents-done";
  return (
    <div className="flex items-center justify-center gap-2 text-xs text-muted-foreground">
      <span className="h-px flex-1 bg-border" />
      {agents ? <AgentIcon background className="size-3.5" /> : <MessagesSquareIcon className="size-3.5" />}
      <span className="max-w-[70%] truncate">{noteLabel(kind, text.trim())}</span>
      <span className="h-px flex-1 bg-border" />
    </div>
  );
}

export function UserMessage({ turn }: { turn: Turn }) {
  // Only a message with the whole structure of an automated one is an event;
  // text that merely starts like one is the person's own.
  const kind = noteKind(turn.prompt);
  if (kind) return <BackgroundNote kind={kind} text={turn.prompt} />;
  return <UserPrompt turn={turn} />;
}

function TurnMeta({ turn }: { turn: Turn }) {
  const line = turnMetaLine(turn.meta);
  if (!line) return null;
  return <span className="font-mono text-xs text-muted-foreground tabular-nums">{line}</span>;
}

function ProcessRows({ parts, running }: { parts: Process; running: boolean }) {
  return (
    <Trace>
      {parts.map((part) =>
        part.kind === "reasoning" ? (
          <Reasoning key={part.key} text={part.text} streaming={part.streaming} startedAt={part.startedAt} endedAt={part.endedAt} />
        ) : (
          <Tool key={part.key} part={part} streaming={running} />
        ),
      )}
    </Trace>
  );
}

type ProcessGroupProps = { parts: Process; running: boolean; live: boolean };

/**
 * `segments` rebuilds every group's array on each delta, but a part object only
 * changes when that part does: compare them one by one, so streaming text does
 * not re-render the tool rows above it.
 */
function sameGroup(a: ProcessGroupProps, b: ProcessGroupProps): boolean {
  return (
    a.running === b.running &&
    a.live === b.live &&
    a.parts.length === b.parts.length &&
    a.parts.every((part, index) => part === b.parts[index])
  );
}

/** A process group: its rows, under a title once there is more than one. */
const ProcessGroup = React.memo(function ProcessGroup({ parts, running, live: turnLive }: ProcessGroupProps) {
  const [open, setOpen] = React.useState<boolean>();
  // A background agent keeps its group live after the turn ended (until it ends, or its engine exits).
  const background = parts.some(runningInBackground);
  const live = turnLive || background;
  if (parts.length === 1) return <ProcessRows parts={parts} running={running} />;
  return (
    <TraceCollapsible open={open ?? live} onOpenChange={setOpen}>
      <TraceCollapsibleTrigger>
        <TraceLine status={live ? "running" : undefined}>
          <TraceTitle>{groupTitle(parts, live)}</TraceTitle>
        </TraceLine>
      </TraceCollapsibleTrigger>
      <TraceCollapsibleContent className="max-h-[min(400px,50vh)] overflow-y-auto">
        <ProcessRows parts={parts} running={running} />
      </TraceCollapsibleContent>
    </TraceCollapsible>
  );
}, sameGroup);

/**
 * A process group in the quick entry box: one line per step, no details,
 * output or diffs (docs/features/quick-entry/spec.md, FR-13).
 */
const CompactSteps = React.memo(function CompactSteps({ parts, running }: ProcessGroupProps) {
  const lines = parts.flatMap((part) => {
    const line = stepLine(part, running);
    return line ? [{ key: part.key, ...line }] : [];
  });
  if (!lines.length) return null;
  return (
    <Trace className="gap-1">
      {lines.map((line) => (
        <div key={line.key}>
          <TraceLine status={line.status}>
            {line.status === "running" && <Spinner className="size-3.5" />}
            <TraceTitle>{line.label}</TraceTitle>
          </TraceLine>
        </div>
      ))}
    </Trace>
  );
}, sameGroup);

/**
 * "Working for 12s · running commands", ticking while the turn runs; names
 * the step that runs now (ux-polish FR-12).
 */
function RunningStatus({ startedAt, parts }: { startedAt: number; parts: Turn["parts"] }) {
  // A new chat's first message waits for its engine (ux-polish FR-14).
  const waking = useWaking();
  const step = React.useMemo(() => runningStep(parts), [parts]);
  const [now, setNow] = React.useState(() => Date.now());
  React.useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, []);
  return (
    <div className="flex items-center gap-2 text-sm text-muted-foreground" role="status" aria-live="polite">
      <Spinner />
      <span className="shimmer">
        {waking ? (
          "Starting the chat engine…"
        ) : (
          <>
            Working for {seconds(now - startedAt)}
            {step && ` · ${step}`}
          </>
        )}
      </span>
    </div>
  );
}

export function AssistantMessage({
  turn,
  compact = false,
  onOpenInEvo,
}: {
  turn: Turn;
  /**
   * The quick entry box: steps as single lines, nothing folds, no changes,
   * checkpoints, worktrees or message menu (quick-entry FR-12, FR-13).
   */
  compact?: boolean;
  /** Compact only: a failed turn offers to continue in the main window (FR-20). */
  onOpenInEvo?: () => void;
}) {
  const [copied, setCopied] = React.useState(false);
  const running = turn.status === "running";
  const all = React.useMemo(() => segments(turn.parts), [turn.parts]);
  const hasProcess = all.some((segment) => segment.kind === "process");
  // A completed turn folds its process; failed or stopped turns stay open.
  const foldable = !compact && turn.status === "done" && hasProcess;
  const [expanded, setExpanded] = React.useState(false);
  // Folded, it keeps showing a background agent that is still running.
  const shown =
    foldable && !expanded
      ? all.filter(
          (segment) =>
            segment.kind === "text" ||
            segment.kind === "widget" ||
            segment.parts.some(
              (part) =>
                // A running background agent, or a session card to act on.
                runningInBackground(part) || (part.kind === "tool" && part.name === "start_session" && part.phase === "done"),
            ),
        )
      : all;
  const reply = turnReply(turn);
  // Files the agent sent stay in sight when the process folds: they are the
  // turn's deliverables.
  const sentFiles = foldable && !expanded
    ? turn.parts.flatMap((part) => {
        const sent = part.kind === "tool" && part.name === "send_file" && !part.details?.externalToolResult?.isError ? part.details?.externalToolResult?.structuredContent : undefined;
        return sent && Array.isArray(sent.files) && sent.files.length ? [{ key: part.key, files: sent.files as SentFile[], caption: sent.caption as string | undefined }] : [];
      })
    : [];
  // So do subagents' kept worktrees: their changes wait for the person.
  const keptWorktrees = foldable && !expanded
    ? turn.parts.filter(
        (part): part is ToolPart =>
          part.kind === "tool" && (part.name === "agent" || part.name === "message_agent") && Boolean(childRun(part)?.worktree?.kept),
      )
    : [];

  React.useEffect(() => {
    if (!copied) return;
    const timeout = setTimeout(() => setCopied(false), 2000);
    return () => clearTimeout(timeout);
  }, [copied]);

  const body = (
      <Message>
        <MessageContent className={compact ? "gap-3" : "gap-4"}>
          {running && <RunningStatus startedAt={turn.startedAt} parts={turn.parts} />}

          {foldable && (
            <button
              type="button"
              onClick={() => setExpanded((value) => !value)}
              className="group/fold flex w-fit items-center gap-1 text-sm text-muted-foreground transition-colors hover:text-foreground"
              aria-expanded={expanded}
            >
              Completed in {seconds(turn.meta.durationMs ?? (turn.endedAt ?? turn.startedAt) - turn.startedAt)}
              <ChevronRightIcon className={cn("size-4 transition-transform", expanded && "rotate-90")} />
            </button>
          )}

          {shown.map((segment, index) =>
            segment.kind === "text" ? (
              <Markdown key={segment.key} streaming={segment.streaming} blocks onLocalLink={openFromChat}>
                {segment.text}
              </Markdown>
            ) : segment.kind === "widget" ? (
              <Trace key={segment.key}>
                <Tool part={segment.part} streaming={running} />
              </Trace>
            ) : compact ? (
              <CompactSteps key={segment.key} parts={segment.parts} running={running} live={false} />
            ) : (
              <ProcessGroup
                key={segment.key}
                parts={segment.parts}
                running={running}
                live={running && index === all.length - 1}
              />
            ),
          )}

          {sentFiles.map((sent) => (
            <div key={sent.key} className="@container -ml-7">
              <SentFiles files={sent.files} caption={sent.caption} />
            </div>
          ))}

          {keptWorktrees.map((part) => (
            <KeptWorktree key={part.key} part={part} />
          ))}

          {!compact && !running && turn.checkpoint && <TurnChanges turn={{ ...turn, checkpoint: turn.checkpoint }} />}

          {turn.error && <TurnError error={turn.error} provider={turn.meta.provider} prompt={turn.prompt || undefined} />}
          {compact && turn.error && onOpenInEvo && (
            <button
              type="button"
              onClick={onOpenInEvo}
              className="w-fit text-sm text-muted-foreground underline-offset-4 transition-colors hover:text-foreground hover:underline"
            >
              Open in Evo
            </button>
          )}
          {turn.status === "aborted" && <p className="text-sm text-muted-foreground">Stopped.</p>}

          {!running && (
            <MessageFooter className="px-0">
              {reply && (
                <MessageAction
                  label={copied ? "Copied" : "Copy"}
                  onClick={() => {
                    void navigator.clipboard.writeText(replyMarkdown(reply));
                    setCopied(true);
                  }}
                >
                  {copied ? <CheckIcon /> : <CopyIcon />}
                </MessageAction>
              )}
              {!compact && <TurnMeta turn={turn} />}
            </MessageFooter>
          )}
        </MessageContent>
      </Message>
  );
  if (compact) return body;
  return (
    <MessageMenu turn={turn} text={reply} markdown>
      {body}
    </MessageMenu>
  );
}
