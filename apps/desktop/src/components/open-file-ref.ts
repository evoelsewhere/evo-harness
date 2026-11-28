// Opening a file a reply names (docs/features/rich-replies/spec.md, FR-37):
// resolved against the chat's folder, found by name below it when the reply
// gave a bare file name, then shown in the Files tool (Evo Harness) or the
// person's default app for it (Evo Mates). A file that is not there says so.

import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import { fileName, resolvePath, searchFiles, statFile } from "@/features/files/fs";
import { notify } from "@/features/notifications/notify";
import { useEngineStore } from "@/stores/engine-store";
import { openFile } from "@/stores/files-store";
import { parseFileRef } from "./file-refs";

/** The file calls `locate` makes (a test passes its own). */
export interface FileLookup {
  statFile: typeof statFile;
  searchFiles: typeof searchFiles;
}

const disk: FileLookup = { statFile, searchFiles };

/** `fs_stat` answers `{ exists: false }` for a path that is not there; only a file counts. */
async function exists(path: string, fs: FileLookup): Promise<boolean> {
  return fs.statFile(path).then(
    (stat) => stat.exists && !stat.dir,
    () => false,
  );
}

/** The file `reference` names, or null when it cannot be found. */
export async function locate(reference: string, root: string | null, fs: FileLookup = disk): Promise<{ path: string; line?: number } | null> {
  const { path, line } = parseFileRef(reference);
  const direct = root ? resolvePath(root, path) : path;
  if (await exists(direct, fs)) return { path: direct, line };
  // A bare name ("pricing-notes.md") may live in a folder below the root.
  if (root && !/[\\/]/.test(path)) {
    const hits = await fs.searchFiles(root, path, 20).catch(() => []);
    const exact = hits.find((hit) => fileName(hit.path).toLowerCase() === path.toLowerCase());
    if (exact) return { path: exact.path, line };
  }
  return null;
}

export async function openFileRef(reference: string, root: string | null, app: "harness" | "mates"): Promise<void> {
  const found = await locate(reference, root);
  if (!found) {
    notify({ source: "files", level: "warning", title: "File not found", body: `${parseFileRef(reference).path} is not in ${root ?? "this chat's folder"}.` });
    return;
  }
  if (app === "harness") {
    void openFile(found.path, found.line);
    return;
  }
  await openPath(found.path).catch(() => revealItemInDir(found.path).catch(() => undefined));
}

/** A Harness reply's file link: relative to the open chat's folder. */
export function openFromChat(reference: string): void {
  void openFileRef(reference, useEngineStore.getState().cwd ?? null, "harness");
}
