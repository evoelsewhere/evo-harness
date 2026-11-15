import * as React from "react";
import { ShieldAlertIcon, TriangleAlertIcon } from "lucide-react";
import type { PermissionRow } from "@/lib/plugins";

/** One action at a time, with its error (Customize → Plugins). */
export function useBusy() {
  const [busy, setBusy] = React.useState<string | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const run = async (key: string, work: () => Promise<unknown>) => {
    setBusy(key);
    setError(null);
    try {
      await work();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };
  return { busy, error, run, setError };
}

export function ErrorNote({ error }: { error: string | null }) {
  if (!error) return null;
  return (
    <div className="flex gap-2 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs">
      <TriangleAlertIcon className="mt-px size-3.5 shrink-0 text-destructive" />
      <span className="min-w-0 break-words">{error}</span>
    </div>
  );
}

/** A titled block of a tab. */
export function Section({ title, children, aside }: { title: string; children: React.ReactNode; aside?: React.ReactNode }) {
  return (
    <section className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <h3 className="flex-1 text-xs font-medium text-muted-foreground">{title}</h3>
        {aside}
      </div>
      <div className="flex flex-col gap-3 rounded-xl border border-border/70 px-4 py-3">{children}</div>
    </section>
  );
}

/** "12.4 MB". */
export function bytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB"];
  let value = n / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value < 10 ? 1 : 0)} ${units[unit]}`;
}

/** "hooks, connector servers and Python backend". */
export function sentence(items: string[]): string {
  if (items.length <= 1) return items[0] ?? "";
  return `${items.slice(0, -1).join(", ")} and ${items.at(-1)}`;
}

/** The note under a permission: its reason, why it is needed, what it does not bind. */
export function PermissionNotes({ permission }: { permission: PermissionRow }) {
  const parts = permission.notEnforcedFor ?? [];
  return (
    <>
      {permission.reason && <span className="text-[11px] text-muted-foreground">{permission.reason}</span>}
      {permission.implied && <span className="text-[11px] text-muted-foreground">Needed by what the plugin contains.</span>}
      {parts.length > 0 && (
        <span className="flex items-center gap-1 text-[11px] text-amber-600 dark:text-amber-400">
          <ShieldAlertIcon className="size-3" />
          Not enforced for its {sentence(parts)}: evo cannot hold them to it.
        </span>
      )}
    </>
  );
}
