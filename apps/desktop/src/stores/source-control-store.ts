// The Source control tool's state: the repository of the active chat's folder
// (its lists and the selected file's diff), a turn's diff when one is shown,
// the commit message, the last discard's Undo, and the git actions
// (docs/features/source-control/spec.md, FR-1 – FR-7, FR-10, FR-13; plan 3.5).
//
// Git runs in the shell (`lib/scm.ts`); the draft message comes from the
// chat's engine through `engine-store`. The panel refreshes the status while
// it is visible (`refresh`), and nothing re-renders when it did not change.

import { create } from "zustand";
import { joinForDraft, keptSelection, patchText, scm, statusSignature, type Area, type FileDiff, type Repository, type ScmStatus, type Target } from "@/lib/scm";
import { useEngineStore } from "@/stores/engine-store";
import { useUiStore } from "@/stores/ui-store";

/** A turn's changes shown instead of the folder's (FR-10). */
export interface TurnView {
  chat: string;
  /** The checkpoint's id, start and end commits. */
  checkpoint: string;
  start: string;
  end: string;
  /** The turn's prompt, for the header. */
  prompt: string;
  files: { path: string; status: string; added?: number | null; removed?: number | null }[];
}

/** A discard that can be undone once. */
interface Undo {
  cwd: string;
  token: string;
  label: string;
}

interface SourceControlState {
  /** The chat's folder the repositories were found for. */
  folder: string | null;
  /** Its repositories: the one it is in, then nested ones (FR-17). */
  repos: Repository[];
  /** The repository picked per folder (its root). */
  picked: Record<string, string>;
  /** The repository shown (its root, or the folder when it has none); `null` before the first refresh. */
  cwd: string | null;
  status: ScmStatus | null;
  signature: string;
  loaded: boolean;
  error: string | null;
  selected: { path: string; area: Area } | null;
  diff: FileDiff | null;
  diffError: string | null;
  turn: TurnView | null;
  /** The turn file shown (turn mode). */
  turnFile: string | null;
  /** The commit message being written, per folder. */
  messages: Record<string, string>;
  amend: boolean;
  /** An action running ("stage", "commit", "draft", …): its buttons wait. */
  busy: string | null;
  undo: Undo | null;
  /** Closed folders of the tree, per repository and list (`<root>|<area>`), for the app run (FR-18). */
  closed: Record<string, string[]>;
  toggleFolder(area: Area, path: string): void;
  /** Ask the panel to show a file (from the summary card or Files, FR-15). */
  reveal: { path: string; nonce: number } | null;

  /** Read the repositories of the chat's `folder` (now and then) and the picked one's status. */
  refresh(folder: string | null): Promise<void>;
  /** Show another of the folder's repositories. */
  pick(root: string): Promise<void>;
  select(path: string, area: Area): Promise<void>;
  reloadDiff(): Promise<void>;
  stage(target: Target): Promise<void>;
  unstage(target: Target): Promise<void>;
  discard(target: Target, label: string): Promise<void>;
  undoDiscard(): Promise<void>;
  setMessage(text: string): void;
  setAmend(on: boolean): void;
  /** Commit; resolves with what needs the person's confirmation, if anything. */
  commit(confirmRunning: boolean): Promise<{ running: string[]; pushed: boolean } | null>;
  draftMessage(): Promise<void>;
  showTurn(turn: TurnView, path?: string | null): Promise<void>;
  selectTurnFile(path: string): Promise<void>;
  leaveTurn(): void;
  revealPath(path: string): void;
  clearError(): void;
}

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

/**
 * Open Source control at `path` (relative to the repository's top folder):
 * the summary card's Git section and the Files menu use it (FR-15).
 */
export function showInSourceControl(path: string) {
  useSourceControlStore.getState().revealPath(path);
  useUiStore.getState().openPanel("source-control");
}

/** Commit messages being written, per repository: kept across reloads and restarts. */
const MESSAGES_KEY = "evo.scm.messages";
function savedMessages(): Record<string, string> {
  try {
    const raw = JSON.parse(localStorage.getItem(MESSAGES_KEY) ?? "{}") as unknown;
    return raw && typeof raw === "object" ? (raw as Record<string, string>) : {};
  } catch {
    return {};
  }
}

