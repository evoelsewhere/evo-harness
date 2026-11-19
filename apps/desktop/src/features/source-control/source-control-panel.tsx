// The Source control tool (docs/features/source-control/spec.md, FR-1 – FR-8,
// FR-10, FR-13, FR-15, FR-16): the repository of the active chat's folder —
// branch, the commit box, Staged / Changes lists with file actions, and the
// selected file's diff with hunk actions and line comments. A turn's changes
// (from "Changed N files" under a reply) show here too, read-only.

import * as React from "react";
import {
  ArrowDownIcon,
  ArrowLeftIcon,
  ArrowUpIcon,
  ChevronRightIcon,
  Columns2Icon,
  ExternalLinkIcon,
  FileIcon,
  GitBranchIcon,
  ListIcon,
  ListTreeIcon,
  Maximize2Icon,
  MessageSquarePlusIcon,
  Minimize2Icon,
  MinusIcon,
  PlusIcon,
  RefreshCwIcon,
  RowsIcon,
  SparklesIcon,
  Undo2Icon,
  XIcon,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { Spinner } from "@/components/ui/spinner";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { SourceControlIcon } from "@/components/icons/tool-icons";
import { describeCodeComment } from "@/lib/code-comments";
import { modKey } from "@/lib/platform";
import { type Area, type Hunk, type ScmFile } from "@/lib/scm";
import { cn } from "@/lib/utils";
import { useAnnotationsStore } from "@/stores/annotations-store";
import { useEngineStore } from "@/stores/engine-store";
import { openFile } from "@/stores/files-store";
import { useSourceControlStore } from "@/stores/source-control-store";
import { useUiStore } from "@/stores/ui-store";
import { ChangeRows, Counts, visibleFiles, type RowHandlers } from "./change-tree";
import { ConfirmDialog, type Confirm } from "./confirm-dialog";
import { DiffView, type HunkActions, type LineSelection } from "./diff-view";

/** How often the status is read while the tool shows (FR-3: changes show within 2 s). */
const POLL_MS = 1500;

const baseName = (path: string) => path.split("/").pop() ?? path;
const folderOf = (path: string) => path.split("/").slice(0, -1).join("/");
const joinPath = (root: string, path: string) => `${root.replace(/[\\/]+$/, "")}/${path}`;
const EMPTY: string[] = [];

function Section({
  title,
  count,
  children,
  actions,
}: {
  title: string;
  count: number;
  children: React.ReactNode;
  actions?: React.ReactNode;
}) {
  const [open, setOpen] = React.useState(true);
  return (
    <section className="flex flex-col">
      <div className="group/section flex h-7 items-center gap-1 px-2">
        <button
          type="button"
          onClick={() => setOpen((value) => !value)}
          aria-expanded={open}
          className="flex min-w-0 flex-1 items-center gap-1 text-left text-xs font-medium text-muted-foreground"
        >
          <ChevronRightIcon className={cn("size-3.5 transition-transform", open && "rotate-90")} />
          {title}
          <span className="tabular-nums">{count}</span>
        </button>
        <span className="flex items-center gap-0.5 opacity-0 group-hover/section:opacity-100 focus-within:opacity-100">{actions}</span>
      </div>
      {open && (
        <ul role="listbox" aria-label={title} className="flex flex-col px-1">
          {children}
        </ul>
      )}
    </section>
  );
}

function IconAction({ label, onClick, disabled, children }: { label: string; onClick(): void; disabled?: boolean; children: React.ReactNode }) {
  return (
    <Button variant="ghost" size="icon-xs" className="size-6 text-muted-foreground" aria-label={label} title={label} onClick={onClick} disabled={disabled}>
      {children}
    </Button>
  );
}

/** The commit box: message, Draft, Amend, Commit (FR-6, FR-7, FR-13). */
function CommitBox({ onConfirm }: { onConfirm(confirm: Confirm): void }) {
  const cwd = useSourceControlStore((s) => s.cwd);
  const message = useSourceControlStore((s) => (s.cwd ? (s.messages[s.cwd] ?? "") : ""));
  const staged = useSourceControlStore((s) => s.status?.staged.length ?? 0);
  const state = useSourceControlStore((s) => s.status?.state ?? "normal");
  const conflicted = useSourceControlStore((s) => s.status?.conflicted.length ?? 0);
  const amend = useSourceControlStore((s) => s.amend);
  const busy = useSourceControlStore((s) => s.busy);
  const store = useSourceControlStore.getState();
  const blocked = conflicted > 0 || state === "rebase";
  const canCommit = Boolean(cwd && message.trim() && (staged > 0 || amend) && !busy && !blocked);
  // Why Commit is off, said on the button (audit: a grey button gave no reason).
  const why = blocked
    ? "Resolve the conflicts first"
    : staged === 0 && !amend
      ? "Stage changes to commit them"
      : !message.trim()
        ? "Write a commit message (or Draft message)"
        : `Commit (${modKey}+Enter)`;

  const commit = async (confirmRunning: boolean) => {
    const ask = await store.commit(confirmRunning);
    if (!ask) return;
    const why = [
      ask.running.length ? `The agent is still working in this folder (${ask.running.length === 1 ? "one chat" : `${ask.running.length} chats`}): you may commit half-done work.` : "",
      ask.pushed ? "The last commit is already on the upstream branch: amending it rewrites shared history." : "",
    ]
      .filter(Boolean)
      .join(" ");
    onConfirm({
      title: amend ? "Amend the last commit anyway?" : "Commit anyway?",
      body: why,
      action: amend ? "Amend" : "Commit",
      run: () => void commit(true),
    });
  };

  return (
    <div className="flex flex-col gap-1.5 px-3 pt-2 pb-3">
      <Textarea
        value={message}
        onChange={(event) => store.setMessage(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
            event.preventDefault();
            if (canCommit) void commit(false);
          }
        }}
        placeholder={blocked ? "Resolve the conflicts to commit" : "Commit message"}
        aria-label="Commit message"
        className="max-h-32 min-h-16 resize-none overflow-y-auto text-[13px]"
        disabled={!cwd || blocked}
      />
      <div className="flex items-center gap-2">
        <Button
          variant="ghost"
          size="sm"
          className="h-7 gap-1.5 px-2 text-xs text-muted-foreground"
          disabled={staged === 0 || Boolean(busy)}
          onClick={() => void store.draftMessage()}
          title="Write a message for the staged changes in this repository's style"
        >
          {busy === "draft" ? <Spinner className="size-3.5" /> : <SparklesIcon className="size-3.5" />}
          Draft message
        </Button>
        <label className="ml-auto flex items-center gap-1.5 text-xs text-muted-foreground">
          <Checkbox checked={amend} onCheckedChange={(on) => store.setAmend(Boolean(on))} aria-label="Amend the last commit" />
          Amend
        </label>
        <Button size="sm" className="h-7 px-3 text-xs" disabled={!canCommit} onClick={() => void commit(false)} title={why}>
          {busy === "commit" ? <Spinner className="size-3.5" /> : null}
          {amend ? "Amend" : `Commit${staged ? ` ${staged}` : ""}`}
        </Button>
      </div>
    </div>
  );
}

