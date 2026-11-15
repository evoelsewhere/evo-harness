// docs/features/plugin-ui/spec.md (FR-17).
import { describe, expect, test } from "bun:test";
import type { InstalledPlugin, RuntimeState } from "@/lib/plugins";
import { runsBackend } from "@/stores/plugin-ui-store";
import { backendStage, opensAt } from "./backend-stage";

const server = (key: string, status: string, error: string | null = null) => ({ key, name: key, status, error, tools: 1, log: [] });
const state = (preparation: RuntimeState["preparation"], ...servers: RuntimeState["servers"]): RuntimeState => ({ preparation, servers });

describe("a plugin backend's stage", () => {
  test("not read yet: starting, and the view waits to open", () => {
    const stage = backendStage(null);
    expect(stage).toEqual({ kind: "starting", inPool: false });
    expect(opensAt(stage)).toBe(false);
  });

  test("preparing: downloading, then installing; the view does not open", () => {
    const downloading = backendStage(state({ phase: "downloading", message: "Getting uv 0.8.0" }));
    expect(downloading).toEqual({ kind: "preparing", step: "downloading", detail: "Getting uv 0.8.0" });
    const installing = backendStage(state({ phase: "installing", message: "Preparing its dependencies" }));
    expect(installing).toMatchObject({ kind: "preparing", step: "installing" });
    expect(opensAt(downloading) || opensAt(installing)).toBe(false);
  });

  test("a failed preparation is a failure the view cannot wake", () => {
    const stage = backendStage(state({ phase: "failed", message: "uv sync failed" }));
    expect(stage).toEqual({ kind: "failed", detail: "uv sync failed", inPool: false });
    expect(opensAt(stage)).toBe(false);
  });

  test("prepared but not in the pool yet: still starting", () => {
    expect(backendStage(state({ phase: "ready" }))).toEqual({ kind: "starting", inPool: false });
    expect(backendStage(state(null, server("plugin:wall:tracker", "connected")))).toEqual({ kind: "starting", inPool: false });
  });

  test("in the pool: connecting opens the view hidden; connected and sleeping show it", () => {
    const connecting = backendStage(state(null, server("plugin:wall:backend", "connecting")));
    expect(connecting).toEqual({ kind: "starting", inPool: true });
    expect(opensAt(connecting)).toBe(true);
    for (const status of ["connected", "sleeping", "off", "needs-auth"]) {
      expect(backendStage(state({ phase: "ready" }, server("plugin:wall:backend", status)))).toEqual({ kind: "ready" });
    }
  });

  test("a failed or crashed backend says why; opening the view tries again", () => {
    for (const status of ["failed", "crashed"]) {
      const stage = backendStage(state(null, server("plugin:wall:backend", status, "exit code 1")));
      expect(stage).toEqual({ kind: "failed", detail: "exit code 1", inPool: true });
      expect(opensAt(stage)).toBe(true);
    }
  });
});

describe("which plugins' views wait for a backend", () => {
  const plugin = (extra: Partial<InstalledPlugin>) =>
    ({ runtime: { language: "python", entry: "server/main.py", startup: "onDemand" }, waiting: [], ...extra }) as unknown as InstalledPlugin;

  test("one with a backend that is to run", () => {
    expect(runsBackend(plugin({}))).toBe(true);
  });

  test("not one without a backend, set never to start, or waiting on the person", () => {
    expect(runsBackend(plugin({ runtime: null }))).toBe(false);
    expect(runsBackend(plugin({ startMode: "off" }))).toBe(false);
    expect(runsBackend(plugin({ waiting: [{ part: "runtime", needs: ["network"] }] as InstalledPlugin["waiting"] }))).toBe(false);
    expect(runsBackend(plugin({ waiting: [{ part: "setup", needs: ["token"] }] as InstalledPlugin["waiting"] }))).toBe(false);
  });

  test("the person's start mode wins over the manifest's", () => {
    const manifestOff = { language: "python", entry: "server/main.py", startup: "off" } as InstalledPlugin["runtime"];
    expect(runsBackend(plugin({ runtime: manifestOff, startMode: "onDemand" }))).toBe(true);
  });
});
