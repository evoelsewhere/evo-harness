// A mate's header pill and panel (docs/features/evo-mates/ux.md 2; spec
// FR-7, FR-10, FR-13, FR-25, FR-29). The header floats over the
// conversation: face, name, role or state word, the panel toggle. The panel
// holds the profile: About, Settings and Files; changes apply from the
// mate's next turn.

import * as React from "react";
import { open as pickFolder } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { FolderOpenIcon, PanelRightIcon, PencilIcon, XIcon } from "lucide-react";
import { faceColors, MateFace } from "@/components/mate-face/mate-face";
import { faces } from "@/components/mate-face/faces";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { matesApi, type ApprovalMode, type Mate } from "@/lib/mates";
import { cn } from "@/lib/utils";
import { useEngineStore } from "@/stores/engine-store";
import { mateMood, moodWord, useMatesStore, type MatePanelTab } from "@/stores/mates-store";
import { connectorLabel } from "@/lib/mcp";
import { useConnectorsStore } from "@/stores/connectors-store";
import { useSessionStore } from "@/stores/session-store";
import { useUiStore } from "@/stores/ui-store";
import { MateConnectedApps } from "@/features/connected-apps";
import { ConfirmDelete } from "./confirm";
import { FollowingUp } from "./following-up";
import { MateSkills } from "./skills-tab";

/** A new conversation's first screen: the mate, ready to talk (ux.md 5). */
export function MateGreeting({ mate }: { mate: Mate }) {
  return (
    <div className="mx-auto flex w-full max-w-(--chat-width) flex-col items-center gap-3 px-6 pb-6 text-center">
      <MateFace avatar={mate.avatar} size={88} live plate={false} label={mate.name} />
      <h1 className="text-2xl font-semibold tracking-tight text-foreground">Say hi to {mate.name}</h1>
      {mate.role && <p className="max-w-md text-sm text-muted-foreground">{mate.role}</p>}
    </div>
  );
}

export function useShownMate(): Mate | null {
  return useMatesStore((s) => {
    const view = s.view;
    return view.kind === "mate" ? (s.mates.find((m) => m.id === view.id) ?? null) : null;
  });
}

/**
 * The conversation's bar, in the window's title row (ux.md 2): the mate's
 * face, name and state, Edit; on the right its model and the profile toggle.
 */
export function MateBar({ mate, modelPill }: { mate: Mate; modelPill?: React.ReactNode }) {
  const work = useSessionStore((s) => s.work[mate.chatId]);
  const busy = useEngineStore((s) => s.activeId === mate.chatId && s.transcript.busy);
  const queued = useMatesStore((s) => s.queued.includes(mate.chatId));
  const mood = mateMood(work, busy, queued);
  const panelOpen = useMatesStore((s) => s.panelOpen);
  const setPanelOpen = useMatesStore((s) => s.setPanelOpen);
  const word = mate.paused ? "Paused" : moodWord(mood);
  return (
    <div className="flex min-w-0 flex-1 items-center gap-2 pl-3">
      {/* The mate's name as a pill, like the model's: a click edits its profile. */}
      <button
        type="button"
        aria-label={`${mate.name}: edit profile`}
        onClick={() => setPanelOpen(true)}
        className="no-drag group flex h-8 min-w-0 items-center gap-1.5 rounded-full bg-card pr-2.5 pl-1 shadow-[0_0_0_1px_var(--border)] transition-colors hover:bg-accent"
      >
        <MateFace avatar={mate.avatar} size={24} mood={mood} live plate={false} label={mate.name} />
        <span className="truncate text-[13px] font-semibold text-foreground">{mate.name}</span>
        {word && <span className="truncate text-xs text-muted-foreground">{word}</span>}
        <PencilIcon className="size-3.5 shrink-0 text-muted-foreground transition-colors group-hover:text-foreground" />
      </button>
      <div className="flex-1" />
      <div className="flex items-center gap-1">
        {modelPill}
        <Tooltip>
          <TooltipTrigger
            render={
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={panelOpen ? "Close profile" : "Open profile"}
                onClick={() => setPanelOpen(!panelOpen)}
                className="rounded-full text-muted-foreground"
              />
            }
          >
            <PanelRightIcon />
          </TooltipTrigger>
          <TooltipContent>{panelOpen ? "Close profile" : "Profile"}</TooltipContent>
        </Tooltip>
      </div>
    </div>
  );
}

