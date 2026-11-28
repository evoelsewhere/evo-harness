// "Meet your first mate" (docs/features/evo-mates/ux.md 5; spec FR-7, FR-8,
// FR-37): gallery roles by category with search, each card with its face,
// name, one line and Add; Start from scratch opens a conversation, no form.

import * as React from "react";
import { PlusIcon, SearchIcon } from "lucide-react";
import { MateFace } from "@/components/mate-face/mate-face";
import { Button } from "@/components/ui/button";
import { teamApi, type MateTemplate, type TeamSummary } from "@/lib/mates";
import { cn } from "@/lib/utils";
import { log } from "@/lib/db";
import { useEngineStore } from "@/stores/engine-store";
import { useMatesStore } from "@/stores/mates-store";
import { ImportTeam } from "./team-import";

const ALL = "All";
/** The tab of ready-made teams (FR-75). */
const TEAMS = "Teams";

const MODE_WORD: Record<string, string> = { "auto-edits": "Approves edits", auto: "Approves for you" };

function RoleCard({ role, busy, onAdd }: { role: MateTemplate; busy: boolean; onAdd(): void }) {
  return (
    <div className="flex flex-col gap-3 rounded-xl bg-card p-4 shadow-[0_0_0_1px_var(--border)]">
      <div className="flex items-center gap-3">
        <MateFace avatar={role.avatar} size={48} label={role.name} plate={false} />
        <div className="min-w-0">
          <div className="truncate text-sm font-semibold text-foreground">{role.name}</div>
          <div className="truncate text-xs text-muted-foreground">{role.role}</div>
        </div>
      </div>
      <p className="line-clamp-2 min-h-10 text-[13px] text-muted-foreground">{role.description}</p>
      <div className="flex items-center gap-2">
        {role.approval && MODE_WORD[role.approval] && (
          <span className="rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground">{MODE_WORD[role.approval]}</span>
        )}
        <Button size="sm" className="ml-auto" disabled={busy} onClick={onAdd}>
          Add
        </Button>
      </div>
    </div>
  );
}

/** A ready-made team: its faces, name, one line and Add team (FR-75). */
function TeamCard({ team, onAdd }: { team: TeamSummary; onAdd(): void }) {
  return (
    <div className="flex flex-col gap-3 rounded-xl bg-card p-4 shadow-[0_0_0_1px_var(--border)]">
      <div className="flex items-center -space-x-2">
        {team.mates.map((mate) => (
          <span key={mate.name} className="rounded-full bg-card p-0.5">
            <MateFace avatar={mate.avatar} size={36} label={mate.name} plate={false} />
          </span>
        ))}
      </div>
      <div className="min-w-0">
        <div className="truncate text-sm font-semibold text-foreground">#{team.name}</div>
        <p className="line-clamp-2 min-h-10 text-[13px] text-muted-foreground">{team.about}</p>
      </div>
      <div className="flex items-center gap-2">
        <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">{team.mates.map((m) => m.name).join(" · ")}</span>
        <Button size="sm" onClick={onAdd}>
          Add team
        </Button>
      </div>
    </div>
  );
}

/**
 * Describe a mate (FR-78): a mate from scratch whose first message is the
 * person's description; in its first turn it names itself from it.
 */
function Describe({ busy, onBusy }: { busy: boolean; onBusy(busy: boolean): void }) {
  const create = useMatesStore((s) => s.create);
  const [text, setText] = React.useState("");
  const go = async () => {
    const description = text.trim();
    if (!description) return;
    onBusy(true);
    const mate = await create({});
    if (mate) {
      try {
        await useEngineStore.getState().sendPrompt(description, []);
      } catch (error) {
        log("warn", "mates", `could not send the description: ${String(error)}`);
      }
    }
    onBusy(false);
  };
  return (
    <div className="flex flex-col gap-2 rounded-2xl bg-muted/50 p-3">
      <label htmlFor="describe-mate" className="px-1 text-[13px] font-medium text-foreground">
        Describe a mate
      </label>
      <div className="flex items-end gap-2">
        <textarea
          id="describe-mate"
          rows={2}
          value={text}
          onChange={(event) => setText(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              void go();
            }
          }}
          placeholder="Someone to keep my receipts and monthly spending in order…"
          className="min-h-10 flex-1 resize-none rounded-xl bg-background px-3 py-2 text-sm text-foreground shadow-[0_0_0_1px_var(--border)] outline-none placeholder:text-muted-foreground"
        />
        <Button disabled={busy || !text.trim()} onClick={() => void go()}>
          Create
        </Button>
      </div>
      <p className="px-1 text-xs text-muted-foreground">Your mate picks its name and role from this and asks only what is missing.</p>
    </div>
  );
}