/** "Comment" on the selected lines of the diff: the comment waits in the composer (FR-8). */
function LineComment({ file, selection, onDone }: { file: string; selection: LineSelection; onDone(): void }) {
  const chat = useEngineStore((s) => s.activeId);
  const [text, setText] = React.useState("");
  const [open, setOpen] = React.useState(false);
  const add = () => {
    if (!chat) return;
    useAnnotationsStore.getState().addCode(chat, {
      id: `c-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`,
      file,
      from: selection.from,
      to: selection.to,
      quote: selection.text,
      instruction: text.trim(),
    });
    setText("");
    setOpen(false);
    onDone();
  };
  if (!open) {
    return (
      <div className="flex items-center gap-2 border-b px-3 py-1.5 text-xs text-muted-foreground">
        <span className="flex-1 truncate">{describeCodeComment({ file, from: selection.from, to: selection.to })} selected</span>
        <Button variant="outline" size="sm" className="h-6 gap-1 px-2 text-xs" onClick={() => setOpen(true)} disabled={!chat}>
          <MessageSquarePlusIcon className="size-3.5" />
          Comment
        </Button>
      </div>
    );
  }
  return (
    <div className="flex flex-col gap-1.5 border-b px-3 py-2">
      <span className="text-xs text-muted-foreground">{describeCodeComment({ file, from: selection.from, to: selection.to })}</span>
      <Textarea
        autoFocus
        value={text}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
            event.preventDefault();
            add();
          } else if (event.key === "Escape") setOpen(false);
        }}
        placeholder="What should the agent do with these lines?"
        className="min-h-14 resize-none text-[13px]"
      />
      <div className="flex justify-end gap-1.5">
        <Button variant="ghost" size="sm" className="h-6 px-2 text-xs" onClick={() => setOpen(false)}>
          Cancel
        </Button>
        <Button size="sm" className="h-6 px-2 text-xs" onClick={add}>
          Add to message
        </Button>
      </div>
    </div>
  );
}

