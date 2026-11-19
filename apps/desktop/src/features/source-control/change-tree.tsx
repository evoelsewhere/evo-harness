// The Staged and Changes lists: a tree of folders (folder rows with their
// file count, lines and actions) or a flat list (docs/features/source-control/spec.md,
// FR-1, FR-4, FR-5, FR-18; plan 3.10).

import * as React from "react";
import { ChevronRightIcon, FileIcon, FolderIcon, MinusIcon, PlusIcon, Undo2Icon } from "lucide-react";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import { ContextMenu, ContextMenuContent, ContextMenuTrigger } from "@/components/ui/context-menu";
import { DropdownMenuItem, DropdownMenuSeparator } from "@/components/ui/dropdown-menu";
import { composerDraft } from "@/features/chat/chat-composer";
import { isMac, isWindows } from "@/lib/platform";
import { changeTree, statusMark, visibleRows, type Area, type ScmFile, type TreeFolder } from "@/lib/scm";
import { cn } from "@/lib/utils";

const MANAGER = isMac ? "Finder" : isWindows ? "Explorer" : "the file manager";

const TONE = {
  added: "text-status-ok",
  modified: "text-status-warn",
  deleted: "text-status-error",
} as const;

const folderOf = (path: string) => path.split("/").slice(0, -1).join("/");
/** Left padding of a row at `depth` (px): the chevron column, then 12 px per level. */
const indent = (depth: number) => ({ paddingLeft: 8 + depth * 12 });

export function Counts({ added, removed }: { added?: number | null; removed?: number | null }) {
  if (!added && !removed) return null;
  return (
    <span className="shrink-0 font-mono text-[11px] tabular-nums">
      {added ? <span className="text-status-ok">+{added}</span> : null}
      {added && removed ? " " : null}
      {removed ? <span className="text-status-error">−{removed}</span> : null}
    </span>
  );
}

function RowAction({ label, onClick, disabled, children }: { label: string; onClick(): void; disabled?: boolean; children: React.ReactNode }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={(event) => {
        event.stopPropagation();
        onClick();
      }}
      className="grid size-5 place-items-center rounded text-muted-foreground hover:bg-background hover:text-foreground disabled:opacity-50"
    >
      {children}
    </button>
  );
}

export interface RowHandlers {
  /** The repository's top folder: file paths are relative to it. */
  root: string;
  busy: boolean;
  onSelect(path: string): void;
  /** Open the file in Files (double-click). */
  onOpen(path: string): void;
  /** Stage (Changes) or unstage (Staged) a file or a folder. */
  onStage(path: string): void;
  /** Changes only: discard a file, or a folder's `count` files. */
  onDiscard?(path: string, count: number, label: string): void;
}

const absolute = (root: string, path: string) => `${root.replace(/[\\/]+$/, "")}/${path}`;
const copy = (text: string) => void navigator.clipboard.writeText(text).catch(() => undefined);
const quietly = (run: () => Promise<unknown>) => void run().catch(() => undefined);

/** Right-click on a file row (audit: the WebView's own menu showed instead). */
function FileMenu({ file, area, handlers }: { file: ScmFile; area: Area; handlers: RowHandlers }) {
  const full = absolute(handlers.root, file.path);
  const name = file.path.split("/").pop() ?? file.path;
  const gone = file.status === "deleted";
  return (
    <ContextMenuContent className="w-56">
      <DropdownMenuItem onClick={() => handlers.onSelect(file.path)}>Show changes</DropdownMenuItem>
      <DropdownMenuItem disabled={gone} onClick={() => handlers.onOpen(file.path)}>
        Open file
      </DropdownMenuItem>
      <DropdownMenuSeparator />
      <DropdownMenuItem disabled={handlers.busy} onClick={() => handlers.onStage(file.path)}>
        {area === "staged" ? "Unstage" : "Stage"}
      </DropdownMenuItem>
      {handlers.onDiscard && (
        <DropdownMenuItem variant="destructive" disabled={handlers.busy} onClick={() => handlers.onDiscard?.(file.path, 1, name)}>
          Discard changes…
        </DropdownMenuItem>
      )}
      <DropdownMenuSeparator />
      <DropdownMenuItem disabled={gone} onClick={() => composerDraft.attach(full)}>
        Attach to message
      </DropdownMenuItem>
      <DropdownMenuItem onClick={() => copy(full)}>Copy path</DropdownMenuItem>
      <DropdownMenuItem onClick={() => copy(file.path)}>Copy relative path</DropdownMenuItem>
      <DropdownMenuSeparator />
      <DropdownMenuItem disabled={gone} onClick={() => quietly(() => openPath(full))}>
        Open with default app
      </DropdownMenuItem>
      <DropdownMenuItem disabled={gone} onClick={() => quietly(() => revealItemInDir(full))}>
        Show in {MANAGER}
      </DropdownMenuItem>
    </ContextMenuContent>
  );
}

