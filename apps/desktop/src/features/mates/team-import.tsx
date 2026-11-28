// Team files (docs/features/evo-mates/spec.md, FR-20, FR-21, FR-27): export a
// mate or a channel to one file, and import one after a preview that shows
// what it adds and creates nothing until the person confirms.

import * as React from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { AlertTriangleIcon, CheckIcon } from "lucide-react";
import { MateFace } from "@/components/mate-face/mate-face";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { log } from "@/lib/db";
import { teamApi, type TeamPreview } from "@/lib/mates";
import { cn } from "@/lib/utils";
import { useMatesStore } from "@/stores/mates-store";
import { useSessionStore } from "@/stores/session-store";

const MODE_WORDS: Record<string, string> = { ask: "Ask every time", "auto-edits": "Approve edits", auto: "Approve for me" };

/** A file name for an export: the name in plain characters. */
export function teamFileName(name: string): string {
  const plain = name.trim().replace(/[\\/:*?"<>|]+/g, "").replace(/\s+/g, " ") || "team";
  return `${plain}.evo-team.md`;
}

/** Export a mate or a channel to a file the person picks (FR-20). */
export async function exportTeam(target: { mate?: string; channel?: string }, name: string): Promise<void> {
  const path = await save({ title: `Export ${name}`, defaultPath: teamFileName(name), filters: [{ name: "Evo team", extensions: ["md"] }] });
  if (!path) return;
  try {
    await teamApi.export(target, path);
  } catch (error) {
    log("warn", "mates", `could not export: ${String(error)}`);
  }
}

/**
 * Pick a team file and show its preview (FR-21); with `team`, preview one of
 * the gallery's ready-made teams instead (FR-75).
 */
export function ImportTeam({ open: shown, onOpenChange, team }: { open: boolean; onOpenChange(open: boolean): void; team?: string }) {
  const [path, setPath] = React.useState<string | null>(null);
  const [preview, setPreview] = React.useState<TeamPreview | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [skip, setSkip] = React.useState<string[]>([]);
  const [busy, setBusy] = React.useState(false);

  // Opening the dialog asks for the file first.
  React.useEffect(() => {
    if (!shown) return;
    setPreview(null);
    setError(null);
    setSkip([]);
    void (async () => {
      if (team) {
        setPath(null);
        try {
          setPreview(await teamApi.previewBuiltin(team));
        } catch (e) {
          setError(String(e).replace(/^Error:\s*/, ""));
        }
        return;
      }
      const picked = await open({ title: "Import a team", multiple: false, filters: [{ name: "Evo team", extensions: ["md"] }] });
      if (typeof picked !== "string") {
        onOpenChange(false);
        return;
      }
      setPath(picked);
      try {
        setPreview(await teamApi.preview(picked));
      } catch (e) {
        setError(String(e).replace(/^Error:\s*/, ""));
      }
    })();
  }, [shown, onOpenChange, team]);

  const run = async () => {
    if ((!path && !team) || !preview) return;
    setBusy(true);
    try {
      const made = team ? await teamApi.importBuiltin(team, skip) : await teamApi.import(path!, skip);
      const store = useMatesStore.getState();
      await store.load();
      // The new chats reach the session store before they can be opened, as a new mate's do.
      await useSessionStore.getState().load();
      if (made.channel) await store.openChannel(made.channel);
      else if (made.mates[0]) await store.open(made.mates[0]);
      onOpenChange(false);
    } catch (e) {
      setError(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  };

  const kept = preview ? preview.mates.filter((m) => !skip.includes(m.name)) : [];

  return (
    <Dialog open={shown && Boolean(preview || error)} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md rounded-[22px]">
        <DialogHeader>
          <DialogTitle>{preview?.channel ? `${team ? "Add" : "Import"} #${preview.channel.name}` : "Import mates"}</DialogTitle>
          <DialogDescription>Nothing is created until you import. Imported mates start careful: your approval mode, every app off, skills off, and they ask before sending.</DialogDescription>
        </DialogHeader>
        {error ? (
          <p className="rounded-2xl bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</p>
        ) : preview ? (
          <div className="flex max-h-[55vh] flex-col gap-2 overflow-y-auto px-0.5 py-1">
            {preview.channel && (
              <div className="rounded-2xl bg-muted/40 px-3 py-2 text-xs text-muted-foreground">
                A channel, <span className="font-medium text-foreground">#{preview.channel.name}</span>
                {preview.channel.lead ? `, led by ${preview.channel.lead}` : ""}
                {preview.channel.instructions ? `: ${preview.channel.instructions}` : "."}
              </div>
            )}
            {preview.mates.map((mate) => {
              const on = !skip.includes(mate.name);
              return (
                <button
                  key={mate.name}
                  type="button"
                  role="checkbox"
                  aria-checked={on}
                  onClick={() => setSkip((s) => (on ? [...s, mate.name] : s.filter((n) => n !== mate.name)))}
                  className={cn(
                    "flex items-start gap-2.5 rounded-2xl bg-muted/40 px-3 py-2 text-left transition-colors hover:bg-muted/80",
                    on && "bg-brand/5 shadow-[0_0_0_1.5px_var(--brand)]",
                  )}
                >
                  <MateFace avatar={mate.avatar ?? {}} size={28} plate={false} label={mate.name} />
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-1.5 text-[13px] font-medium text-foreground">
                      {mate.name}
                      {on && <CheckIcon className="size-3.5 text-brand" />}
                    </div>
                    {mate.role && <div className="truncate text-xs text-muted-foreground">{mate.role}</div>}
                    <div className="text-[11px] text-muted-foreground">
                      {MODE_WORDS[mate.approvalMode] ?? mate.approvalMode}
                      {mate.asked && mate.asked !== mate.approvalMode ? ` (the file asked for ${MODE_WORDS[mate.asked] ?? mate.asked})` : ""}
                    </div>
                    {mate.missingConnectors.length > 0 && (
                      <div className="mt-0.5 flex items-center gap-1 text-[11px] text-amber-700 dark:text-amber-300">
                        <AlertTriangleIcon className="size-3" />
                        Not connected here: {mate.missingConnectors.join(", ")}
                      </div>
                    )}
                  </div>
                </button>
              );
            })}
            {preview.unknown.length > 0 && (
              <p className="text-[11px] text-muted-foreground">Left out, this Evo does not know them: {preview.unknown.join(", ")}.</p>
            )}
          </div>
        ) : null}
        <DialogFooter>
          <Button variant="ghost" className="rounded-full" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          {!error && (
            <Button
              disabled={busy || kept.length === 0}
              onClick={() => void run()}
              className="rounded-full bg-brand px-4 text-brand-foreground hover:bg-brand/90"
            >
              {team ? "Add" : "Import"} {kept.length} {kept.length === 1 ? "mate" : "mates"}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
