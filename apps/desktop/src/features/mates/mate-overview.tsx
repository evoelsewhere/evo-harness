// A mate's Overview, the profile's first tab (docs/features/evo-mates-ui-v2/
// spec.md, FR-23): what it is (model, approvals, sending, apps, skills), what
// it did today (replies, cost), what it follows up next, the channels it is
// in and its latest files. Every row opens its place. Read from Evo's own
// data; no model call.

import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import { openPath } from "@tauri-apps/plugin-opener";
import { BellRingIcon, CalendarClockIcon, CheckIcon, ChevronRightIcon, FileIcon, HashIcon, MessageCircleQuestionIcon } from "lucide-react";
import { formatCost, formatRelative } from "@/lib/format";
import { matesApi, type AgendaItem, type Mate } from "@/lib/mates";
import { useConnectorsStore } from "@/stores/connectors-store";
import { useEngineStore } from "@/stores/engine-store";
import { useMatesStore } from "@/stores/mates-store";
import { useSessionStore } from "@/stores/session-store";
import { useUiStore } from "@/stores/ui-store";
import { EmptyState } from "./empty-state";
import { dueWords } from "./following-up";
import type { MateFile } from "./files-page";
import { PlacedCardRow, useAssignedCards } from "./assigned-cards";
import { LinkedList } from "./linked-list";

const APPROVAL: Record<string, string> = { ask: "Asks every time", "auto-edits": "Edits on its own", auto: "Acts without asking" };
const KIND_ICON = { promise: CheckIcon, event: CalendarClockIcon, question: MessageCircleQuestionIcon, checkin: BellRingIcon } as const;

function Card({ title, action, onAction, children }: { title: string; action?: string; onAction?(): void; children: React.ReactNode }) {
  return (
    <section className="flex flex-col gap-1.5">
      <div className="flex items-center justify-between px-1">
        <h3 className="text-[11px] font-medium tracking-wide text-muted-foreground uppercase">{title}</h3>
        {action && onAction && (
          <button type="button" onClick={onAction} className="text-[11.5px] font-medium text-brand hover:underline">
            {action}
          </button>
        )}
      </div>
      <div className="overflow-hidden rounded-2xl bg-muted/40">{children}</div>
    </section>
  );
}

function Fact({ label, value, onClick }: { label: string; value: string; onClick?(): void }) {
  const inner = (
    <>
      <span className="text-[12.5px] text-muted-foreground">{label}</span>
      <span className="ml-auto min-w-0 truncate text-right text-[12.5px] font-medium text-foreground">{value}</span>
      {onClick && <ChevronRightIcon className="size-3.5 shrink-0 text-muted-foreground" />}
    </>
  );
  return onClick ? (
    <button type="button" onClick={onClick} className="flex w-full items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-muted/70">
      {inner}
    </button>
  ) : (
    <div className="flex items-center gap-2 px-3 py-2">{inner}</div>
  );
}

function useOverviewData(mate: Mate) {
  const [agenda, setAgenda] = React.useState<AgendaItem[]>([]);
  const [day, setDay] = React.useState<{ replies: number; costUsd: number } | null>(null);
  const [files, setFiles] = React.useState<MateFile[]>([]);
  const mates = useMatesStore((s) => s.mates);
  React.useEffect(() => {
    let gone = false;
    void matesApi
      .agenda(mate.id)
      .then((items) => !gone && setAgenda(items.filter((i) => i.status === "open").sort((a, b) => a.dueAt - b.dueAt).slice(0, 3)))
      .catch(() => undefined);
    void matesApi
      .today()
      .then((today) => !gone && setDay(today.team.find((t) => t.mateId === mate.id) ?? { replies: 0, costUsd: 0 }))
      .catch(() => undefined);
    void invoke<MateFile[]>("mates_files")
      .then((list) => !gone && setFiles(list.filter((f) => f.mateId === mate.id).sort((a, b) => b.modified - a.modified).slice(0, 4)))
      .catch(() => undefined);
    return () => {
      gone = true;
    };
  }, [mate.id, mates]);
  return { agenda, day, files };
}