/** The selected file's diff, or why there is none. */
function DiffPane() {
  const diff = useSourceControlStore((s) => s.diff);
  const diffError = useSourceControlStore((s) => s.diffError);
  const selected = useSourceControlStore((s) => s.selected);
  const turn = useSourceControlStore((s) => s.turn);
  const busy = useSourceControlStore((s) => Boolean(s.busy));
  const layout = useUiStore((s) => s.scmDiffLayout);
  const [selection, setSelection] = React.useState<LineSelection | null>(null);
  const store = useSourceControlStore.getState();
  React.useEffect(() => setSelection(null), [diff?.path, diff?.hash]);

  const actions = React.useMemo((): HunkActions | null => {
    if (!diff || !selected || turn) return null;
    const target = (hunk: Hunk) => ({ kind: "hunk" as const, path: diff.path, hunk: hunk.id, expected: diff.hash });
    if (selected.area === "staged") return { primary: { label: "Unstage", run: (hunk) => void store.unstage(target(hunk)) } };
    return {
      primary: { label: "Stage", run: (hunk) => void store.stage(target(hunk)) },
      discard: {
        label: "Discard",
        run: (hunk) => void store.discard(target(hunk), `a change in ${baseName(diff.path)}`),
      },
    };
  }, [diff, selected, turn, store]);

  if (diffError) return <p className="p-4 text-xs text-destructive">{diffError}</p>;
  if (!diff) return <p className="p-4 text-xs text-muted-foreground">Select a file to see its changes.</p>;
  if (!diff.binary && !diff.tooLarge && !diff.eolOnly && diff.hunks.length === 0) {
    return <p className="p-4 text-xs text-muted-foreground">No changes in this file.</p>;
  }
  if (diff.binary || diff.tooLarge || diff.eolOnly) {
    return (
      <p className="p-4 text-xs text-muted-foreground">
        {diff.eolOnly
          ? "Only line endings changed in this file."
          : diff.binary
            ? "A binary file changed. Stage, unstage or discard it as a whole."
            : "This file is too large to show (over 1 MB). Stage, unstage or discard it as a whole."}
      </p>
    );
  }
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {selection && !turn && <LineComment file={diff.path} selection={selection} onDone={() => setSelection(null)} />}
      {/* A definite box: the editor fills it and scrolls inside. */}
      <div className="relative min-h-0 flex-1">
        <DiffView
          path={diff.path}
          old={diff.old ?? ""}
          neu={diff.new ?? ""}
          hunks={diff.hunks}
          layout={layout}
          actions={actions}
          busy={busy}
          onSelect={setSelection}
        />
      </div>
    </div>
  );
}

