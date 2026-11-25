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
import type { ApprovalMode, Mate } from "@/lib/mates";
import { cn } from "@/lib/utils";
import { useEngineStore } from "@/stores/engine-store";
import { mateMood, moodWord, useMatesStore } from "@/stores/mates-store";
import { useSessionStore } from "@/stores/session-store";
import { ConfirmDelete } from "./confirm";

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
  const mood = mateMood(work, busy);
  const panelOpen = useMatesStore((s) => s.panelOpen);
  const setPanelOpen = useMatesStore((s) => s.setPanelOpen);
  const word = mate.paused ? "Paused" : moodWord(mood);
  return (
    <div className="flex min-w-0 flex-1 items-center gap-2 pl-4">
      <MateFace avatar={mate.avatar} size={24} mood={mood} live plate={false} label={mate.name} />
      <span className="truncate text-[13px] font-semibold text-foreground">{mate.name}</span>
      {word && <span className="truncate text-xs text-muted-foreground">{word}</span>}
      <Tooltip>
        <TooltipTrigger
          render={
            <Button
              variant="ghost"
              size="icon-xs"
              aria-label="Edit profile"
              onClick={() => setPanelOpen(true)}
              className="text-muted-foreground"
            />
          }
        >
          <PencilIcon />
        </TooltipTrigger>
        <TooltipContent>Edit profile</TooltipContent>
      </Tooltip>
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
                className="text-muted-foreground"
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
  { id: "ask", label: "Ask every time", hint: "Asks before edits and commands." },
  { id: "auto-edits", label: "Approve edits", hint: "Edits files on its own; asks before commands." },
  { id: "auto", label: "Approve for me", hint: "Acts without asking." },
];

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
      <div className="flex flex-col items-center gap-3 rounded-xl bg-muted/60 p-4">
        <MateFace avatar={mate.avatar} size={96} live plate={false} label={mate.name} />
      </div>
      <span className="text-[11px] font-medium tracking-wide text-muted-foreground uppercase">Face</span>
      <div className="grid grid-cols-4 gap-1.5" role="radiogroup" aria-label="Face">
        {faces.map((face) => (
          <button
            key={face.id}
            type="button"
            role="radio"
            aria-checked={mate.avatar.definition === face.id}
            aria-label={face.name}
            onClick={() => void update(mate.id, { avatar: { ...mate.avatar, definition: face.id } })}
            className={cn(
              "flex items-center justify-center rounded-lg bg-muted/60 p-1.5 transition-colors hover:bg-muted",
              mate.avatar.definition === face.id && "ring-2 ring-primary",
            )}
          >
            <MateFace avatar={{ ...mate.avatar, definition: face.id }} size={36} plate={false} />
          </button>
        ))}
      </div>
      <span className="text-[11px] font-medium tracking-wide text-muted-foreground uppercase">Colour</span>
      <div className="flex flex-wrap gap-2" role="radiogroup" aria-label="Colour">
        {HUES.map((hue) => (
          <button
            key={hue}
            type="button"
            role="radio"
            aria-checked={(mate.avatar.hue ?? 210) === hue}
            aria-label={`Colour ${hue}`}
            onClick={() => void update(mate.id, { avatar: { ...mate.avatar, hue } })}
            className={cn(
              "size-7 rounded-full ring-offset-2 ring-offset-background transition",
              (mate.avatar.hue ?? 210) === hue && "ring-2 ring-foreground",
            )}
            style={{ background: faceColors(hue, false).body }}
          />
        ))}
      </div>
      <Field label="Name">
        <Input value={name} onChange={(event) => setName(event.target.value)} />
      </Field>
      <Field label="Role" hint="One line: what this mate does.">
        <Input value={role} onChange={(event) => setRole(event.target.value)} />
      </Field>
      <Field label="Instructions" hint="How this mate works. Applies from its next reply.">
        <Textarea value={instructions} onChange={(event) => setInstructions(event.target.value)} rows={8} />
      </Field>
      <div className="flex justify-end">
        <Button
          size="sm"
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
        <div className="truncate rounded-lg bg-muted px-2.5 py-1.5 text-[13px] text-foreground">{model ?? "The app's default model"}</div>
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
              "flex flex-col items-start rounded-lg px-3 py-2 text-left shadow-[0_0_0_1px_var(--border)] transition-colors hover:bg-accent",
              mate.approvalMode === mode.id && "shadow-[0_0_0_1.5px_var(--primary)]",
            )}
          >
            <span className="text-[13px] font-medium text-foreground">{mode.label}</span>
            <span className="text-xs text-muted-foreground">{mode.hint}</span>
          </button>
        ))}
      </div>
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
      const state = await useEngineStore
        .getState()
        .request<{ project?: { items?: Topic[] }; personal?: { items?: Topic[] }; usePersonal?: boolean }>(command, args);
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
  if (!shown) return <p className="text-sm text-muted-foreground">Open {mate.name}'s conversation to see its memory.</p>;
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
        <li key={topic.name} className="flex items-start gap-2 rounded-lg px-2.5 py-2 shadow-[0_0_0_1px_var(--border)]">
          <div className="min-w-0 flex-1">
            <div className="truncate text-[13px] font-medium text-foreground">{topic.name}</div>
            <div className="line-clamp-2 text-xs text-muted-foreground">{topic.description}</div>
          </div>
          <Button
            size="sm"
            variant="ghost"
            className="shrink-0"
            onClick={() => void request("delete_memory", { scope: "project", name: topic.name })}
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
          <h3 className="text-[11px] font-medium tracking-wide text-muted-foreground uppercase">Shared with every mate</h3>
          <p className="text-xs text-muted-foreground">About you, from your personal memory. Change it in Evo Harness → Memory.</p>
          <ul className="flex flex-col gap-1.5">
            {shared.map((topic) => (
              <li key={topic.name} className="rounded-lg bg-muted/60 px-2.5 py-1.5">
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

function Files({ mate }: { mate: Mate }) {
  const update = useMatesStore((s) => s.update);
  const folder = mate.projectFolder || mate.folder;
  const choose = async () => {
    const picked = await pickFolder({ directory: true, multiple: false, title: `A project folder for ${mate.name}` });
    if (typeof picked === "string") await update(mate.id, { projectFolder: picked });
  };
  return (
    <div className="flex flex-col gap-4">
      <Field label="Works in" hint={mate.projectFolder ? "A project folder." : `${mate.name}'s own folder.`}>
        <div className="truncate rounded-lg bg-muted px-2.5 py-1.5 font-mono text-xs text-foreground" title={folder}>
          {folder}
        </div>
      </Field>
      <div className="flex flex-wrap gap-2">
        <Button size="sm" variant="outline" onClick={() => void openPath(folder)}>
          <FolderOpenIcon />
          Open folder
        </Button>
        <Button size="sm" variant="outline" onClick={() => void choose()}>
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
  const duplicate = useMatesStore((s) => s.duplicate);
  const remove = useMatesStore((s) => s.remove);
  const [confirm, setConfirm] = React.useState(false);
  return (
    <aside className="relative flex h-full w-[320px] shrink-0 flex-col border-l border-border bg-background" aria-label={`${mate.name}'s profile`}>
      <div className="flex h-11 items-center justify-center px-4">
        <div className="text-[13px] font-semibold text-foreground">Mate profile</div>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label="Close profile"
          className="absolute top-2 right-2"
          onClick={() => setPanelOpen(false)}
        >
          <XIcon />
        </Button>
      </div>
      <Tabs defaultValue="about" className="min-h-0 flex-1">
        <TabsList className="mx-4">
          <TabsTrigger value="about">About</TabsTrigger>
          <TabsTrigger value="settings">Settings</TabsTrigger>
          <TabsTrigger value="memory">Memory</TabsTrigger>
          <TabsTrigger value="files">Files</TabsTrigger>
        </TabsList>
        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
          <TabsContent value="about">
            <About mate={mate} />
          </TabsContent>
          <TabsContent value="settings">
            <Settings mate={mate} />
          </TabsContent>
          <TabsContent value="memory">
            <Memory mate={mate} />
          </TabsContent>
          <TabsContent value="files">
            <Files mate={mate} />
          </TabsContent>
        </div>
      </Tabs>
      <div className="flex gap-2 border-t border-border p-3">
        <Button size="sm" variant="outline" onClick={() => void duplicate(mate.id)}>
          Duplicate
        </Button>
        <Button size="sm" variant="ghost" className="ml-auto text-destructive" onClick={() => setConfirm(true)}>
          Delete…
        </Button>
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