export function MateOverview({ mate }: { mate: Mate }) {
  const { agenda, day, files } = useOverviewData(mate);
  const allChannels = useMatesStore((s) => s.channels);
  const channels = React.useMemo(() => allChannels.filter((c) => !c.hidden && c.members.some((m) => m.mateId === mate.id)), [allChannels, mate.id]);
  const setTab = useMatesStore((s) => s.setPanelTab);
  const modelRow = useSessionStore((s) => s.sessions.find((c) => c.id === mate.chatId)?.model ?? null);
  const modelName = useEngineStore((s) =>
    s.activeId === mate.chatId ? s.models.find((m) => m.provider === s.state.provider && m.id === (s.state.modelId ?? s.state.model))?.name : undefined,
  );
  const servers = useConnectorsStore((s) => s.servers);
  const off = useUiStore((s) => s.chatConnectorsOff[mate.chatId]) ?? [];
  const appsOn = servers.filter((server) => !off.includes(server.key)).length;
  const openCards = useAssignedCards(mate.id);

  return (
    <div className="flex flex-col gap-5">
      <Card title="At a glance">
        <Fact label="Model" value={modelName ?? modelRow ?? "Evo's default"} />
        <Fact label="Approvals" value={APPROVAL[mate.approvalMode] ?? mate.approvalMode} onClick={() => setTab("settings")} />
        <Fact label="Sending for you" value={mate.sending === "send" ? "Without asking" : "Asks first"} onClick={() => setTab("settings")} />
        <Fact label="Apps" value={servers.length ? `${appsOn} of ${servers.length} on` : "None connected"} onClick={() => setTab("apps")} />
        <Fact label="Skills" value={mate.skills ? `${mate.skills.length} on` : "All"} onClick={() => setTab("apps")} />
      </Card>
      <Card title="Today">
        <div className="grid grid-cols-2 divide-x divide-border/60">
          <div className="flex flex-col px-3 py-2.5">
            <span className="text-[20px] leading-6 font-semibold tracking-tight text-foreground tabular-nums">{day?.replies ?? "–"}</span>
            <span className="text-xs text-muted-foreground">{day?.replies === 1 ? "reply" : "replies"}</span>
          </div>
          <div className="flex flex-col px-3 py-2.5">
            <span className="text-[20px] leading-6 font-semibold tracking-tight text-foreground tabular-nums">{day ? formatCost(day.costUsd) : "–"}</span>
            <span className="text-xs text-muted-foreground">spent</span>
          </div>
        </div>
      </Card>
      <Card title="Following up" action="See all" onAction={() => setTab("settings")}>
        {agenda.length === 0 ? (
          <EmptyState compact text={`Nothing yet. ${mate.name} notes what to come back to.`} className="py-2" />
        ) : (
          <ul className="divide-y divide-border/50">
            {agenda.map((item) => {
              const Icon = KIND_ICON[item.kind] ?? CheckIcon;
              return (
                <li key={item.id} className="flex items-center gap-2.5 px-3 py-2">
                  <Icon className="size-3.5 shrink-0 text-muted-foreground" />
                  <span className="min-w-0 flex-1 truncate text-[12.5px] text-foreground">{item.text}</span>
                  <span className="shrink-0 text-[11px] text-muted-foreground">{dueWords(item.dueAt)}</span>
                </li>
              );
            })}
          </ul>
        )}
      </Card>
      {/* Its open cards across channels (evo-mates-threads-board FR-14). */}
      {openCards.length > 0 && (
        <Card title={`Open cards · ${openCards.length}`}>
          <ul className="divide-y divide-border/50">
            {openCards.map((placed) => (
              <li key={placed.card.id}>
                <PlacedCardRow placed={placed} className="px-3 py-2" />
              </li>
            ))}
          </ul>
        </Card>
      )}
      {/* Shared into or out of its conversation (FR-19). */}
      <Card title="Linked">
        <LinkedList chat={mate.chatId} />
      </Card>
      <Card title="Channels">
        {channels.length === 0 ? (
          <EmptyState compact text={`${mate.name} is in no channel yet.`} className="py-2" />
        ) : (
          <ul className="divide-y divide-border/50">
            {channels.map((channel) => (
              <li key={channel.id}>
                <button
                  type="button"
                  onClick={() => void useMatesStore.getState().openChannel(channel.id)}
                  className="flex w-full items-center gap-2.5 px-3 py-2 text-left transition-colors hover:bg-muted/70"
                >
                  <HashIcon className="size-3.5 shrink-0 text-muted-foreground" />
                  <span className="min-w-0 flex-1 truncate text-[12.5px] font-medium text-foreground">{channel.name}</span>
                  {channel.leadMateId === mate.id && <span className="text-[11px] text-muted-foreground">Lead</span>}
                </button>
              </li>
            ))}
          </ul>
        )}
      </Card>
      <Card title="Latest files" action={files.length ? "Files" : undefined} onAction={() => setTab("files")}>
        {files.length === 0 ? (
          <EmptyState compact text="No files yet." className="py-2" />
        ) : (
          <ul className="divide-y divide-border/50">
            {files.map((file) => (
              <li key={file.path}>
                <button
                  type="button"
                  onClick={() => void openPath(file.path).catch(() => undefined)}
                  className="flex w-full items-center gap-2.5 px-3 py-2 text-left transition-colors hover:bg-muted/70"
                  title={file.path}
                >
                  <FileIcon className="size-3.5 shrink-0 text-muted-foreground" />
                  <span className="min-w-0 flex-1 truncate text-[12.5px] text-foreground">{file.name}</span>
                  <span className="shrink-0 text-[11px] text-muted-foreground">{formatRelative(file.modified)}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </Card>
    </div>
  );
}
