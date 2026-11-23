// What each workspace looks like on the rail: the icon the person chose
// (saved as the app setting `sidebar.workspaceIcons`) and the project logo
// found in its folder, read with the shell's file commands and kept as a blob
// URL for this run (docs/features/workspace-rail/spec.md, FR-13, FR-14, A-7).

import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";
import { db, log } from "@/lib/db";
import { inTauri } from "@/lib/platform";
import { LOGO_MAX_BYTES, logoCandidates, logoMime, NESTED_ROOTS } from "@/lib/project-logo";

export type ChosenIcon =
  | { kind: "icon"; name: string }
  | { kind: "emoji"; text: string }
  | { kind: "logo" }
  | { kind: "initials" };

const ICONS_KEY = "sidebar.workspaceIcons";
/** Nested projects looked at per monorepo folder. */
const NESTED_MAX = 8;

interface DirEntry {
  path: string;
  name: string;
  dir: boolean;
  size: number;
}

interface WorkspaceIconStore {
  loaded: boolean;
  /** Chosen icons by workspace id; no entry: automatic (logo, else initials). */
  chosen: Record<string, ChosenIcon>;
  /** Found logos by workspace id: a blob URL, `null` when there is none. */
  logos: Record<string, string | null>;
  load(): Promise<void>;
  /** Save a workspace's icon; `null` goes back to automatic. */
  choose(workspaceId: string, icon: ChosenIcon | null): void;
  /** Look for the project's logo once per run. */
  findLogo(workspaceId: string, folder: string): void;
}

const isChosen = (value: unknown): value is ChosenIcon => {
  if (!value || typeof value !== "object") return false;
  const icon = value as Record<string, unknown>;
  return (
    (icon.kind === "icon" && typeof icon.name === "string") ||
    (icon.kind === "emoji" && typeof icon.text === "string") ||
    icon.kind === "logo" ||
    icon.kind === "initials"
  );
};

const searching = new Set<string>();

/** The path of the first logo of `folder` that exists, or `null`. */
async function locateLogo(folder: string): Promise<string | null> {
  const listings = new Map<string, Promise<DirEntry[]>>();
  const list = (dir: string) => {
    let listing = listings.get(dir);
    if (!listing) {
      listing = invoke<DirEntry[]>("fs_list_dir", { path: dir }).catch(() => []);
      listings.set(dir, listing);
    }
    return listing;
  };
  const parentOf = (path: string) => path.replace(/[\\/][^\\/]*$/, "");
  const nameOf = (path: string) => path.slice(parentOf(path).length + 1).toLowerCase();
  const nestedDirs = async (root: string) =>
    (await list(`${folder}${folder.includes("\\") ? "\\" : "/"}${root}`))
      .filter((entry) => entry.dir)
      .slice(0, NESTED_MAX)
      .map((entry) => entry.name);
  const own = logoCandidates(folder, []);
  const candidates = [...own];
  for (const root of NESTED_ROOTS) {
    const nested = await nestedDirs(root);
    // The nested projects' places come after the folder's own.
    if (nested.length) candidates.push(...logoCandidates(folder, nested, root).slice(own.length));
  }
  for (const path of candidates) {
    if (!logoMime(path)) continue;
    const entry = (await list(parentOf(path))).find((item) => !item.dir && item.name.toLowerCase() === nameOf(path));
    if (entry && entry.size > 0 && entry.size <= LOGO_MAX_BYTES) return entry.path;
  }
  return null;
}

export const useWorkspaceIconStore = create<WorkspaceIconStore>()((set, get) => ({
  loaded: false,
  chosen: {},
  logos: {},

  async load() {
    if (get().loaded) return;
    const settings = await db.settings().catch(() => ({}) as Record<string, unknown>);
    const saved = settings[ICONS_KEY];
    const chosen: Record<string, ChosenIcon> = {};
    if (saved && typeof saved === "object") {
      for (const [id, icon] of Object.entries(saved as Record<string, unknown>)) if (isChosen(icon)) chosen[id] = icon;
    }
    set({ loaded: true, chosen });
  },

  choose(workspaceId, icon) {
    const chosen = { ...get().chosen };
    if (icon) chosen[workspaceId] = icon;
    else delete chosen[workspaceId];
    set({ chosen });
    void db.setSetting(ICONS_KEY, chosen).catch((error) => log("error", "db", `saving the workspace icons failed: ${String(error)}`));
  },

  findLogo(workspaceId, folder) {
    if (!inTauri || workspaceId in get().logos || searching.has(workspaceId)) return;
    searching.add(workspaceId);
    void locateLogo(folder)
      .then(async (path) => {
        const mime = path && logoMime(path);
        if (!path || !mime) return null;
        const bytes = await invoke<ArrayBuffer>("fs_read_bytes", { path });
        return URL.createObjectURL(new Blob([bytes], { type: mime }));
      })
      .catch(() => null)
      .then((url) => set((s) => ({ logos: { ...s.logos, [workspaceId]: url } })))
      .finally(() => searching.delete(workspaceId));
  },
}));
