// A mate's header pill and panel (docs/features/evo-mates/ux.md 2; spec
// FR-7, FR-10, FR-13, FR-25, FR-29). The header floats over the
// conversation: face, name, role or state word, the panel toggle. The panel
// holds the profile: About, Settings and Files; changes apply from the
// mate's next turn.

import * as React from "react";
import { open as pickFolder } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { FolderOpenIcon, PanelRightIcon, XIcon } from "lucide-react";
import { MateFace } from "@/components/mate-face/mate-face";
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

export function useShownMate(): Mate | null {
  return useMatesStore((s) => {
    const view = s.view;
    return view.kind === "mate" ? (s.mates.find((m) => m.id === view.id) ?? null) : null;
  });
}

export function MateHeader({ mate }: { mate: Mate }) {
  const work = useSessionStore((s) => s.work[mate.chatId]);
  const busy = useEngineStore((s) => s.activeId === mate.chatId && s.transcript.busy);
  const mood = mateMood(work, busy);
  const panelOpen = useMatesStore((s) => s.panelOpen);
  const setPanelOpen = useMatesStore((s) => s.setPanelOpen);
  const word = moodWord(mood);
  return (
    <div className="pointer-events-none absolute inset-x-0 top-2 z-10 flex justify-center px-4">
      <div className="pointer-events-auto flex max-w-full items-center gap-2.5 rounded-full bg-background/90 py-1 pr-1 pl-1 shadow-[0_0_0_1px_var(--border),0_4px_16px_rgb(0_0_0/0.06)] backdrop-blur">
        <MateFace avatar={mate.avatar} size={32} mood={mood} live label={mate.name} />
        <div className="min-w-0 leading-tight">
          <div className="truncate text-[13px] font-semibold text-foreground">{mate.name}</div>
          <div className={cn("truncate text-xs text-muted-foreground", word && "text-foreground")}>
            {mate.paused ? "Paused" : (word ?? (mate.role || "Mate"))}
          </div>
        </div>
        <Tooltip>
          <TooltipTrigger
            render={
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={panelOpen ? "Close profile" : "Open profile"}
                onClick={() => setPanelOpen(!panelOpen)}
                className="ml-2 rounded-full"
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
      <div className="flex flex-wrap gap-1.5" role="radiogroup" aria-label="Face">
        {faces.map((face) => (
          <button
            key={face.id}
            type="button"
            role="radio"
            aria-checked={mate.avatar.definition === face.id}
            aria-label={face.name}
            onClick={() => void update(mate.id, { avatar: { ...mate.avatar, definition: face.id } })}
            className={cn(
              "rounded-full p-0.5 ring-offset-2 ring-offset-background transition",
              mate.avatar.definition === face.id && "ring-2 ring-primary",
            )}
          >
            <MateFace avatar={{ ...mate.avatar, definition: face.id }} size={34} />
          </button>
        ))}
      </div>
      <Field label="Colour">
        <input
          type="range"
          min={0}
          max={359}
          value={mate.avatar.hue ?? 210}
          onChange={(event) => void update(mate.id, { avatar: { ...mate.avatar, hue: Number(event.target.value) } })}
          className="w-full accent-[var(--primary)]"
          aria-label="Colour"
        />
      </Field>
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
  return (
    <div className="flex flex-col gap-5">
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
      <div className="flex flex-col items-center gap-2 px-4 pt-6 pb-3">
        <MateFace avatar={mate.avatar} size={72} live label={mate.name} />
        <div className="text-base font-semibold text-foreground">{mate.name}</div>
        <div className="text-center text-xs text-muted-foreground">{mate.role}</div>
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
          <TabsTrigger value="files">Files</TabsTrigger>
        </TabsList>
        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
          <TabsContent value="about">
            <About mate={mate} />
          </TabsContent>
          <TabsContent value="settings">
            <Settings mate={mate} />
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
