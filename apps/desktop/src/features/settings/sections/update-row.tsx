// The Updates rows in Settings → About (docs/features/app-updates/spec.md,
// FR-2, FR-3, FR-11 to FR-14, FR-16): how evo checks (automatically or by
// hand), the update state, Check for updates, What's new (the new-version
// dialog) and Restart to update. The question when chats are working is in
// features/updates, mounted once for every place that restarts.

import * as React from "react";
import { DownloadIcon, ExternalLinkIcon, HandIcon, RefreshCwIcon, RotateCwIcon, SparklesIcon, ZapIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { Spinner } from "@/components/ui/spinner";
import { notify } from "@/features/notifications/notify";
import { openLink } from "@/features/shell/app-commands";
import { Segmented, SettingsRow } from "../settings-controls";
import { DOWNLOAD_URL, useUpdatesStore, type UpdateState } from "@/stores/updates-store";

const when = (at: number) =>
  new Date(at).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });

const lastChecked = (state: UpdateState) => (state.lastCheckedAt ? ` Last checked ${when(state.lastCheckedAt)}.` : "");

function percent(received: number, total: number | null) {
  return total ? Math.min(100, Math.round((received / total) * 100)) : null;
}

/** Automatic or manual checks (FR-2): the app setting `updates.autoCheck`. */
export function UpdateModeRow() {
  const autoCheck = useUpdatesStore((s) => s.autoCheck);
  const load = useUpdatesStore((s) => s.loadAutoCheck);
  const setAutoCheck = useUpdatesStore((s) => s.setAutoCheck);
  React.useEffect(() => {
    if (autoCheck === null) void load();
  }, [autoCheck, load]);
  const change = (mode: "auto" | "manual") =>
    setAutoCheck(mode === "auto").catch((error: unknown) =>
      notify({ source: "settings", level: "error", title: "Could not change the setting", body: String(error) }),
    );
  return (
    <SettingsRow
      labelId="setting-updatesAutoCheck"
      label="Check for updates"
      description={
        autoCheck === false
          ? "Only when you choose Check for updates below. evo makes no update request on its own."
          : "Shortly after evo starts and every 6 hours, downloading a new version in the background and showing what changed. Nothing is installed until you choose Restart to update."
      }
      control={
        <Segmented
          labelledBy="setting-updatesAutoCheck"
          value={autoCheck === false ? "manual" : "auto"}
          onValueChange={change}
          options={[
            { value: "auto", label: "Automatically", icon: ZapIcon },
            { value: "manual", label: "Manually", icon: HandIcon },
          ]}
        />
      }
    />
  );
}

export function UpdateRow() {
  const state = useUpdatesStore((s) => s.state);
  const check = useUpdatesStore((s) => s.check);
  const install = useUpdatesStore((s) => s.install);
  const autoCheck = useUpdatesStore((s) => s.autoCheck);
  const openWhatsNew = useUpdatesStore((s) => s.openWhatsNew);
  const [error, setError] = React.useState<string | null>(null);

  const run = (action: () => Promise<void>) => {
    setError(null);
    action().catch((e: unknown) => setError(String(e)));
  };
  const checkButton = (label = "Check for updates", disabled = false) => (
    <Button variant="outline" size="sm" disabled={disabled} onClick={() => run(check)}>
      {disabled ? <Spinner /> : <RefreshCwIcon />}
      {label}
    </Button>
  );
  const notesButton = (_version: string) => (
    <Button variant="ghost" size="sm" onClick={openWhatsNew}>
      <SparklesIcon />
      What's new
    </Button>
  );
  const downloadButton = (
    <Button variant="outline" size="sm" onClick={() => openLink(DOWNLOAD_URL)}>
      <ExternalLinkIcon />
      Download page
    </Button>
  );

  let description: React.ReactNode;
  let control: React.ReactNode = null;
  let progress: React.ReactNode = null;
  const status = state?.status;

  if (!state || !status || status.kind === "unavailable") {
    description = "Updates aren't available in this build.";
  } else {
    switch (status.kind) {
      case "idle":
        description =
          autoCheck === false
            ? `Automatic checks are off.${lastChecked(state)}`
            : `evo looks for new versions on its own.${lastChecked(state)}`;
        control = checkButton();
        break;
      case "checking":
        description = "Checking for updates…";
        control = checkButton("Checking…", true);
        break;
      case "upToDate":
        description = `Evo Harness is up to date.${lastChecked(state)}`;
        control = checkButton();
        break;
      case "downloading": {
        const done = percent(status.received, status.total);
        description = `Downloading Evo Harness ${status.version}${done === null ? "…" : ` · ${done}%`}`;
        control = notesButton(status.version);
        progress = <Progress value={done} aria-label="Download progress" />;
        break;
      }
      case "ready":
        description = `Evo Harness ${status.version} is ready. Restart evo to finish updating; your chats and settings are kept.`;
        control = (
          <span className="flex gap-1.5">
            {notesButton(status.version)}
            <Button size="sm" onClick={() => run(() => install(false))}>
              <RotateCwIcon />
              Restart to update
            </Button>
          </span>
        );
        break;
      case "notice":
        description = `Evo Harness ${status.version} is available. Install it from the download page.`;
        control = (
          <span className="flex gap-1.5">
            {notesButton(status.version)}
            {downloadButton}
          </span>
        );
        break;
      case "installing":
        description = `Installing Evo Harness ${status.version}… evo will restart.`;
        control = (
          <Button size="sm" disabled>
            <Spinner />
            Installing…
          </Button>
        );
        break;
      case "failed":
        switch (status.stage) {
          case "check":
            description = `Couldn't check for updates: ${status.message}`;
            control = checkButton("Try again");
            break;
          case "download":
            description = `The download of Evo Harness ${status.version ?? ""} failed: ${status.message}. evo tries again at the next check.`;
            control = checkButton("Try again");
            break;
          case "verify":
            description = `The update ${status.version ?? ""} could not be verified, so it was deleted and not installed.`;
            control = checkButton("Try again");
            break;
          case "install":
            description = `Couldn't install Evo Harness ${status.version ?? ""}: ${status.message}. This version keeps working.`;
            control = (
              <span className="flex gap-1.5">
                {downloadButton}
                <Button size="sm" onClick={() => run(() => install(false))}>
                  <DownloadIcon />
                  Try again
                </Button>
              </span>
            );
            break;
        }
        break;
    }
  }

  return (
    <SettingsRow
      label="Status"
      description={
        <>
          {description}
          {error && <span className="mt-1 block text-destructive">{error}</span>}
        </>
      }
      control={control}
    >
      {progress}
    </SettingsRow>
  );
}
