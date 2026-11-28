// A mate's header pill and panel (docs/features/evo-mates/ux.md 2; spec
// FR-7, FR-10, FR-13, FR-25, FR-29). The header floats over the
// conversation: face, name, role or state word, the panel toggle. The panel
// holds the profile: About, Settings and Files; changes apply from the
// mate's next turn.

import { isQuiet, useMatesPrefs } from "@/stores/mates-prefs-store";
import * as React from "react";
import { open as pickFolder } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { BellIcon, BellOffIcon, FolderOpenIcon, MessageCircleIcon, PauseIcon, PinIcon, PinOffIcon, PlayIcon, XIcon } from "lucide-react";
import { useTheme } from "@/components/theme-provider";
import { faceColors, MateFace } from "@/components/mate-face/mate-face";
import { faces } from "@/components/mate-face/faces";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { matesApi, type ApprovalMode, type Mate } from "@/lib/mates";
import { cn } from "@/lib/utils";
import { useEngineStore } from "@/stores/engine-store";
import { useMatesStore, type MatePanelTab } from "@/stores/mates-store";
import { connectorLabel } from "@/lib/mcp";
import { useConnectorsStore } from "@/stores/connectors-store";
import { useSessionStore } from "@/stores/session-store";
import { useUiStore } from "@/stores/ui-store";
import { MemoryHistory, TidyNow, type MemoryChange } from "@/features/memory";
import { MateLearning } from "./mate-learning";
import { MateConnectedApps } from "@/features/connected-apps";
import { MateOverview } from "./mate-overview";
import { MatePresenceFace, usePresence } from "./presence-face";
import { MateMenuButton } from "./roster";
import { panelFloats } from "./bubble-style";
import { FollowingUp } from "./following-up";
import { MateSkills } from "./skills-tab";
import { Knowledge } from "./knowledge";

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

function Field({ label, children, hint }: { label: React.ReactNode; children: React.ReactNode; hint?: string }) {
  return (
    <label className="flex flex-col gap-1.5">
      <span className="text-xs font-medium text-muted-foreground">{label}</span>
      {children}
      {hint && <span className="text-xs text-muted-foreground">{hint}</span>}
    </label>
  );
}

/** A field that saves when the person leaves it, and says so (FR-24; QA A6). */
function useSaved() {
  const [saved, setSaved] = React.useState<string | null>(null);
  React.useEffect(() => {
    if (!saved) return;
    const timer = window.setTimeout(() => setSaved(null), 1600);
    return () => window.clearTimeout(timer);
  }, [saved]);
  return { saved, mark: setSaved };
}

function SavedMark({ show }: { show: boolean }) {
  return (
    <span aria-live="polite" className={cn("text-[11px] font-medium text-brand transition-opacity", show ? "opacity-100" : "opacity-0")}>
      Saved
    </span>
  );
}

