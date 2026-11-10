import * as React from "react";
import { getVersion } from "@tauri-apps/api/app";
import { version as packageVersion } from "../../../../package.json";
import { invoke } from "@tauri-apps/api/core";
import { BookOpenIcon, BugIcon, CopyIcon, CpuIcon, ExternalLinkIcon, FolderOpenIcon, RotateCwIcon } from "lucide-react";
import { EvoLogo } from "@/components/icons/evo-logo";
import { StatusDot } from "@/components/status-dot";
import { Button } from "@/components/ui/button";
import { REPOSITORY_URL, openLink } from "@/features/shell/app-commands";
import { UpdateRow } from "./update-row";
import { inTauri } from "@/lib/platform";
import { useEngineStore, type EngineStatus } from "@/stores/engine-store";
import { SettingsGroup, SettingsHeader, SettingsRow } from "../settings-controls";

const STATUS: Record<EngineStatus, { tone: "ok" | "warn" | "error" | "idle"; text: string }> = {
  idle: { tone: "idle", text: "Idle" },
  starting: { tone: "warn", text: "Starting…" },
  ready: { tone: "ok", text: "Running" },
  exited: { tone: "error", text: "Stopped" },
  error: { tone: "error", text: "Failed to start" },
};

export function EngineSettings() {
  const status = useEngineStore((s) => s.status);
  const error = useEngineStore((s) => s.error);
  const cwd = useEngineStore((s) => s.cwd);
  const activeId = useEngineStore((s) => s.activeId);
  const allLogs = useEngineStore((s) => s.logs);
  const logs = React.useMemo(() => allLogs.filter((l) => l.chat === activeId), [allLogs, activeId]);
  const running = useEngineStore((s) => Object.values(s.runtimes).filter((rt) => rt.status === "ready").length);
  const busy = useEngineStore((s) => Object.values(s.runtimes).filter((rt) => rt.transcript.busy).length);
  const models = useEngineStore((s) => s.models);
  const start = useEngineStore((s) => s.start);
  const [copied, setCopied] = React.useState(false);
  // The engine's own data (database, sessions, credentials, logs) lives in the
  // user's folder, never in a workspace.
  const [dataDir, setDataDir] = React.useState<string | null>(null);
  React.useEffect(() => {
    if (!inTauri) return;
    invoke<{ dir: string }>("db_info")
      .then((info) => setDataDir(info.dir))
      .catch(() => setDataDir(null));
  }, []);
  const { tone, text } = STATUS[status];

  async function copyLog() {
    await navigator.clipboard.writeText(logs.map((l) => l.line).join("\n"));
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  }

  return (
    <div className="flex flex-col gap-8">
      <SettingsHeader
        icon={CpuIcon}
        title="Engine"
        description="The local evo engine runs beside the app and does the agent's work."
      />
      <SettingsGroup title="Status">
        <SettingsRow
          label="Engine"
          description={error ?? "This chat's engine."}
          control={
            <span className="flex items-center gap-2 text-[13px]">
              <StatusDot tone={tone} pulse={status === "starting"} />
              {text}
            </span>
          }
        />
        <SettingsRow
          label="Data folder"
          description={
            dataDir ? (
              <>
                Chats, sessions, credentials and logs, on this computer:{" "}
                <span className="font-mono text-xs">{dataDir}</span>
              </>
            ) : (
              "Chats, sessions, credentials and logs, on this computer."
            )
          }
          control={
            <Button variant="outline" size="sm" disabled={!dataDir} onClick={() => void invoke("db_reveal").catch(() => undefined)}>
              <FolderOpenIcon />
              Show
            </Button>
          }
        />
        <SettingsRow
          label="Working folder"
          description={
            cwd ? (
              <>
                The folder this chat works in. Nothing of the engine's is stored there.{" "}
                <span className="font-mono text-xs">{cwd}</span>
              </>
            ) : (
              "Not attached to a folder"
            )
          }
        />
        <SettingsRow
          label="Parallel chats"
          description="Every open chat runs its own engine, so chats keep working while you switch. Idle ones close after a while and reopen from the saved chat."
          control={
            <span className="font-mono text-xs text-muted-foreground">
              {running} running · {busy} working
            </span>
          }
        />
        <SettingsRow
          label="Restart engine"
          description="Restarts this chat's engine. A reply in progress is cancelled; the conversation is kept."
          control={
            <Button variant="outline" size="sm" onClick={() => void start()}>
              <RotateCwIcon />
              Restart
            </Button>
          }
        />
        <SettingsRow
          label="Models"
          description="Models the engine reported as available."
          control={<span className="font-mono text-xs text-muted-foreground">{models.length}</span>}
        />
      </SettingsGroup>
      <SettingsGroup title="Diagnostics">
        <SettingsRow
          label="Engine log"
          description={`${logs.length} line${logs.length === 1 ? "" : "s"} of engine output from this run.`}
          control={
            <Button variant="outline" size="sm" disabled={logs.length === 0} onClick={() => void copyLog()}>
              <CopyIcon />
              {copied ? "Copied" : "Copy log"}
            </Button>
          }
        />
      </SettingsGroup>
    </div>
  );
}

export function AboutSettings() {
  const [version, setVersion] = React.useState<string>();
  React.useEffect(() => {
    (inTauri ? getVersion() : Promise.resolve(`${packageVersion} (preview)`)).then(setVersion).catch(() => setVersion("unknown"));
  }, []);

  return (
    <div className="flex flex-col gap-8">
      <header className="flex items-center gap-4">
        <EvoLogo className="size-16 rounded-2xl shadow-sm ring-1 ring-border" />
        <div className="min-w-0">
          <h1 className="text-lg font-semibold tracking-tight">Evo Harness</h1>
          <p className="mt-0.5 text-[13px] text-muted-foreground">A local-first desktop workspace for AI agents.</p>
          <p className="mt-1.5 flex items-center gap-2 text-xs text-muted-foreground">
            <span className="rounded-full bg-muted px-2 py-0.5 font-mono">v{version ?? "…"}</span>
            <span>Apache 2.0 licensed</span>
          </p>
        </div>
      </header>
      <SettingsGroup title="Updates">
        <UpdateRow />
      </SettingsGroup>
      <SettingsGroup title="Help">
        <SettingsRow
          label="Documentation"
          description="Setup, configuration and how the agent works."
          control={
            <Button variant="outline" size="sm" onClick={() => openLink(`${REPOSITORY_URL}#readme`)}>
              <BookOpenIcon />
              Open
            </Button>
          }
        />
        <SettingsRow
          label="Report an issue"
          description="Something broken or confusing? Open an issue on GitHub."
          control={
            <Button variant="outline" size="sm" onClick={() => openLink(`${REPOSITORY_URL}/issues/new`)}>
              <BugIcon />
              Report
            </Button>
          }
        />
        <SettingsRow
          label="Source code"
          description={<span className="font-mono text-xs">{REPOSITORY_URL.replace("https://", "")}</span>}
          control={
            <Button variant="outline" size="sm" onClick={() => openLink(REPOSITORY_URL)}>
              <ExternalLinkIcon />
              GitHub
            </Button>
          }
        />
      </SettingsGroup>
    </div>
  );
}
