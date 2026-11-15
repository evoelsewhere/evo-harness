// A plugin's AI threads on its Models tab (docs/features/plugin-ai/spec.md,
// FR-18, FR-19): the conversations it keeps per subject, read as a
// conversation (with the tools the model called) and deleted by the person.

import * as React from "react";
import { listen } from "@tauri-apps/api/event";
import { ChevronDownIcon, ChevronRightIcon, WrenchIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { formatRelative } from "@/lib/format";
import { inTauri } from "@/lib/platform";
import { plugins, type AiThread, type AiThreadMessage, type InstalledPlugin } from "@/lib/plugins";
import { ErrorNote, Section, useBusy } from "./parts";

/** One message; `last` marks the end of the model's turn, where its name shows. */
function Message({ message, last }: { message: AiThreadMessage; last: boolean }) {
  const mine = message.role === "user";
  return (
    <li className={mine ? "flex flex-col items-end gap-1" : "flex flex-col gap-1"}>
      {message.text && (
        <div className={mine ? "max-w-[85%] whitespace-pre-wrap rounded-lg bg-muted px-3 py-1.5 text-[13px]" : "max-w-[95%] whitespace-pre-wrap text-[13px]"}>{message.text}</div>
      )}
      {message.tools?.map((tool) => (
        <details key={tool.id} className="max-w-[95%] rounded-md border border-border/60 px-2 py-1 text-[12px] text-muted-foreground">
          <summary className="flex cursor-pointer items-center gap-1.5">
            <WrenchIcon className="size-3" />
            <span className="font-mono">{tool.name}</span>
            {tool.isError && <span className="text-destructive">failed</span>}
          </summary>
          <pre className="mt-1 overflow-x-auto whitespace-pre-wrap break-words font-mono text-[11px]">{JSON.stringify(tool.input, null, 2)}</pre>
          {tool.result != null && <pre className="mt-1 overflow-x-auto whitespace-pre-wrap break-words border-t border-border/60 pt-1 font-mono text-[11px]">{tool.result}</pre>}
        </details>
      ))}
      {!mine && last && message.model && <span className="text-[11px] text-muted-foreground">{message.model}</span>}
    </li>
  );
}

function ThreadRow({ plugin, thread, onDeleted }: { plugin: InstalledPlugin; thread: AiThread; onDeleted(): void }) {
  const [open, setOpen] = React.useState(false);
  const [messages, setMessages] = React.useState<AiThreadMessage[] | null>(null);
  const [confirm, setConfirm] = React.useState(false);
  const { busy, error, run } = useBusy();
  React.useEffect(() => {
    if (!open) return;
    void run("read", async () => setMessages((await plugins.aiThread(plugin.id, thread.id)).messages));
    // Read again when the thread grows.
  }, [open, plugin.id, thread.id, thread.updatedAt]); // eslint-disable-line react-hooks/exhaustive-deps
  return (
    <li className="flex flex-col gap-2 border-b border-border/60 pb-2 last:border-b-0 last:pb-0">
      <div className="flex items-center gap-2 text-[13px]">
        <button type="button" className="flex min-w-0 flex-1 items-center gap-1.5 text-left" onClick={() => setOpen(!open)} aria-expanded={open}>
          {open ? <ChevronDownIcon className="size-3.5 shrink-0" /> : <ChevronRightIcon className="size-3.5 shrink-0" />}
          <span className="min-w-0 truncate">{thread.title ?? thread.id}</span>
          {thread.title && <span className="shrink-0 truncate text-[11px] text-muted-foreground">{thread.id}</span>}
        </button>
        {thread.going && <span className="size-2 shrink-0 rounded-full bg-status-ok" title="A run continues it now" />}
        <span className="shrink-0 text-[11px] text-muted-foreground">
          {thread.messages} messages · {formatRelative(thread.updatedAt)}
        </span>
        {confirm ? (
          <Button
            size="xs"
            variant="destructive"
            onBlur={() => setConfirm(false)}
            disabled={busy !== null || thread.going}
            onClick={() =>
              void run("delete", async () => {
                await plugins.aiDeleteThread(plugin.id, thread.id);
                onDeleted();
              })
            }
          >
            Delete thread
          </Button>
        ) : (
          <Button size="xs" variant="ghost" disabled={thread.going} onClick={() => setConfirm(true)}>
            Delete
          </Button>
        )}
      </div>
      {open && (
        <div className="max-h-96 overflow-y-auto rounded-lg border border-border/60 px-3 py-2">
          {messages === null ? (
            <p className="text-xs text-muted-foreground">Reading…</p>
          ) : messages.length === 0 ? (
            <p className="text-xs text-muted-foreground">Nothing kept.</p>
          ) : (
            <ul className="flex flex-col gap-3">
              {messages.map((m, i) => (
                <Message key={i} message={m} last={messages[i + 1]?.role !== "assistant"} />
              ))}
            </ul>
          )}
        </div>
      )}
      <ErrorNote error={error} />
    </li>
  );
}

export function ThreadsSection({ plugin }: { plugin: InstalledPlugin }) {
  const [threads, setThreads] = React.useState<AiThread[] | null>(null);
  const load = React.useCallback(() => {
    if (!inTauri) return;
    void plugins.aiThreads(plugin.id).then(setThreads).catch(() => setThreads([]));
  }, [plugin.id]);
  React.useEffect(() => {
    load();
    if (!inTauri) return;
    const off = listen("plugin-ai://runs", load);
    return () => void off.then((stop) => stop());
  }, [load]);
  return (
    <Section title="Threads">
      {threads === null ? null : threads.length === 0 ? (
        <p className="text-xs text-muted-foreground">No threads. A plugin keeps one when it asks its AI to continue a conversation.</p>
      ) : (
        <ul className="flex flex-col gap-2">
          {threads.map((t) => (
            <ThreadRow key={t.id} plugin={plugin} thread={t} onDeleted={load} />
          ))}
        </ul>
      )}
      <p className="text-[11px] text-muted-foreground">Threads stay in evo until you delete them; uninstalling {plugin.name} removes them.</p>
    </Section>
  );
}