function About({ mate }: { mate: Mate }) {
  const update = useMatesStore((s) => s.update);
  const [name, setName] = React.useState(mate.name);
  const [role, setRole] = React.useState(mate.role);
  const [instructions, setInstructions] = React.useState(mate.instructions);
  const { saved, mark } = useSaved();
  React.useEffect(() => {
    setName(mate.name);
    setRole(mate.role);
    setInstructions(mate.instructions);
  }, [mate.id, mate.name, mate.role, mate.instructions]);
  const save = (field: "name" | "role" | "instructions") => {
    if (field === "name") {
      if (!name.trim()) return; // An empty name is refused; the note says so.
      if (name.trim() === mate.name) return;
      void update(mate.id, { name: name.trim() }).then(() => mark("name"));
    } else if (field === "role") {
      if (role === mate.role) return;
      void update(mate.id, { role }).then(() => mark("role"));
    } else {
      if (instructions === mate.instructions) return;
      void update(mate.id, { instructions }).then(() => mark("instructions"));
    }
  };
  const enter = (field: "name" | "role") => (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      save(field);
      event.currentTarget.blur();
    }
  };
  const label = (text: string, field: string) => (
    <span className="flex items-center justify-between">
      {text}
      <SavedMark show={saved === field} />
    </span>
  );
  return (
    <div className="flex flex-col gap-5">
      <Field label={label("Name", "name")} hint={!name.trim() ? "A mate needs a name." : undefined}>
        <Input
          value={name}
          onChange={(event) => setName(event.target.value)}
          onBlur={() => save("name")}
          onKeyDown={enter("name")}
          aria-invalid={!name.trim()}
          className="h-9 rounded-2xl border-transparent bg-muted/50 shadow-none focus-visible:bg-background"
        />
      </Field>
      <Field label={label("Role", "role")} hint="One line: what this mate does.">
        <Input
          value={role}
          onChange={(event) => setRole(event.target.value)}
          onBlur={() => save("role")}
          onKeyDown={enter("role")}
          className="h-9 rounded-2xl border-transparent bg-muted/50 shadow-none focus-visible:bg-background"
        />
      </Field>
      <Field label={label("Instructions", "instructions")} hint="How this mate works. Saved when you leave the field; applies from its next reply.">
        <Textarea
          value={instructions}
          onChange={(event) => setInstructions(event.target.value)}
          onBlur={() => save("instructions")}
          rows={8}
          className="rounded-2xl border-transparent bg-muted/50 shadow-none focus-visible:bg-background"
        />
      </Field>
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
              onClick={() => void update(mate.id, { avatar: { ...mate.avatar, definition: face.id } })}
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
                "size-6 rounded-full ring-offset-2 ring-offset-background transition-transform motion-safe:hover:scale-110",
                (mate.avatar.hue ?? 210) === hue && "ring-2 ring-brand",
              )}
              style={{ background: faceColors(hue, false).body }}
            />
          ))}
        </div>
      </section>
    </div>
  );
}

function Settings({ mate }: { mate: Mate }) {
  const update = useMatesStore((s) => s.update);
  const model = useSessionStore((s) => s.sessions.find((c) => c.id === mate.chatId)?.model ?? null);
  // The model's name as the bar shows it, when this mate's conversation is shown.
  const modelName = useEngineStore((s) =>
    s.activeId === mate.chatId ? s.models.find((m) => m.provider === s.state.provider && m.id === (s.state.modelId ?? s.state.model))?.name : undefined,
  );
  return (
    <div className="flex flex-col gap-5">
      <Field label="Model" hint="Pick another from the model menu at the top of the conversation; the mate keeps it.">
        <div className="truncate rounded-2xl bg-muted/50 px-3 py-2 text-[13px] text-foreground">
          {modelName ?? model ?? "The app's default model"}
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
      <MateLearning mate={mate} />
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
  // Every change, with Undo (docs/features/learning-layers/spec.md, FR-1, FR-2).
  const [changes, setChanges] = React.useState<MemoryChange[]>([]);
  const history = React.useCallback(async (command = "list_memory_changes", args?: Record<string, unknown>) => {
    const next = await useEngineStore
      .getState()
      .request<{ changes?: MemoryChange[] }>(command, args)
      .catch((e: unknown) => {
        if (command !== "list_memory_changes") setError(String(e).replace(/^Error:\s*/, ""));
        return null;
      });
    if (Array.isArray(next?.changes)) setChanges(next.changes);
    return next != null;
  }, []);
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
    if (shown && !busy) {
      void request("list_memories");
      void history();
    }
  }, [shown, busy, request, history]);
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
                    }).then(() => history())
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
      <TidyNow owners={[`mate:${mate.id}`]} onDone={() => void request("list_memories").then(() => history())} />
      <MemoryHistory
        agentName={mate.name}
        changes={changes}
        onUndo={(id) => void history("undo_memory_change", { change: id }).then((done) => (done ? request("list_memories") : undefined))}
      />
    </div>
  );
}

/**
 * The apps a mate may use (FR-39): every connector Evo has, switched per mate
 * on its chat, as Harness switches them per chat (`connectors.chatOff`).
 */