/** The colours a mate can take (ux.md 6): one hue each, drawn per theme. */
const HUES = [150, 210, 0, 30, 270, 190, 330, 45, 175, 12, 240, 95];

const MODES: { id: ApprovalMode; label: string; hint: string }[] = [
  {
    id: "ask",
    label: "Ask every time",
    hint: "Asks before edits and commands.",
  },
  {
    id: "auto-edits",
    label: "Approve edits",
    hint: "Edits files on its own; asks before commands.",
  },
  { id: "auto", label: "Approve for me", hint: "Acts without asking." },
];

const SENDING: { id: Mate["sending"]; label: string; hint: string }[] = [
  {
    id: "ask",
    label: "Ask every time",
    hint: "Asks before it sends, posts or publishes to other people.",
  },
  {
    id: "send",
    label: "Send without asking",
    hint: "Sends on its own, under its approvals.",
  },
];

/** "mcp__gmail__search" as "gmail · search". */
function toolLabel(name: string): string {
  const parts = name.split("__").filter(Boolean);
  return parts.length >= 3 ? `${parts[1]} · ${parts.slice(2).join("__")}` : name;
}

/** Tools the mate runs without asking, each removable (FR-41). */
function AlwaysAllowed({ mate }: { mate: Mate }) {
  const [tools, setTools] = React.useState<string[]>([]);
  // The mates were read again (`mates://changed`, a new rule among them): read again.
  const mates = useMatesStore((s) => s.mates);
  React.useEffect(() => {
    let gone = false;
    void matesApi
      .grants(mate.id)
      .then((list) => !gone && setTools(list))
      .catch(() => undefined);
    return () => {
      gone = true;
    };
  }, [mate.id, mates]);
  if (tools.length === 0) return null;
  return (
    <div className="flex flex-col gap-1.5">
      <span className="text-xs font-medium text-muted-foreground">Always allowed</span>
      <ul className="flex flex-col gap-1">
        {tools.map((tool) => (
          <li key={tool} className="flex items-center gap-2 rounded-2xl bg-muted/40 px-3 py-1.5">
            <span className="min-w-0 flex-1 truncate text-[13px] text-foreground" title={tool}>
              {toolLabel(tool)}
            </span>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => {
                setTools((current) => current.filter((t) => t !== tool));
                void matesApi.removeGrant(mate.id, tool).catch(() => undefined);
              }}
            >
              Remove
            </Button>
          </li>
        ))}
      </ul>
    </div>
  );
}

