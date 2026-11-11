// The new-version dialog (docs/features/app-updates/spec.md, FR-16, FR-17):
// opens by itself when an automatic check finds a version, and from What's
// new in Settings › About. It shows the release notes in sections and the
// one action that applies: Restart to update, or Download where the app
// cannot install (FR-12). Also the question when chats are working (FR-9),
// for every place that offers Restart to update.

import * as React from "react";
import { ArrowRightIcon, ExternalLinkIcon, RotateCwIcon } from "lucide-react";
import { Markdown } from "@/components/markdown";
import { EvoLogo } from "@/components/icons/evo-logo";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Spinner } from "@/components/ui/spinner";
import { openLink } from "@/features/shell/app-commands";
import { notify } from "@/features/notifications/notify";
import { cn } from "@/lib/utils";
import {
  DOWNLOAD_URL,
  releaseNotesUrl,
  useUpdatesStore,
  type UpdateRelease,
  type UpdateStatus,
} from "@/stores/updates-store";
import { changeCounts, parseChangelog, type ChangeEntry, type ChangeKind, type ChangeSection } from "./changelog";

/** Status dots (DESIGN.md: colour only as small indicators). */
const DOT: Record<ChangeKind, string> = {
  breaking: "bg-[#e5484d]",
  features: "bg-brand",
  fixes: "bg-[#45a557]",
  performance: "bg-[#ff990a]",
  other: "bg-muted-foreground/60",
};

const day = (at: number) => new Date(at).toLocaleDateString(undefined, { dateStyle: "medium" });

function percent(received: number, total: number | null) {
  return total ? Math.min(100, Math.round((received / total) * 100)) : null;
}

/** Inline `code` in a commit subject. */
function Inline({ text }: { text: string }) {
  const parts = text.split(/(`[^`]+`)/g);
  return (
    <>
      {parts.map((part, index) =>
        part.startsWith("`") && part.endsWith("`") && part.length > 2 ? (
          <code key={index} className="rounded-sm bg-muted px-1 py-px font-mono text-[0.9em]">
            {part.slice(1, -1)}
          </code>
        ) : (
          <React.Fragment key={index}>{part}</React.Fragment>
        ),
      )}
    </>
  );
}

function Entry({ entry }: { entry: ChangeEntry }) {
  return (
    <li className="group/entry flex items-baseline gap-2 py-1.5 text-[13px] leading-5">
      <span aria-hidden className="mt-2 size-1 shrink-0 self-start rounded-full bg-muted-foreground/40" />
      <span className="min-w-0 flex-1 text-foreground/90">
        {entry.scope && (
          <span className="mr-1.5 inline-flex h-[18px] items-center rounded-[5px] bg-muted px-1.5 align-[1px] font-mono text-[11px] text-muted-foreground shadow-hairline">
            {entry.scope}
          </span>
        )}
        <Inline text={entry.text} />
      </span>
      {entry.commit && (
        <button
          type="button"
          onClick={() => openLink(entry.commit!.url)}
          title="Open the commit"
          className="shrink-0 rounded font-mono text-[11px] text-muted-foreground/70 opacity-0 transition-opacity outline-none group-hover/entry:opacity-100 hover:text-link focus-visible:opacity-100"
        >
          {entry.commit.sha}
        </button>
      )}
    </li>
  );
}

function Section({ section }: { section: ChangeSection }) {
  return (
    <section>
      <h3 className="flex items-center gap-2 text-[12px] font-medium tracking-wide text-muted-foreground uppercase">
        <span aria-hidden className={cn("size-2 rounded-full", DOT[section.kind])} />
        {section.title}
        <span className="font-mono text-[11px] font-normal tabular-nums text-muted-foreground/70">
          {section.entries.length}
        </span>
      </h3>
      <ul className="mt-1.5">
        {section.entries.map((entry, index) => (
          <Entry key={index} entry={entry} />
        ))}
      </ul>
    </section>
  );
}

/** The release notes, laid out; Markdown when they are in another shape. */
function Notes({ release }: { release: UpdateRelease }) {
  const changelog = React.useMemo(() => (release.notes ? parseChangelog(release.notes) : null), [release.notes]);
  if (!release.notes) {
    return (
      <div className="flex flex-col items-center gap-3 px-6 py-10 text-center">
        <p className="max-w-xs text-[13px] text-muted-foreground">
          The notes for this version are on its release page.
        </p>
        <Button variant="outline" size="sm" onClick={() => openLink(releaseNotesUrl(release.version))}>
          <ExternalLinkIcon />
          Read the release notes
        </Button>
      </div>
    );
  }
  if (!changelog) {
    return <Markdown className="px-6 py-5 text-[13px]">{release.notes}</Markdown>;
  }
  return (
    <div className="flex flex-col gap-5 px-6 py-5">
      {changelog.summary.length > 0 && (
        <p className="text-[13px] text-muted-foreground">{changelog.summary.join(" ")}</p>
      )}
      {changelog.sections.map((section) => (
        <Section key={section.title} section={section} />
      ))}
    </div>
  );
}

function headline(status: UpdateStatus | undefined, version: string) {
  switch (status?.kind) {
    case "ready":
      return {
        title: `Evo Harness ${version} is ready`,
        line: "Restart to finish updating. Your chats and settings are kept.",
      };
    case "downloading":
      return {
        title: `Evo Harness ${version} is on its way`,
        line: "It is downloading in the background; keep working.",
      };
    case "installing":
      return {
        title: `Installing Evo Harness ${version}`,
        line: "evo will restart in a moment.",
      };
    case "notice":
      return {
        title: `Evo Harness ${version} is available`,
        line: "Install it from the download page.",
      };
    default:
      return {
        title: `What's new in Evo Harness ${version}`,
        line: "The changes in this release.",
      };
  }
}