/** What the mate may do at the computer (docs/features/evo-mates-computer/spec.md, FR-3). */
function AtTheComputer({ mate }: { mate: Mate }) {
  const saved = useMatesPrefs((s) => s.computer[mate.id]);
  const appControl = useMatesPrefs((s) => s.appControl);
  React.useEffect(() => {
    void useMatesPrefs.getState().load();
  }, []);
  const rows: { key: "browser" | "app" | "chrome"; label: string; hint: string; shown: boolean }[] = [
    { key: "browser", label: "Browser", hint: `${mate.name} opens and uses web pages in Evo's own browser. You can watch and take over.`, shown: true },
    { key: "chrome", label: "Your Chrome", hint: "Tabs of your own Chrome, through Evo Webbridge, with your sign-ins.", shown: true },
    { key: "app", label: "App control", hint: "Use other apps on this computer. Asks first, as set in Settings › Computer.", shown: appControl },
  ];
  return (
    <section className="flex flex-col gap-1.5">
      <h3 className="text-xs font-medium text-muted-foreground">At the computer</h3>
      <ul className="flex flex-col gap-1.5">
        {rows
          .filter((row) => row.shown)
          .map((row) => (
            <li key={row.key} className="flex items-center gap-3 rounded-2xl bg-muted/40 px-3 py-2">
              <div className="min-w-0 flex-1">
                <div className="text-[13px] font-medium text-foreground">{row.label}</div>
                <div className="text-xs text-muted-foreground">{row.hint}</div>
              </div>
              <Switch
                checked={saved?.[row.key] !== false}
                onCheckedChange={(on) => useMatesPrefs.getState().setComputer(mate.id, row.key, on)}
                aria-label={row.label}
              />
            </li>
          ))}
      </ul>
    </section>
  );
}

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
      <Knowledge mate={mate} />
    </div>
  );
}

/** A round quick action under the hero (FR-22). */
function QuickAction({ label, onClick, active, children }: { label: string; onClick(): void; active?: boolean; children: React.ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={active}
      className="group flex w-14 flex-col items-center gap-1 text-[11px] text-muted-foreground transition-colors hover:text-foreground"
    >
      <span
        className={cn(
          "flex size-9 items-center justify-center rounded-full bg-background/80 text-foreground shadow-[0_0_0_1px_var(--border)] transition-colors group-hover:bg-background [&_svg]:size-4",
          active && "bg-foreground text-background group-hover:bg-foreground/90",
        )}
      >
        {children}
      </span>
      {label}
    </button>
  );
}

const TABS: { id: MatePanelTab; label: string }[] = [
  { id: "overview", label: "Overview" },
  { id: "about", label: "Profile" },
  { id: "settings", label: "Settings" },
  { id: "apps", label: "Apps" },
  { id: "memory", label: "Memory" },
  { id: "files", label: "Files" },
];