export function Gallery() {
  const templates = useMatesStore((s) => s.templates);
  const hasMates = useMatesStore((s) => s.mates.length > 0);
  const create = useMatesStore((s) => s.create);
  const [category, setCategory] = React.useState(ALL);
  const [query, setQuery] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [teams, setTeams] = React.useState<TeamSummary[]>([]);
  const [adding, setAdding] = React.useState<string | null>(null);
  React.useEffect(() => {
    void teamApi
      .builtins()
      .then(setTeams)
      .catch(() => undefined);
  }, []);
  const needle = query.trim().toLowerCase();
  const categories = [ALL, ...Array.from(new Set(templates.map((t) => t.category))), ...(teams.length ? [TEAMS] : [])];
  const teamNeedle = (t: TeamSummary) =>
    !needle || [t.name, t.about, ...t.mates.map((m) => m.name)].some((text) => text.toLowerCase().includes(needle));
  const shownTeams = category === TEAMS || (category === ALL && needle) ? teams.filter(teamNeedle) : [];
  const shown = templates.filter(
    (t) =>
      (category === ALL || t.category === category) &&
      (!needle || [t.name, t.role, t.description, t.category].some((text) => text.toLowerCase().includes(needle))),
  );

  const add = async (template?: string) => {
    setBusy(true);
    await create(template ? { template } : {});
    setBusy(false);
  };

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto flex max-w-4xl flex-col gap-6 px-8 py-10">
        <header className="flex flex-col items-center gap-2 text-center">
          <img src="/mates.png" alt="" className="size-16 rounded-2xl" draggable={false} />
          <h1 className="text-xl font-semibold text-foreground">{hasMates ? "Add a mate" : "Meet your first mate"}</h1>
          <p className="max-w-md text-sm text-muted-foreground">
            Describe what you need, or pick a role or a team. Your mate will introduce itself and ask a few questions to settle in.
          </p>
        </header>
        <Describe busy={busy} onBusy={setBusy} />
        <div className="flex flex-wrap items-center gap-2">
          <div role="tablist" className="flex flex-wrap gap-1">
            {categories.map((name) => (
              <button
                key={name}
                role="tab"
                type="button"
                aria-selected={category === name}
                onClick={() => setCategory(name)}
                className={cn(
                  "rounded-full px-3 py-1 text-[13px] text-muted-foreground transition-colors hover:bg-accent",
                  category === name && "bg-accent text-foreground",
                )}
              >
                {name}
              </button>
            ))}
          </div>
          <label className="ml-auto flex h-8 w-56 items-center gap-2 rounded-lg bg-muted px-2.5 text-sm">
            <SearchIcon className="size-4 shrink-0 text-muted-foreground" />
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Search roles and teams"
              className="min-w-0 flex-1 bg-transparent outline-none placeholder:text-muted-foreground"
            />
          </label>
        </div>
        <div className="grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-3">
          {shown.map((role) => (
            <RoleCard key={role.id} role={role} busy={busy} onAdd={() => void add(role.id)} />
          ))}
        </div>
        {shownTeams.length > 0 && (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-3">
            {shownTeams.map((team) => (
              <TeamCard key={team.id} team={team} onAdd={() => setAdding(team.id)} />
            ))}
          </div>
        )}
        {shown.length === 0 && shownTeams.length === 0 && (
          <p className="text-center text-sm text-muted-foreground">{category === TEAMS ? "No team matches." : "No role matches."}</p>
        )}
        <ImportTeam open={adding !== null} onOpenChange={(open) => !open && setAdding(null)} team={adding ?? undefined} />
        <div className="flex justify-center">
          <Button variant="outline" disabled={busy} onClick={() => void add()}>
            <PlusIcon />
            Start from scratch
          </Button>
        </div>
      </div>
    </div>
  );
}