/** The one action that applies to the update's state. */
function PrimaryAction({ status, onInstall }: { status: UpdateStatus | undefined; onInstall(): void }) {
  switch (status?.kind) {
    case "ready":
      return (
        <Button autoFocus onClick={onInstall}>
          <RotateCwIcon />
          Restart to update
        </Button>
      );
    case "downloading": {
      const done = percent(status.received, status.total);
      return (
        <Button disabled className="relative overflow-hidden">
          <Spinner />
          {done === null ? "Downloading…" : `Downloading ${done}%`}
          {done !== null && (
            <span
              aria-hidden
              className="absolute inset-x-0 bottom-0 h-0.5 bg-brand-foreground/40"
              style={{ width: `${done}%` }}
            />
          )}
        </Button>
      );
    }
    case "installing":
      return (
        <Button disabled>
          <Spinner />
          Installing…
        </Button>
      );
    case "notice":
      return (
        <Button autoFocus onClick={() => openLink(DOWNLOAD_URL)}>
          <ExternalLinkIcon />
          Download
        </Button>
      );
    default:
      return null;
  }
}

export function UpdateDialog() {
  const open = useUpdatesStore((s) => s.dialogOpen);
  const close = useUpdatesStore((s) => s.closeDialog);
  const state = useUpdatesStore((s) => s.state);
  const install = useUpdatesStore((s) => s.install);
  const release = state?.release ?? null;
  const status = state?.status;
  // Keep the last release on screen while the dialog animates out.
  const shown = React.useRef(release);
  if (release) shown.current = release;
  const current = shown.current;
  if (!current) return null;

  const { title, line } = headline(status, current.version);
  const changelog = current.notes ? parseChangelog(current.notes) : null;
  const counts = changelog && changelog.sections.length > 0 ? changeCounts(changelog) : null;

  // Chats working: the question below takes over (FR-9).
  const restart = () => {
    close();
    install(false).catch((e: unknown) =>
      notify({ source: "updates", level: "error", title: "Could not restart to update", body: String(e) }),
    );
  };

  return (
    <Dialog open={open && release !== null} onOpenChange={(next) => !next && close()}>
      <DialogContent className="flex max-h-[min(640px,calc(100vh-4rem))] flex-col gap-0 overflow-hidden p-0 sm:max-w-lg">
        <DialogHeader className="shrink-0 gap-0 px-6 pt-6 pb-5">
          <div className="flex items-start gap-4">
            <EvoLogo className="size-12 rounded-[14px] shadow-sm ring-1 ring-border" />
            <div className="min-w-0 flex-1 pr-8">
              <p className="flex items-center gap-1.5 text-[12px] font-medium text-brand">
                <span aria-hidden className="relative flex size-2">
                  <span className="absolute inset-0 animate-ping rounded-full bg-brand/50 motion-reduce:animate-none" />
                  <span className="relative size-2 rounded-full bg-brand" />
                </span>
                New version
              </p>
              <DialogTitle className="mt-1 text-[17px] leading-6 font-semibold tracking-tight">{title}</DialogTitle>
              <DialogDescription className="mt-0.5 text-[13px]">{line}</DialogDescription>
            </div>
          </div>
          <div className="mt-4 flex flex-wrap items-center gap-x-3 gap-y-1.5 text-xs text-muted-foreground">
            <span className="inline-flex items-center gap-1.5 rounded-full bg-muted px-2.5 py-1 font-mono shadow-hairline">
              <span>v{state?.current ?? "?"}</span>
              <ArrowRightIcon className="size-3 opacity-60" />
              <span className="text-foreground">v{current.version}</span>
            </span>
            {current.publishedAt && <span>Released {day(current.publishedAt)}</span>}
            {counts && <span>{counts}</span>}
          </div>
        </DialogHeader>
        <div
          className="min-h-0 flex-auto overflow-y-auto overscroll-contain shadow-[inset_0_1px_0_var(--hairline),inset_0_-1px_0_var(--hairline)]"
          data-selectable
        >
          <Notes release={current} />
        </div>
        <DialogFooter className="shrink-0 items-center bg-muted/40 px-6 py-4 sm:justify-between">
          {current.notes ? (
            <Button
              variant="ghost"
              size="sm"
              className="text-muted-foreground"
              onClick={() => openLink(changelog?.compareUrl ?? releaseNotesUrl(current.version))}
            >
              <ExternalLinkIcon />
              {changelog?.compareUrl ? "Full changelog" : "Release page"}
            </Button>
          ) : (
            <span />
          )}
          <div className="flex flex-col-reverse gap-2 sm:flex-row">
            <Button variant="outline" onClick={close}>
              {status?.kind === "ready" || status?.kind === "downloading" ? "Later" : "Close"}
            </Button>
            <PrimaryAction status={status} onInstall={restart} />
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/** Chats are working: restarting stops them (FR-9). */
export function ConfirmRestart() {
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

/** Mounted once, in the main window: the dialogs updates raise. */
export function UpdatesHost() {
  return (
    <>
      <UpdateDialog />
      <ConfirmRestart />
    </>
  );
}