export function MatePanel({ mate }: { mate: Mate }) {
  const setPanelOpen = useMatesStore((s) => s.setPanelOpen);
  const panelTab = useMatesStore((s) => s.panelTab);
  const setPanelTab = useMatesStore((s) => s.setPanelTab);
  const update = useMatesStore((s) => s.update);
  const { resolved } = useTheme();
  const { presence, word } = usePresence(mate);
  const muted = useMatesPrefs((s) => isQuiet(s.mute[mate.chatId]));
  // Under 1100 px the panel floats over the conversation instead of squeezing
  // it to a sliver that scrolls sideways (ux.md 2; qa-2026-11-28, H2; v2 FR-31).
  const [width, setWidth] = React.useState(() => window.innerWidth);
  React.useEffect(() => {
    const onResize = () => setWidth(window.innerWidth);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  const floats = panelFloats(width);
  // The hero's band in the mate's colour, at its plate's strength (evo-mates-ui-v2 6.2).
  const band = faceColors(mate.avatar.hue ?? 210, resolved === "dark").plate;
  return (
    // A soft card beside the conversation, not a ruled-off column (ux.md 2).
    <aside
      className={cn(
        "flex h-full w-[360px] shrink-0 p-2 pl-0",
        floats && "absolute inset-y-0 right-0 z-30 max-w-full pl-2 animate-in fade-in slide-in-from-right-4 duration-200",
      )}
      aria-label={`${mate.name}'s profile`}
    >
      <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden rounded-[22px] bg-card shadow-[0_0_0_1px_color-mix(in_srgb,var(--border)_70%,transparent),0_10px_30px_-18px_rgb(0_0_0/0.25)]">
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label="Close profile"
          className="absolute top-2.5 right-2.5 z-10 rounded-full bg-background/60 hover:bg-background"
          onClick={() => setPanelOpen(false)}
        >
          <XIcon />
        </Button>
        {/* The hero (FR-22): face, name, role, state, quick actions. */}
        <header
          className="flex shrink-0 flex-col items-center gap-2 px-4 pt-6 pb-4 text-center"
          style={{ background: `linear-gradient(to bottom, ${band}, transparent)` }}
        >
          <MatePresenceFace mate={mate} size={72} live ring="var(--card)" plate />
          <div className="flex max-w-full flex-col items-center">
            <div className="max-w-full truncate text-[17px] font-semibold tracking-tight text-foreground">{mate.name}</div>
            {mate.role && <div className="line-clamp-2 text-xs text-muted-foreground">{mate.role}</div>}
            {word && <div className={cn("mt-1 text-[11.5px] font-medium", presence === "working" ? "text-brand" : "text-foreground")}>{word}</div>}
          </div>
          <div className="mt-1 flex items-start justify-center gap-1">
            <QuickAction
              label="Message"
              onClick={() => {
                setPanelOpen(false);
                requestAnimationFrame(() => (document.querySelector("main textarea") as HTMLTextAreaElement | null)?.focus());
              }}
            >
              <MessageCircleIcon />
            </QuickAction>
            <QuickAction label={mate.paused ? "Resume" : "Pause"} active={mate.paused} onClick={() => void update(mate.id, { paused: !mate.paused })}>
              {mate.paused ? <PlayIcon /> : <PauseIcon />}
            </QuickAction>
            {/* Mute is the mute of the messaging basics: Needs you still notifies (review 1b #3). */}
            <QuickAction
              label={muted ? "Unmute" : "Mute"}
              active={muted}
              onClick={() => useMatesPrefs.getState().setMute(mate.chatId, muted ? null : { until: null })}
            >
              {muted ? <BellOffIcon /> : <BellIcon />}
            </QuickAction>
            <QuickAction label={mate.pinned ? "Unpin" : "Pin"} active={mate.pinned} onClick={() => void update(mate.id, { pinned: !mate.pinned })}>
              {mate.pinned ? <PinOffIcon /> : <PinIcon />}
            </QuickAction>
            <div className="flex w-14 flex-col items-center gap-1 text-[11px] text-muted-foreground">
              <MateMenuButton
                mate={mate}
                label={`${mate.name}: more`}
                className="size-9 bg-background/80 text-foreground shadow-[0_0_0_1px_var(--border)] hover:bg-background"
              />
              More
            </div>
          </div>
        </header>
        <Tabs value={panelTab} onValueChange={(tab) => setPanelTab(tab as MatePanelTab)} className="min-h-0 flex-1">
          <TabsList className="mx-3 grid h-9 w-[calc(100%-1.5rem)] shrink-0 grid-cols-6 rounded-full bg-muted/70 p-1">
            {TABS.map((tab) => (
              <TabsTrigger key={tab.id} value={tab.id} className="min-w-0 rounded-full px-0 text-[11.5px] data-active:shadow-[0_1px_2px_rgb(0_0_0/0.08)]">
                {tab.label}
              </TabsTrigger>
            ))}
          </TabsList>
          <div className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto px-4 py-4">
            <TabsContent value="overview">
              <MateOverview mate={mate} />
            </TabsContent>
            <TabsContent value="about">
              <About mate={mate} />
            </TabsContent>
            <TabsContent value="settings">
              <Settings mate={mate} />
            </TabsContent>
            <TabsContent value="apps" className="flex flex-col gap-6">
              <AtTheComputer mate={mate} />
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
      </div>
    </aside>
  );
}