/** Refreshes overtaken by a newer one are dropped. */
let generation = 0;
/** The repository list is read again at most this often (the status more often). */
const REPOS_EVERY_MS = 5000;
let reposReadAt = 0;

const sameRoot = (a: string, b: string) => a.replace(/\\/g, "/").toLowerCase() === b.replace(/\\/g, "/").toLowerCase();

export const useSourceControlStore = create<SourceControlState>((set, get) => {
  /** Run a git action, then refresh; an error shows in the panel. */
  const act = async (name: string, run: (cwd: string) => Promise<void>) => {
    const cwd = get().cwd;
    if (!cwd || get().busy) return;
    set({ busy: name, error: null });
    try {
      await run(cwd);
    } catch (error) {
      set({ error: message(error) });
    } finally {
      set({ busy: null });
      await get().refresh(cwd);
      await get().reloadDiff();
    }
  };

  return {
    folder: null,
    repos: [],
    picked: {},
    cwd: null,
    status: null,
    signature: "",
    loaded: false,
    error: null,
    selected: null,
    diff: null,
    diffError: null,
    turn: null,
    turnFile: null,
    messages: savedMessages(),
    amend: false,
    busy: null,
    undo: null,
    reveal: null,
    closed: {},

    toggleFolder(area, path) {
      const key = `${get().cwd}|${area}`;
      set((s) => {
        const now = s.closed[key] ?? [];
        return { closed: { ...s.closed, [key]: now.includes(path) ? now.filter((p) => p !== path) : [...now, path] } };
      });
    },

    async refresh(folder) {
      const ticket = ++generation;
      if (!folder) {
        set({ folder: null, repos: [], cwd: null, status: null, signature: "", loaded: true, selected: null, diff: null });
        return;
      }
      // The folder's repositories: on a new folder, and every few seconds.
      let repos = get().repos;
      if (folder !== get().folder || Date.now() - reposReadAt > REPOS_EVERY_MS) {
        repos = await scm.repositories(folder).catch((): Repository[] => []);
        if (ticket !== generation) return;
        reposReadAt = Date.now();
        set({ folder, repos });
      }
      const wanted = get().picked[folder];
      const cwd = (wanted && repos.find((repo) => sameRoot(repo.root, wanted))?.root) || repos[0]?.root || folder;
      const moved = cwd !== get().cwd;
      if (moved) set({ cwd, status: null, signature: "", loaded: false, selected: null, diff: null, turn: null, turnFile: null, undo: null, error: null });
      try {
        const status = await scm.status(cwd);
        if (ticket !== generation) return;
        const signature = statusSignature(status);
        // A selection that is not in this repository's lists goes (another workspace or repository).
        const selected = keptSelection(get().selected, status);
        if (signature === get().signature && get().loaded) {
          if (JSON.stringify(selected) !== JSON.stringify(get().selected)) set({ selected, ...(selected ? {} : { diff: null }) });
          return;
        }
        const changedSelection = JSON.stringify(selected) !== JSON.stringify(get().selected);
        set({ status, signature, loaded: true, selected, ...(selected ? {} : { diff: null }) });
        if (selected && (changedSelection || !moved)) await get().reloadDiff();
      } catch (error) {
        if (ticket !== generation) return;
        set({ status: null, signature: "", loaded: true, error: message(error) });
      }
    },

    async pick(root) {
      const folder = get().folder;
      if (!folder) return;
      set((s) => ({ picked: { ...s.picked, [folder]: root } }));
      await get().refresh(folder);
    },

    async select(path, area) {
      // Only a file this repository lists (a request may come from another folder's view).
      const list = area === "staged" ? get().status?.staged : get().status?.unstaged;
      if (!list?.some((file) => file.path === path)) return;
      set({ selected: { path, area }, turn: null, turnFile: null, diffError: null });
      await get().reloadDiff();
    },

    async reloadDiff() {
      const { cwd, selected, turn, turnFile } = get();
      if (!cwd) return;
      try {
        if (turn && turnFile) {
          const diff = await scm.diffBetween(cwd, turn.start, turn.end, turnFile);
          if (get().turnFile === turnFile) set({ diff, diffError: null });
          return;
        }
        if (!selected) return set({ diff: null });
        const diff = await scm.diff(cwd, selected.path, selected.area);
        const now = get().selected;
        // Only the selection that asked; and nothing to redraw when the diff is the same.
        if (now?.path !== selected.path || now.area !== selected.area) return;
        if (get().diff?.hash === diff.hash && get().diff?.path === diff.path) return;
        set({ diff, diffError: null });
      } catch (error) {
        set({ diff: null, diffError: message(error) });
      }
    },

    stage: (target) => act("stage", (cwd) => scm.stage(cwd, target)),
    unstage: (target) => act("unstage", (cwd) => scm.unstage(cwd, target)),
    discard: (target, label) =>
      act("discard", async (cwd) => {
        const { undo } = await scm.discard(cwd, target);
        set({ undo: { cwd, token: undo, label } });
      }),
    undoDiscard: () =>
      act("undo", async (cwd) => {
        const undo = get().undo;
        if (!undo || undo.cwd !== cwd) return;
        await scm.undoDiscard(cwd, undo.token);
        set({ undo: null });
      }),

    setMessage(text) {
      const cwd = get().cwd;
      if (cwd) set((s) => ({ messages: { ...s.messages, [cwd]: text } }));
    },
    setAmend: (amend) => set({ amend }),

    async commit(confirmRunning) {
      const { cwd, amend } = get();
      const text = cwd ? (get().messages[cwd] ?? "").trim() : "";
      if (!cwd || !text || get().busy) return null;
      set({ busy: "commit", error: null });
      try {
        const result = await scm.commit(cwd, { message: text, amend, confirmRunning });
        if ("needsConfirm" in result) return result.needsConfirm;
        set((s) => ({ messages: { ...s.messages, [cwd]: "" }, amend: false, undo: null }));
        return null;
      } catch (error) {
        set({ error: message(error) });
        return null;
      } finally {
        set({ busy: null });
        await get().refresh(cwd);
        await get().reloadDiff();
      }
    },

    async draftMessage() {
      const { cwd, status } = get();
      if (!cwd || !status || status.staged.length === 0 || get().busy) return;
      set({ busy: "draft", error: null });
      try {
        const [diffs, subjects] = await Promise.all([
          Promise.all(status.staged.map((file) => scm.diff(cwd, file.path, "staged"))),
          scm.recentSubjects(cwd, 20).catch((): string[] => []),
        ]);
        const diff = joinForDraft(diffs.map((d) => ({ path: d.path, text: patchText(d) })));
        const drafted = await useEngineStore.getState().draftCommitMessage(diff, subjects);
        if (drafted.trim()) set((s) => ({ messages: { ...s.messages, [cwd]: drafted.trim() } }));
      } catch (error) {
        set({ error: `Could not draft a message: ${message(error)}` });
      } finally {
        set({ busy: null });
      }
    },

    async showTurn(turn, path) {
      const first = path ?? turn.files[0]?.path ?? null;
      set({ turn, turnFile: first, selected: null, diff: null, diffError: null });
      if (first) await get().reloadDiff();
    },
    async selectTurnFile(path) {
      set({ turnFile: path, diff: null, diffError: null });
      await get().reloadDiff();
    },
    leaveTurn: () => set({ turn: null, turnFile: null, diff: null }),

    revealPath(path) {
      set({ reveal: { path, nonce: Date.now() } });
    },
    clearError: () => set({ error: null }),
  };
});

// A message being written survives a window reload or a restart.
useSourceControlStore.subscribe((state, before) => {
  if (state.messages === before.messages) return;
  try {
    const kept = Object.fromEntries(Object.entries(state.messages).filter(([, text]) => text.trim()));
    localStorage.setItem(MESSAGES_KEY, JSON.stringify(kept));
  } catch {
    // Storage unavailable: the message lives for this window only.
  }
});