/** A turn's files (FR-10): its own list, read-only. */
function TurnFiles() {
  const turn = useSourceControlStore((s) => s.turn);
  const current = useSourceControlStore((s) => s.turnFile);
  const store = useSourceControlStore.getState();
  if (!turn) return null;
  return (
    <div className="flex flex-col">
      <div className="flex items-center gap-2 px-2 py-1.5">
        <IconAction label="Back to the folder's changes" onClick={store.leaveTurn}>
          <ArrowLeftIcon className="size-3.5" />
        </IconAction>
        <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground" title={turn.prompt}>
          Changed in the turn “{turn.prompt.slice(0, 80)}”
        </span>
      </div>
      <ul role="listbox" aria-label="Files the turn changed" className="flex flex-col px-1 pb-2">
        {turn.files.map((file) => (
          <li
            key={file.path}
            role="option"
            aria-selected={file.path === current}
            onClick={() => void store.selectTurnFile(file.path)}
            className={cn("flex h-7 items-center gap-2 rounded-md px-2 text-[13px]", file.path === current ? "bg-accent" : "hover:bg-accent/60")}
          >
            <span className="min-w-0 flex-1 truncate">
              {baseName(file.path)}
              <span className="ml-1.5 text-xs text-muted-foreground">{folderOf(file.path)}</span>
            </span>
            <Counts added={file.added} removed={file.removed} />
          </li>
        ))}
      </ul>
    </div>
  );
}

/**
 * The branch in the header; with several repositories in the folder (FR-17)
 * it is also the switch between them, saving a row.
 */
