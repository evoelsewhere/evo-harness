// Whether a plugin's backend is ready for its views
// (docs/features/plugin-ui/spec.md, FR-17). A Python or TypeScript backend
// starts after its runtime is downloaded and its dependencies installed, the
// first time minutes later; until it runs, a view's `backend.call` fails. The
// slot waits for it: it opens the view once the backend is in the pool (the
// opening wakes it) and shows it once it runs.

import * as React from "react";
import { listen } from "@tauri-apps/api/event";
import { inTauri } from "@/lib/platform";
import { plugins, type RuntimeState } from "@/lib/plugins";

export type BackendStage =
  /** Nothing to wait for: it runs, sleeps (a call wakes it), is off or needs a sign-in. */
  | { kind: "ready" }
  /** Its runtime is being downloaded, or its dependencies installed. */
  | { kind: "preparing"; step: "downloading" | "installing"; detail: string | null }
  /** Prepared; joining the pool or starting. */
  | { kind: "starting"; inPool: boolean }
  /** Its preparation or its start failed. */
  | { kind: "failed"; detail: string | null; inPool: boolean };

/** Where the backend in `state` stands; `null` while it is not read yet. */
export function backendStage(state: RuntimeState | null): BackendStage {
  if (!state) return { kind: "starting", inPool: false };
  const preparation = state.preparation;
  if (preparation?.phase === "downloading" || preparation?.phase === "installing") {
    return { kind: "preparing", step: preparation.phase, detail: preparation.message ?? null };
  }
  if (preparation?.phase === "failed") return { kind: "failed", detail: preparation.message ?? null, inPool: false };
  const backend = state.servers.find((server) => server.key.endsWith(":backend"));
  if (!backend) return { kind: "starting", inPool: false };
  switch (backend.status) {
    case "connecting":
      return { kind: "starting", inPool: true };
    case "failed":
    case "crashed":
      return { kind: "failed", detail: backend.error ?? null, inPool: true };
    default:
      return { kind: "ready" };
  }
}

/** Whether the view may open: its backend is ready or in the pool, where the opening wakes it. */
export const opensAt = (stage: BackendStage) => stage.kind === "ready" || ("inPool" in stage && stage.inPool);

/** The plugin's code as it runs now, read again when the pool or its runtime changes. */
export function usePluginRuntime(id: string, name: string, enabled = true): [RuntimeState | null, () => void] {
  const [state, setState] = React.useState<RuntimeState | null>(null);
  const load = React.useCallback(() => {
    if (!inTauri || !enabled) return;
    void plugins.runtimeState(id, name).then(setState).catch(() => undefined);
  }, [id, name, enabled]);
  React.useEffect(() => {
    load();
    if (!inTauri || !enabled) return;
    const offs = [listen("mcp://changed", load), listen("plugins://runtime", load), listen("plugins://changed", load)];
    return () => offs.forEach((off) => void off.then((stop) => stop()));
  }, [load, enabled]);
  return [state, load];
}

/** The backend's stage for a slot; always ready for a plugin with no backend to wait for. */
export function useBackendStage(id: string, name: string, backend: boolean): BackendStage {
  const watch = backend && inTauri;
  const [state] = usePluginRuntime(id, name, watch);
  return watch ? backendStage(state) : { kind: "ready" };
}
