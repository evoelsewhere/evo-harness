// The Updates row in Settings → About (docs/features/app-updates/spec.md,
// FR-3, FR-9, FR-11 to FR-14): the update state, Check for updates, Restart
// to update, and the question when chats are working.

import * as React from "react";
import { DownloadIcon, ExternalLinkIcon, RefreshCwIcon, RotateCwIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Progress } from "@/components/ui/progress";
import { Spinner } from "@/components/ui/spinner";
import { openLink } from "@/features/shell/app-commands";
import { SettingsRow } from "../settings-controls";
import { DOWNLOAD_URL, releaseNotesUrl, useUpdatesStore, type UpdateState } from "@/stores/updates-store";

const when = (at: number) =>
  new Date(at).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });

const lastChecked = (state: UpdateState) => (state.lastCheckedAt ? ` Last checked ${when(state.lastCheckedAt)}.` : "");

function percent(received: number, total: number | null) {
  return total ? Math.min(100, Math.round((received / total) * 100)) : null;
}

export function UpdateRow() {
  const state = useUpdatesStore((s) => s.state);
  const check = useUpdatesStore((s) => s.check);
  const install = useUpdatesStore((s) => s.install);
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
  const notesButton = (version: string) => (
    <Button variant="ghost" size="sm" onClick={() => openLink(releaseNotesUrl(version))}>
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
        description = `evo looks for new versions on its own.${lastChecked(state)}`;
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
    <>
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
      <ConfirmRestart />
    </>
  );
}

/** Chats are working: restarting stops them (FR-9). */
function ConfirmRestart() {
  const confirm = useUpdatesStore((s) => s.confirm);
  const dismiss = useUpdatesStore((s) => s.dismissConfirm);
  const install = useUpdatesStore((s) => s.install);
  const working = confirm?.working ?? 0;
  return (
    <Dialog open={confirm !== null} onOpenChange={(open) => !open && dismiss()}>
      <DialogContent showCloseButton={false}>
        <DialogHeader>
          <DialogTitle>Restart to update?</DialogTitle>
          <DialogDescription>
            {working === 1 ? "A chat is" : `${working} chats are`} working. Restarting stops{" "}
            {working === 1 ? "it" : "them"}; you can continue after the update. Or wait, and choose Restart to update
            later.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={dismiss}>
            Cancel
          </Button>
          <Button autoFocus onClick={() => void install(true)}>
            Restart anyway
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