function RepositorySwitch() {
  const repos = useSourceControlStore((s) => s.repos);
  const cwd = useSourceControlStore((s) => s.cwd);
  const status = useSourceControlStore((s) => s.status);
  const where = status?.branch ?? (status?.head ? `Detached at ${status.head}` : "No commits yet");
  const tracks = status?.upstream ? `Tracks ${status.upstream}` : "No upstream branch";
  if (repos.length < 2 || !cwd) {
    return (
      <span className="min-w-0 truncate font-medium" title={tracks}>
        {where}
      </span>
    );
  }
  const items = repos.map((repo) => ({
    value: repo.root,
    label: `${repo.path === "." ? "This folder" : repo.path} · ${repo.branch ?? "detached"}${repo.changed ? ` · ${repo.changed} changed` : ""}`,
  }));
  return (
    <Select value={cwd} items={items} onValueChange={(root) => typeof root === "string" && void useSourceControlStore.getState().pick(root)}>
      <SelectTrigger
        size="sm"
        className="h-7 min-w-0 flex-1 justify-start gap-1 border-transparent bg-transparent px-1.5 text-[13px] font-medium shadow-none hover:bg-accent"
        aria-label={`Repository (${repos.length} in this folder)`}
        title={`${repos.length} repositories in this folder · ${tracks}`}
      >
        <SelectValue />
      </SelectTrigger>
      <SelectContent alignItemWithTrigger={false}>
        {items.map((item) => (
          <SelectItem key={item.value} value={item.value} className="text-xs">
            {item.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

export function SourceControlPanel() {
  const cwd = useEngineStore((s) => s.cwd ?? null);
  const busyTurn = useEngineStore((s) => s.transcript.busy);
  const status = useSourceControlStore((s) => s.status);
  const loaded = useSourceControlStore((s) => s.loaded);
  const error = useSourceControlStore((s) => s.error);
  const selected = useSourceControlStore((s) => s.selected);
  const busy = useSourceControlStore((s) => s.busy);
  const undo = useSourceControlStore((s) => s.undo);
  const turn = useSourceControlStore((s) => s.turn);
  const reveal = useSourceControlStore((s) => s.reveal);
  const layout = useUiStore((s) => s.scmDiffLayout);
  const mode = useUiStore((s) => s.scmListMode);
  const store = useSourceControlStore.getState();
  const [confirm, setConfirm] = React.useState<Confirm | null>(null);
  const listRef = React.useRef<HTMLDivElement>(null);
  const turnFile = useSourceControlStore((s) => s.turnFile);
  const split = useUiStore((s) => s.scmSplit);
  /** The diff has the whole panel (the files and the commit box hide). */
  const [expanded, setExpanded] = React.useState(false);
  const root = status?.root ?? "";
  const closedStaged = useSourceControlStore((s) => s.closed[`${s.cwd}|staged`] ?? EMPTY);
  const closedUnstaged = useSourceControlStore((s) => s.closed[`${s.cwd}|unstaged`] ?? EMPTY);
  const closed = React.useMemo(
    () => ({ staged: new Set(closedStaged), unstaged: new Set(closedUnstaged) }),
    [closedStaged, closedUnstaged],
  );

  // Read now, then while the window shows, on focus, and when a turn starts or ends (FR-3).
  React.useEffect(() => {
    void store.refresh(cwd);
    const tick = () => {
      if (document.visibilityState === "visible") void store.refresh(cwd);
    };
    const timer = window.setInterval(tick, POLL_MS);
    window.addEventListener("focus", tick);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("focus", tick);
    };
  }, [cwd, busyTurn, store]);

  // Opened at a file (summary card, Files; FR-15).
  React.useEffect(() => {
    if (!reveal || !status) return;
    // Files may ask with a case-folded path (Windows drives).
    const wanted = reveal.path.toLowerCase();
    const unstagedFile = status.unstaged.find((f) => f.path.toLowerCase() === wanted);
    const stagedFile = status.staged.find((f) => f.path.toLowerCase() === wanted);
    if (unstagedFile) void store.select(unstagedFile.path, "unstaged");
    else if (stagedFile) void store.select(stagedFile.path, "staged");
  }, [reveal, status, store]);

  // The files in the order they show (closed folders hide theirs), for the keyboard.
  const ordered = React.useMemo(() => {
    const byPath = (list: ScmFile[]) => new Map(list.map((f) => [f.path, f]));
    const staged = byPath(status?.staged ?? []);
    const unstaged = byPath(status?.unstaged ?? []);
    return [
      ...visibleFiles(status?.staged ?? [], mode, closed.staged).map((path) => ({ file: staged.get(path)!, area: "staged" as const })),
      ...visibleFiles(status?.unstaged ?? [], mode, closed.unstaged).map((path) => ({ file: unstaged.get(path)!, area: "unstaged" as const })),
    ];
  }, [status, mode, closed]);

  // Keyboard (FR-16): ↑/↓ move, Space or Enter stages / unstages, Delete discards.
  const onListKey = (event: React.KeyboardEvent) => {
    if (!ordered.length || turn) return;
    const at = ordered.findIndex((item) => item.file.path === selected?.path && item.area === selected?.area);
    const current = ordered[at];
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const next = ordered[Math.min(ordered.length - 1, Math.max(0, at + (event.key === "ArrowDown" ? 1 : -1)))];
      if (next) void store.select(next.file.path, next.area);
    } else if ((event.key === " " || event.key === "Enter") && current) {
      event.preventDefault();
      const target = { kind: "file" as const, path: current.file.path };
      void (current.area === "staged" ? store.unstage(target) : store.stage(target));
    } else if (event.key === "Delete" && current?.area === "unstaged") {
      event.preventDefault();
      askDiscard(current.file.path, 1, baseName(current.file.path));
    }
  };

  /** Discard a file or a folder's `count` files, after a question (FR-5, FR-18). */
  const askDiscard = (path: string, count: number, label: string) => {
    const file = status?.unstaged.find((f) => f.path === path);
    setConfirm({
      title: `Discard the changes in ${label}?`,
      body: !file
        ? `${count} ${count === 1 ? "file goes" : "files go"} back to the last staged or committed version; new files are deleted. Staged changes stay. You can undo it from the notice in this panel.`
        : file.status === "untracked"
          ? "The new file is deleted. You can undo it from the notice in this panel."
          : "Its unstaged changes go back to the last staged or committed version. You can undo it from the notice in this panel.",
      action: "Discard",
      destructive: true,
      run: () => void store.discard({ kind: "file", path }, count > 1 ? `${count} files in ${label}` : label),
    });
  };

  const openPath = (path: string) => {
    if (root) void openFile(joinPath(root, path));
  };
  const handlers = (area: Area): RowHandlers => ({
    root,
    busy: Boolean(busy),
    onSelect: (path) => void store.select(path, area),
    onOpen: openPath,
    onStage: (path) => void (area === "staged" ? store.unstage({ kind: "file", path }) : store.stage({ kind: "file", path })),
    onDiscard: area === "unstaged" ? askDiscard : undefined,
  });

  if (!cwd && loaded) {
    return (
      <Empty className="h-full">
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <SourceControlIcon />
          </EmptyMedia>
          <EmptyTitle>No folder</EmptyTitle>
          <EmptyDescription>Open a chat in a folder to see its changes.</EmptyDescription>
        </EmptyHeader>
      </Empty>
    );
  }
  if (loaded && !status && !error) {
    return (
      <Empty className="h-full">
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <SourceControlIcon />
          </EmptyMedia>
          <EmptyTitle>Not a Git repository</EmptyTitle>
          <EmptyDescription>This chat's folder is not in a Git repository, so there is nothing to review or commit.</EmptyDescription>
        </EmptyHeader>
      </Empty>
    );
  }

  const unstaged = status?.unstaged ?? [];
  const staged = status?.staged ?? [];
  const shownPath = turn ? turnFile : selected?.path ?? null;
  const shownFile = selected && !turn ? (selected.area === "staged" ? staged : unstaged).find((f) => f.path === selected.path) : undefined;

  return (
    <div
      className="flex h-full min-h-0 flex-col"
      onKeyDown={(event) => {
        if (event.key === "Escape" && expanded) {
          event.stopPropagation();
          setExpanded(false);
        }
      }}
    >
      <header className="flex h-10 shrink-0 items-center gap-2 border-b px-3 text-[13px]">
        <GitBranchIcon className="size-4 shrink-0 text-muted-foreground" />
        <RepositorySwitch />
        {status && (status.ahead > 0 || status.behind > 0) && (
          <span className="flex shrink-0 items-center gap-1 font-mono text-xs text-muted-foreground tabular-nums">
            {status.ahead > 0 && (
              <span className="flex items-center" title={`${status.ahead} commits to push`}>
                <ArrowUpIcon className="size-3" />
                {status.ahead}
              </span>
            )}
            {status.behind > 0 && (
              <span className="flex items-center" title={`${status.behind} commits to pull`}>
                <ArrowDownIcon className="size-3" />
                {status.behind}
              </span>
            )}
          </span>
        )}
        {status && status.state !== "normal" && (
          <span className="shrink-0 rounded-md bg-muted px-1.5 py-0.5 text-[11px] text-muted-foreground">{status.state} in progress</span>
        )}
        <span className="ml-auto flex shrink-0 items-center gap-0.5">
          <IconAction
            label={mode === "tree" ? "Show the files as a list" : "Show the files as a tree"}
            onClick={() => useUiStore.getState().setScmListMode(mode === "tree" ? "list" : "tree")}
          >
            {mode === "tree" ? <ListIcon className="size-3.5" /> : <ListTreeIcon className="size-3.5" />}
          </IconAction>
          <IconAction
            label={layout === "split" ? "Show the diff inline" : "Show the diff side by side (in a wide panel)"}
            onClick={() => useUiStore.getState().setScmDiffLayout(layout === "split" ? "inline" : "split")}
          >
            {layout === "split" ? <RowsIcon className="size-3.5" /> : <Columns2Icon className="size-3.5" />}
          </IconAction>
          <IconAction label="Refresh" onClick={() => void store.refresh(cwd)}>
            <RefreshCwIcon className={cn("size-3.5", !loaded && "animate-spin")} />
          </IconAction>
        </span>
      </header>

      {error && (
        <div role="alert" className="flex shrink-0 items-start gap-2 border-b bg-destructive/5 px-3 py-2 text-xs text-destructive" data-selectable>
          <span className="min-w-0 flex-1 whitespace-pre-wrap break-words">{error}</span>
          <button type="button" aria-label="Dismiss" onClick={store.clearError} className="shrink-0">
            <XIcon className="size-3.5" />
          </button>
        </div>
      )}
      {undo && (
        <div className="flex shrink-0 items-center gap-2 border-b px-3 py-1.5 text-xs text-muted-foreground">
          <span className="min-w-0 flex-1 truncate">Discarded {undo.label}</span>
          <Button variant="outline" size="sm" className="h-6 px-2 text-xs" aria-label={`Undo: put back ${undo.label}`} disabled={Boolean(busy)} onClick={() => void store.undoDiscard()}>
            Undo
          </Button>
        </div>
      )}

      {/* The commit box stays put (hidden while the diff is expanded or a turn shows). */}
      {!turn && !expanded && (
        <div className="shrink-0 border-b">
          <CommitBox onConfirm={setConfirm} />
        </div>
      )}

      {/* Files over the diff, the line between them dragged to share the height (kept). */}
      <ResizablePanelGroup
        orientation="vertical"
        className="min-h-0 flex-1"
        defaultLayout={expanded ? undefined : { files: split, diff: 100 - split }}
        onLayoutChanged={(next) => {
          if (!expanded && typeof next.files === "number") useUiStore.getState().setScmSplit(Math.round(next.files));
        }}
      >
        {!expanded && (
          <>
            <ResizablePanel id="files" minSize="64px" defaultSize={`${split}`}>
              <div
                ref={listRef}
                className="h-full overflow-y-auto overscroll-contain py-1 outline-none"
                tabIndex={0}
                onKeyDown={onListKey}
                aria-label="Changes"
              >
                {turn ? (
                  <TurnFiles />
                ) : (
                  <>
                    {status && status.conflicted.length > 0 && (
                      <Section title="Conflicts" count={status.conflicted.length}>
                        {status.conflicted.map((path) => (
                          <li
                            key={path}
                            className="flex h-7 cursor-default items-center gap-2 rounded-md px-2 text-[13px] hover:bg-accent/60"
                            title={`${path}\nClick to open and resolve`}
                            onClick={() => openPath(path)}
                          >
                            <span className="min-w-0 flex-1 truncate">{baseName(path)}</span>
                            <span className="w-3 text-center font-mono text-[11px] text-status-error">!</span>
                          </li>
                        ))}
                      </Section>
                    )}
                    {staged.length > 0 && (
                      <Section
                        title="Staged"
                        count={staged.length}
                        actions={
                          <IconAction label="Unstage all" onClick={() => void store.unstage({ kind: "all" })} disabled={Boolean(busy)}>
                            <MinusIcon className="size-3.5" />
                          </IconAction>
                        }
                      >
                        <ChangeRows
                          files={staged}
                          area="staged"
                          mode={mode}
                          selected={selected?.area === "staged" ? selected.path : null}
                          closed={closed.staged}
                          onToggle={(path) => store.toggleFolder("staged", path)}
                          handlers={handlers("staged")}
                        />
                      </Section>
                    )}
                    <Section
                      title="Changes"
                      count={unstaged.length}
                      actions={
                        unstaged.length > 0 && (
                          <>
                            <IconAction
                              label="Discard all changes"
                              disabled={Boolean(busy)}
                              onClick={() =>
                                setConfirm({
                                  title: "Discard all changes?",
                                  body: `${unstaged.length} ${unstaged.length === 1 ? "file goes" : "files go"} back to the last staged or committed version, and new files are deleted. Staged changes stay. You can undo it from the notice in this panel.`,
                                  action: "Discard all",
                                  destructive: true,
                                  run: () => void store.discard({ kind: "all" }, `${unstaged.length} ${unstaged.length === 1 ? "file" : "files"}`),
                                })
                              }
                            >
                              <Undo2Icon className="size-3.5" />
                            </IconAction>
                            <IconAction label="Stage all" onClick={() => void store.stage({ kind: "all" })} disabled={Boolean(busy)}>
                              <PlusIcon className="size-3.5" />
                            </IconAction>
                          </>
                        )
                      }
                    >
                      {unstaged.length === 0 ? (
                        <li className="px-2 py-1 text-xs text-muted-foreground">
                          {!loaded ? "Reading the repository…" : staged.length ? "Everything is staged." : "No changes: the folder matches the last commit."}
                        </li>
                      ) : (
                        <ChangeRows
                          files={unstaged}
                          area="unstaged"
                          mode={mode}
                          selected={selected?.area === "unstaged" ? selected.path : null}
                          closed={closed.unstaged}
                          onToggle={(path) => store.toggleFolder("unstaged", path)}
                          handlers={handlers("unstaged")}
                        />
                      )}
                    </Section>
                  </>
                )}
              </div>
            </ResizablePanel>
            <ResizableHandle
              withHandle
              className="h-1.5 bg-border/60 hover:bg-ring/40 data-[separator=active]:bg-ring/60"
              aria-label="Drag to share the height between the files and the diff"
            />
          </>
        )}
        <ResizablePanel id="diff" minSize="120px" defaultSize={expanded ? "100" : `${100 - split}`}>
          <div className="flex h-full min-h-0 flex-col">
            {shownPath && status && (
              <div className="flex h-9 shrink-0 items-center gap-2 border-b px-3 text-xs">
                <FileIcon className="size-3.5 shrink-0 text-muted-foreground" />
                <span className="flex min-w-0 flex-1 items-baseline gap-1.5" title={shownPath}>
                  <span className="truncate text-[13px] font-medium">{baseName(shownPath)}</span>
                  <span className="min-w-0 truncate text-muted-foreground">{folderOf(shownPath)}</span>
                </span>
                {turn ? (
                  <span className="shrink-0 rounded-md bg-muted px-1.5 py-0.5 text-[11px] text-muted-foreground">This turn</span>
                ) : selected ? (
                  <span className="shrink-0 rounded-md bg-muted px-1.5 py-0.5 text-[11px] text-muted-foreground">
                    {selected.area === "staged" ? "Staged" : "Not staged"}
                  </span>
                ) : null}
                <span className="flex shrink-0 items-center gap-0.5">
                  {shownFile && selected?.area === "unstaged" && (
                    <IconAction label="Discard this file's changes" disabled={Boolean(busy)} onClick={() => askDiscard(shownFile.path, 1, baseName(shownFile.path))}>
                      <Undo2Icon className="size-3.5" />
                    </IconAction>
                  )}
                  {shownFile && (
                    <IconAction
                      label={selected?.area === "staged" ? "Unstage this file" : "Stage this file"}
                      disabled={Boolean(busy)}
                      onClick={() => handlers(selected!.area).onStage(shownFile.path)}
                    >
                      {selected?.area === "staged" ? <MinusIcon className="size-3.5" /> : <PlusIcon className="size-3.5" />}
                    </IconAction>
                  )}
                  <IconAction label="Open the file" onClick={() => openPath(shownPath)}>
                    <ExternalLinkIcon className="size-3.5" />
                  </IconAction>
                  <IconAction label={expanded ? "Show the files again (Esc)" : "Give the diff the whole panel"} onClick={() => setExpanded(!expanded)}>
                    {expanded ? <Minimize2Icon className="size-3.5" /> : <Maximize2Icon className="size-3.5" />}
                  </IconAction>
                </span>
              </div>
            )}
            <div className="flex min-h-0 flex-1 flex-col">
              <DiffPane />
            </div>
          </div>
        </ResizablePanel>
      </ResizablePanelGroup>

      <ConfirmDialog confirm={confirm} onClose={() => setConfirm(null)} />
    </div>
  );
}