/** A mate's saved secrets, by name only, each deletable (FR-43). */
function Secrets({ mate }: { mate: Mate }) {
  const [names, setNames] = React.useState<string[]>([]);
  // A card answered in the conversation saves one: read again when the mate's turn ends.
  const busy = useEngineStore((s) => s.activeId === mate.chatId && s.transcript.busy);
  React.useEffect(() => {
    let gone = false;
    void matesApi
      .secrets(mate.id)
      .then((list) => !gone && setNames(list))
      .catch(() => undefined);
    return () => {
      gone = true;
    };
  }, [mate.id, busy]);
  return (
    <section className="flex flex-col gap-1.5">
      <h3 className="text-xs font-medium text-muted-foreground">Secrets</h3>
      {names.length === 0 ? (
        <p className="text-xs text-muted-foreground">
          None. When {mate.name} needs a key or a token, it asks with a secure card; the value stays on this computer.
        </p>
      ) : (
        <ul className="flex flex-col gap-1">
          {names.map((name) => (
            <li key={name} className="flex items-center gap-2 rounded-2xl bg-muted/40 px-3 py-1.5">
              <span className="min-w-0 flex-1 truncate font-mono text-xs text-foreground">{name}</span>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => {
                  setNames((current) => current.filter((n) => n !== name));
                  void matesApi.deleteSecret(mate.id, name).catch(() => undefined);
                }}
              >
                Delete
              </Button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function Field({ label, children, hint }: { label: string; children: React.ReactNode; hint?: string }) {
  return (
    <label className="flex flex-col gap-1.5">
      <span className="text-xs font-medium text-muted-foreground">{label}</span>
      {children}
      {hint && <span className="text-xs text-muted-foreground">{hint}</span>}
    </label>
  );
}

function About({ mate }: { mate: Mate }) {
  const update = useMatesStore((s) => s.update);
  const [name, setName] = React.useState(mate.name);
  const [role, setRole] = React.useState(mate.role);
  const [instructions, setInstructions] = React.useState(mate.instructions);
  React.useEffect(() => {
    setName(mate.name);
    setRole(mate.role);
    setInstructions(mate.instructions);
  }, [mate.id, mate.name, mate.role, mate.instructions]);
  const dirty = name !== mate.name || role !== mate.role || instructions !== mate.instructions;
  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col items-center gap-2 rounded-[20px] bg-muted/50 px-4 pt-5 pb-4">
        <MateFace avatar={mate.avatar} size={88} live plate={false} label={mate.name} />
        <div className="max-w-full truncate text-[15px] font-semibold text-foreground">{mate.name}</div>
        {mate.role && <div className="-mt-1.5 line-clamp-2 text-center text-xs text-muted-foreground">{mate.role}</div>}
      </div>
      <section className="flex flex-col gap-2">
        <span className="text-xs font-medium text-muted-foreground">Face</span>
        <div className="grid grid-cols-4 gap-2" role="radiogroup" aria-label="Face">
          {faces.map((face) => (
            <button
              key={face.id}
              type="button"
              role="radio"
              aria-checked={mate.avatar.definition === face.id}
              aria-label={face.name}
              onClick={() =>
                void update(mate.id, {
                  avatar: { ...mate.avatar, definition: face.id },
                })
              }
              className={cn(
                "flex aspect-square items-center justify-center rounded-2xl bg-muted/40 transition-all hover:bg-muted/80",
                mate.avatar.definition === face.id && "bg-brand/10 shadow-[0_0_0_1.5px_var(--brand)] hover:bg-brand/10",
              )}
            >
              <MateFace avatar={{ ...mate.avatar, definition: face.id }} size={36} plate={false} />
            </button>
          ))}
        </div>
      </section>
      <section className="flex flex-col gap-2">
        <span className="text-xs font-medium text-muted-foreground">Colour</span>
        <div className="grid grid-cols-8 justify-items-center gap-y-2.5" role="radiogroup" aria-label="Colour">
          {HUES.map((hue) => (
            <button
              key={hue}
              type="button"
              role="radio"
              aria-checked={(mate.avatar.hue ?? 210) === hue}
              aria-label={`Colour ${hue}`}
              onClick={() => void update(mate.id, { avatar: { ...mate.avatar, hue } })}
              className={cn(
                "size-6 rounded-full ring-offset-2 ring-offset-background transition-transform hover:scale-110",
                (mate.avatar.hue ?? 210) === hue && "ring-2 ring-brand",
              )}
              style={{ background: faceColors(hue, false).body }}
            />
          ))}
        </div>
      </section>
      <Field label="Name">
        <Input
          value={name}
          onChange={(event) => setName(event.target.value)}
          className="h-9 rounded-2xl border-transparent bg-muted/50 shadow-none focus-visible:bg-background"
        />
      </Field>
      <Field label="Role" hint="One line: what this mate does.">
        <Input
          value={role}
          onChange={(event) => setRole(event.target.value)}
          className="h-9 rounded-2xl border-transparent bg-muted/50 shadow-none focus-visible:bg-background"
        />
      </Field>
      <Field label="Instructions" hint="How this mate works. Applies from its next reply.">
        <Textarea
          value={instructions}
          onChange={(event) => setInstructions(event.target.value)}
          rows={8}
          className="rounded-2xl border-transparent bg-muted/50 shadow-none focus-visible:bg-background"
        />
      </Field>
      <div className="flex justify-end">
        <Button
          size="sm"
          className="rounded-full bg-brand px-4 text-brand-foreground hover:bg-brand/90"
          disabled={!dirty || !name.trim()}
          onClick={() => void update(mate.id, { name: name.trim(), role, instructions })}
        >
          Save
        </Button>
      </div>
    </div>
  );
}

function Settings({ mate }: { mate: Mate }) {
  const update = useMatesStore((s) => s.update);
  const model = useSessionStore((s) => s.sessions.find((c) => c.id === mate.chatId)?.model ?? null);
  return (
    <div className="flex flex-col gap-5">
      <Field label="Model" hint="Pick another in the message box's model menu; the mate keeps it.">
        <div className="truncate rounded-2xl bg-muted/50 px-3 py-2 text-[13px] text-foreground">
          {model ?? "The app's default model"}
        </div>
      </Field>
      <div className="flex flex-col gap-1.5" role="radiogroup" aria-label="Approvals">
        <span className="text-xs font-medium text-muted-foreground">Approvals</span>
        {MODES.map((mode) => (
          <button
            key={mode.id}
            type="button"
            role="radio"
            aria-checked={mate.approvalMode === mode.id}
            onClick={() => void update(mate.id, { approvalMode: mode.id })}
            className={cn(
              "flex flex-col items-start rounded-2xl bg-muted/40 px-3.5 py-2.5 text-left transition-colors hover:bg-muted/80",
              mate.approvalMode === mode.id && "bg-brand/5 shadow-[0_0_0_1.5px_var(--brand)]",
            )}
          >
            <span className="text-[13px] font-medium text-foreground">{mode.label}</span>
            <span className="text-xs text-muted-foreground">{mode.hint}</span>
          </button>
        ))}
      </div>
      <div className="flex flex-col gap-1.5" role="radiogroup" aria-label="Sending on your behalf">
        <span className="text-xs font-medium text-muted-foreground">Sending on your behalf</span>
        {SENDING.map((choice) => (
          <button
            key={choice.id}
            type="button"
            role="radio"
            aria-checked={mate.sending === choice.id}
            onClick={() => void update(mate.id, { sending: choice.id })}
            className={cn(
              "flex flex-col items-start rounded-2xl bg-muted/40 px-3.5 py-2.5 text-left transition-colors hover:bg-muted/80",
              mate.sending === choice.id && "bg-brand/5 shadow-[0_0_0_1.5px_var(--brand)]",
            )}
          >
            <span className="text-[13px] font-medium text-foreground">{choice.label}</span>
            <span className="text-xs text-muted-foreground">{choice.hint}</span>
          </button>
        ))}
      </div>
      <AlwaysAllowed mate={mate} />
      <FollowingUp mate={mate} />
      <div className="flex items-center justify-between gap-3">
        <div>
          <div className="text-[13px] font-medium text-foreground">Notifications</div>
          <div className="text-xs text-muted-foreground">When {mate.name} needs you or finishes.</div>
        </div>
        <Switch
          checked={mate.notifications}
          onCheckedChange={(checked) => void update(mate.id, { notifications: checked })}
          aria-label="Notifications"
        />
      </div>
      <div className="flex items-center justify-between gap-3">
        <div>
          <div className="text-[13px] font-medium text-foreground">Paused</div>
          <div className="text-xs text-muted-foreground">
            {mate.name} stops its work and holds new messages until you resume it.
          </div>
        </div>
        <Switch
          checked={mate.paused}
          onCheckedChange={(checked) => void update(mate.id, { paused: checked })}
          aria-label="Paused"
        />
      </div>
    </div>
  );
}

interface Topic {
  name: string;
  description: string;
  updated: number;
}

/** The mate's own memory: what it keeps, to read and forget (FR-26). */
function Memory({ mate }: { mate: Mate }) {
  const shown = useEngineStore((s) => s.activeId === mate.chatId);
  const busy = useEngineStore((s) => s.activeId === mate.chatId && s.transcript.busy);
  const [topics, setTopics] = React.useState<Topic[] | null>(null);
  const [shared, setShared] = React.useState<Topic[]>([]);
  const [error, setError] = React.useState<string | null>(null);
  const request = React.useCallback(async (command: string, args?: Record<string, unknown>) => {
    try {
      const state = await useEngineStore.getState().request<{
        project?: { items?: Topic[] };
        personal?: { items?: Topic[] };
        usePersonal?: boolean;
      }>(command, args);
      setTopics(state?.project?.items ?? []);
      setShared(state?.usePersonal === false ? [] : (state?.personal?.items ?? []));
      setError(null);
    } catch (e) {
      setError(String(e).replace(/^Error:\s*/, ""));
    }
  }, []);
  React.useEffect(() => {
    if (shown && !busy) void request("list_memories");
  }, [shown, busy, request]);
  if (!shown)
    return <p className="text-sm text-muted-foreground">Open {mate.name}'s conversation to see its memory.</p>;
  if (error) return <p className="text-sm text-destructive">{error}</p>;
  if (!topics) return <p className="text-sm text-muted-foreground">Reading…</p>;
  return (
    <div className="flex flex-col gap-4">
      <section className="flex flex-col gap-2">
        <h3 className="text-[11px] font-medium tracking-wide text-muted-foreground uppercase">{mate.name}'s own</h3>
        {topics.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            Nothing yet. {mate.name} keeps what you tell it and what it learns in its work.
          </p>
        ) : (
          <ul className="flex flex-col gap-2">
            {topics.map((topic) => (
              <li key={topic.name} className="flex items-start gap-2 rounded-2xl bg-muted/40 px-3 py-2">
                <div className="min-w-0 flex-1">
                  <div className="truncate text-[13px] font-medium text-foreground">{topic.name}</div>
                  <div className="line-clamp-2 text-xs text-muted-foreground">{topic.description}</div>
                </div>
                <Button
                  size="sm"
                  variant="ghost"
                  className="shrink-0"
                  onClick={() =>
                    void request("delete_memory", {
                      scope: "project",
                      name: topic.name,
                    })
                  }
                >
                  Forget
                </Button>
              </li>
            ))}
          </ul>
        )}
      </section>
      {shared.length > 0 && (
        <section className="flex flex-col gap-2">
          <h3 className="text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
            Shared with every mate
          </h3>
          <p className="text-xs text-muted-foreground">
            About you, from your personal memory. Change it in Evo Harness → Memory.
          </p>
          <ul className="flex flex-col gap-1.5">
            {shared.map((topic) => (
              <li key={topic.name} className="rounded-2xl bg-muted/40 px-3 py-1.5">
                <div className="truncate text-[13px] text-foreground">{topic.name}</div>
                <div className="line-clamp-1 text-xs text-muted-foreground">{topic.description}</div>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}

/**
 * The apps a mate may use (FR-39): every connector Evo has, switched per mate
 * on its chat, as Harness switches them per chat (`connectors.chatOff`).
 */
function Apps({ mate }: { mate: Mate }) {
  const servers = useConnectorsStore((s) => s.servers);
  const loaded = useConnectorsStore((s) => s.loaded);
  const off = useUiStore((s) => s.chatConnectorsOff[mate.chatId]) ?? [];
  React.useEffect(() => {
    if (!loaded) void useConnectorsStore.getState().load();
  }, [loaded]);
  if (!loaded) return <p className="text-sm text-muted-foreground">Reading…</p>;
  if (servers.length === 0)
    return (
      <p className="text-sm text-muted-foreground">
        No apps connected yet. Connect them in Evo Harness → Customize → Connectors; then choose here which ones{" "}
        {mate.name} may use.
      </p>
    );
  const allOn = servers.every((server) => !off.includes(server.key));
  return (
    <div className="flex flex-col gap-3">
      <p className="text-xs text-muted-foreground">
        {allOn ? `${mate.name} may use every connected app.` : `${mate.name} may use only the apps switched on.`}
      </p>
      <ul className="flex flex-col gap-1.5">
        {servers.map((server) => (
          <li key={server.key} className="flex items-center gap-2 rounded-2xl bg-muted/40 px-3 py-2">
            <div className="min-w-0 flex-1">
              <div className="truncate text-[13px] font-medium text-foreground">
                {server.app ? connectorLabel(server) : (server.server?.title ?? server.name)}
              </div>
              <div className="truncate text-xs text-muted-foreground">
                {server.status === "connected" ? `${server.tools.length} tools` : server.status}
              </div>
            </div>
            <Switch
              checked={!off.includes(server.key)}
              onCheckedChange={(on) => void useEngineStore.getState().setChatConnector(mate.chatId, server.key, on)}
              aria-label={server.name}
            />
          </li>
        ))}
      </ul>
    </div>
  );
}

function Files({ mate }: { mate: Mate }) {
  const update = useMatesStore((s) => s.update);
  const folder = mate.projectFolder || mate.folder;
  const choose = async () => {
    const picked = await pickFolder({
      directory: true,
      multiple: false,
      title: `A project folder for ${mate.name}`,
    });
    if (typeof picked === "string") await update(mate.id, { projectFolder: picked });
  };
  return (
    <div className="flex flex-col gap-4">
      <Field label="Works in" hint={mate.projectFolder ? "A project folder." : `${mate.name}'s own folder.`}>
        <div className="truncate rounded-2xl bg-muted/50 px-3 py-2 font-mono text-xs text-foreground" title={folder}>
          {folder}
        </div>
      </Field>
      <div className="flex flex-wrap gap-2">
        <Button size="sm" variant="secondary" className="rounded-full" onClick={() => void openPath(folder)}>
          <FolderOpenIcon />
          Open folder
        </Button>
        <Button size="sm" variant="secondary" className="rounded-full" onClick={() => void choose()}>
          Use a project folder…
        </Button>
        {mate.projectFolder && (
          <Button size="sm" variant="ghost" onClick={() => void update(mate.id, { projectFolder: null })}>
            Use its own folder
          </Button>
        )}
      </div>
    </div>
  );
}

export function MatePanel({ mate }: { mate: Mate }) {
  const setPanelOpen = useMatesStore((s) => s.setPanelOpen);
  const panelTab = useMatesStore((s) => s.panelTab);
  const setPanelTab = useMatesStore((s) => s.setPanelTab);
  const duplicate = useMatesStore((s) => s.duplicate);
  const remove = useMatesStore((s) => s.remove);
  const [confirm, setConfirm] = React.useState(false);
  return (
    // A soft card beside the conversation, not a ruled-off column (ux.md 2).
    <aside className="flex h-full w-[332px] shrink-0 p-2 pl-0" aria-label={`${mate.name}'s profile`}>
      <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden rounded-[22px] bg-card shadow-[0_0_0_1px_color-mix(in_srgb,var(--border)_70%,transparent),0_10px_30px_-18px_rgb(0_0_0/0.25)]">
        <div className="flex h-12 items-center justify-center px-4">
          <div className="text-[13px] font-semibold text-foreground">Mate profile</div>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label="Close profile"
            className="absolute top-2.5 right-2.5 rounded-full"
            onClick={() => setPanelOpen(false)}
          >
            <XIcon />
          </Button>
        </div>
        <Tabs value={panelTab} onValueChange={(tab) => setPanelTab(tab as MatePanelTab)} className="min-h-0 flex-1">
          <TabsList className="mx-3 h-9 rounded-full bg-muted/70 p-1">
            {(["about", "settings", "apps", "memory", "files"] as const).map((tab) => (
              <TabsTrigger
                key={tab}
                value={tab}
                className="rounded-full px-2.5 text-[12.5px] data-active:shadow-[0_1px_2px_rgb(0_0_0/0.08)]"
              >
                {tab.charAt(0).toUpperCase() + tab.slice(1)}
              </TabsTrigger>
            ))}
          </TabsList>
          <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
            <TabsContent value="about">
              <About mate={mate} />
            </TabsContent>
            <TabsContent value="settings">
              <Settings mate={mate} />
            </TabsContent>
            <TabsContent value="apps" className="flex flex-col gap-6">
              <Apps mate={mate} />
              <MateConnectedApps mate={mate} />
              <MateSkills mate={mate} />
              <Secrets mate={mate} />
            </TabsContent>
            <TabsContent value="memory">
              <Memory mate={mate} />
            </TabsContent>
            <TabsContent value="files">
              <Files mate={mate} />
            </TabsContent>
          </div>
        </Tabs>
        <div className="flex gap-2 px-3 pt-1 pb-3">
          <Button size="sm" variant="secondary" className="rounded-full px-3.5" onClick={() => void duplicate(mate.id)}>
            Duplicate
          </Button>
          <Button
            size="sm"
            variant="ghost"
            className="ml-auto rounded-full px-3.5 text-destructive hover:bg-destructive/10 hover:text-destructive"
            onClick={() => setConfirm(true)}
          >
            Delete…
          </Button>
        </div>
      </div>
      <ConfirmDelete
        open={confirm}
        onOpenChange={setConfirm}
        title={`Delete ${mate.name}?`}
        description={`${mate.name}'s conversation, memory and folder are deleted too, and ${mate.name} leaves every channel. This cannot be undone.`}
        onConfirm={() => void remove(mate.id)}
      />
    </aside>
  );
}