/** Right-click on a folder row. */
function FolderMenu({
  folder,
  area,
  open,
  onToggle,
  handlers,
}: {
  folder: TreeFolder;
  area: Area;
  open: boolean;
  onToggle(): void;
  handlers: RowHandlers;
}) {
  const full = absolute(handlers.root, folder.path);
  const files = `${folder.count} ${folder.count === 1 ? "file" : "files"}`;
  return (
    <ContextMenuContent className="w-56">
      <DropdownMenuItem onClick={onToggle}>{open ? "Collapse" : "Expand"}</DropdownMenuItem>
      <DropdownMenuSeparator />
      <DropdownMenuItem disabled={handlers.busy} onClick={() => handlers.onStage(folder.path)}>
        {area === "staged" ? `Unstage ${files}` : `Stage ${files}`}
      </DropdownMenuItem>
      {handlers.onDiscard && (
        <DropdownMenuItem
          variant="destructive"
          disabled={handlers.busy}
          onClick={() => handlers.onDiscard?.(folder.path, folder.count, folder.name)}
        >
          Discard {files}…
        </DropdownMenuItem>
      )}
      <DropdownMenuSeparator />
      <DropdownMenuItem onClick={() => copy(full)}>Copy path</DropdownMenuItem>
      <DropdownMenuItem onClick={() => copy(folder.path)}>Copy relative path</DropdownMenuItem>
      <DropdownMenuItem onClick={() => quietly(() => revealItemInDir(full))}>Show in {MANAGER}</DropdownMenuItem>
    </ContextMenuContent>
  );
}

function FileRow({
  file,
  area,
  depth,
  inTree,
  selected,
  handlers,
}: {
  file: ScmFile;
  area: Area;
  depth: number;
  inTree: boolean;
  selected: boolean;
  handlers: RowHandlers;
}) {
  const mark = statusMark(file.status);
  const name = file.path.split("/").pop() ?? file.path;
  const folder = folderOf(file.path);
  return (
    <ContextMenu>
      <ContextMenuTrigger
        render={<li />}
        role="option"
        aria-selected={selected}
        data-path={file.path}
        className={cn(
          "group/row flex h-7 cursor-default items-center gap-1.5 rounded-md pr-2 text-[13px]",
          selected ? "bg-accent" : "hover:bg-accent/60",
        )}
        style={indent(inTree ? depth : 0)}
        onClick={() => handlers.onSelect(file.path)}
        onDoubleClick={() => handlers.onOpen(file.path)}
        title={`${file.from ? `${file.from} → ` : ""}${file.path}\nDouble-click to open`}
      >
        {inTree && (
          <>
            {/* The chevron's place, so files line up with folders of their level. */}
            <span className="size-3.5 shrink-0" />
            <FileIcon className="size-3.5 shrink-0 text-muted-foreground" />
          </>
        )}
        <span className={cn("min-w-0 flex-1 truncate", file.status === "deleted" && "line-through decoration-muted-foreground/60")}>
          {name}
          {!inTree && folder && <span className="ml-1.5 text-xs text-muted-foreground">{folder}</span>}
        </span>
        <Counts added={file.added} removed={file.removed} />
        <span className="hidden shrink-0 items-center gap-0.5 group-hover/row:flex group-aria-selected/row:flex">
          {handlers.onDiscard && (
            <RowAction label={`Discard changes in ${name}`} disabled={handlers.busy} onClick={() => handlers.onDiscard?.(file.path, 1, name)}>
              <Undo2Icon className="size-3.5" />
            </RowAction>
          )}
          <RowAction label={area === "staged" ? `Unstage ${name}` : `Stage ${name}`} disabled={handlers.busy} onClick={() => handlers.onStage(file.path)}>
            {area === "staged" ? <MinusIcon className="size-3.5" /> : <PlusIcon className="size-3.5" />}
          </RowAction>
        </span>
        <span className={cn("w-3 shrink-0 text-center font-mono text-[11px]", TONE[mark.tone])}>{mark.letter}</span>
      </ContextMenuTrigger>
      <FileMenu file={file} area={area} handlers={handlers} />
    </ContextMenu>
  );
}

function FolderRow({
  folder,
  area,
  open,
  onToggle,
  handlers,
}: {
  folder: TreeFolder;
  area: Area;
  open: boolean;
  onToggle(): void;
  handlers: RowHandlers;
}) {
  return (
    <ContextMenu>
      <ContextMenuTrigger
        render={<li />}
        role="treeitem"
        aria-expanded={open}
        data-folder={folder.path}
        className="group/row flex h-7 cursor-default items-center gap-1.5 rounded-md pr-2 text-[13px] hover:bg-accent/60"
        style={indent(folder.depth)}
        onClick={onToggle}
        title={folder.path}
      >
        <ChevronRightIcon className={cn("size-3.5 shrink-0 text-muted-foreground transition-transform", open && "rotate-90")} />
        <FolderIcon className="size-3.5 shrink-0 text-muted-foreground" />
        <span className="min-w-0 flex-1 truncate">{folder.name}</span>
        <span className="shrink-0 text-[11px] text-muted-foreground tabular-nums" title={`${folder.count} changed ${folder.count === 1 ? "file" : "files"}`}>
          {folder.count}
        </span>
        <Counts added={folder.added} removed={folder.removed} />
        <span className="hidden shrink-0 items-center gap-0.5 group-hover/row:flex">
          {handlers.onDiscard && (
            <RowAction
              label={`Discard the changes in ${folder.name}`}
              disabled={handlers.busy}
              onClick={() => handlers.onDiscard?.(folder.path, folder.count, folder.name)}
            >
              <Undo2Icon className="size-3.5" />
            </RowAction>
          )}
          <RowAction
            label={area === "staged" ? `Unstage ${folder.name}` : `Stage ${folder.name}`}
            disabled={handlers.busy}
            onClick={() => handlers.onStage(folder.path)}
          >
            {area === "staged" ? <MinusIcon className="size-3.5" /> : <PlusIcon className="size-3.5" />}
          </RowAction>
        </span>
        {/* Lines up with the files' status letters. */}
        <span className="w-3 shrink-0" />
      </ContextMenuTrigger>
      <FolderMenu folder={folder} area={area} open={open} onToggle={onToggle} handlers={handlers} />
    </ContextMenu>
  );
}

/** The files of one list, as a tree or flat; the visible file paths in order come back through `onRows`. */
export function ChangeRows({
  files,
  area,
  mode,
  selected,
  closed,
  onToggle,
  handlers,
}: {
  files: ScmFile[];
  area: Area;
  mode: "tree" | "list";
  /** The selected path in this list, if any. */
  selected: string | null;
  closed: ReadonlySet<string>;
  onToggle(path: string): void;
  handlers: RowHandlers;
}) {
  const rows = React.useMemo(() => (mode === "tree" ? visibleRows(changeTree(files), closed) : null), [files, mode, closed]);
  if (!rows) {
    return (
      <>
        {files.map((file) => (
          <FileRow key={file.path} file={file} area={area} depth={0} inTree={false} selected={selected === file.path} handlers={handlers} />
        ))}
      </>
    );
  }
  return (
    <>
      {rows.map((node) =>
        node.kind === "folder" ? (
          <FolderRow
            key={`d:${node.path}`}
            folder={node}
            area={area}
            open={!closed.has(node.path)}
            onToggle={() => onToggle(node.path)}
            handlers={handlers}
          />
        ) : (
          <FileRow
            key={node.path}
            file={node.file}
            area={area}
            depth={node.depth}
            inTree
            selected={selected === node.path}
            handlers={handlers}
          />
        ),
      )}
    </>
  );
}

/** The file paths a list shows, in order (for the keyboard). */
export function visibleFiles(files: ScmFile[], mode: "tree" | "list", closed: ReadonlySet<string>): string[] {
  if (mode === "list") return files.map((file) => file.path);
  return visibleRows(changeTree(files), closed).flatMap((node) => (node.kind === "file" ? [node.path] : []));
}
