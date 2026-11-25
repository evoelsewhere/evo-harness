// A mate's skill switches (docs/features/evo-mates/spec.md, FR-44): every
// skill Evo knows, as the mate's engine lists it, with a switch for this mate.
// The mate keeps a list of the skills it may use (`mates.skills`, `null` =
// every skill); its engine drops the others (the role's `skills`). A skill
// switched off in Customize is off for every chat, the mate included.

import * as React from "react";
import { Switch } from "@/components/ui/switch";
import type { Mate } from "@/lib/mates";
import { useCustomizeStore, type SkillInfo } from "@/stores/customize-store";
import { useEngineStore } from "@/stores/engine-store";
import { useMatesStore } from "@/stores/mates-store";

/**
 * The mate's skill list after switching `name` on or off: `null` (every skill)
 * becomes every other skill when one goes off, and a list holding every known
 * skill again goes back to `null`, so skills added later reach the mate too.
 */
export function nextSkills(current: string[] | null, known: string[], name: string, on: boolean): string[] | null {
  const allowed = new Set(current ?? known);
  if (on) allowed.add(name);
  else allowed.delete(name);
  if (known.every((skill) => allowed.has(skill))) return null;
  return [...allowed].sort();
}

/** Whether `skill` works for a mate with the list `allowed`. */
export function skillOn(skill: Pick<SkillInfo, "name">, allowed: string[] | null): boolean {
  return allowed === null || allowed.includes(skill.name);
}

export function MateSkills({ mate }: { mate: Mate }) {
  const shown = useEngineStore((s) => s.activeId === mate.chatId);
  // Read from the mate's engine while it runs; a parked one (restarting to
  // read a change) keeps the last list, which the switches below decide on.
  const ready = useEngineStore((s) => s.activeId === mate.chatId && s.runtimes[mate.chatId]?.status === "ready");
  const data = useCustomizeStore((s) => (s.chat === mate.chatId ? s.data : null));
  const update = useMatesStore((s) => s.update);
  React.useEffect(() => {
    if (ready) void useCustomizeStore.getState().load(true);
  }, [ready, mate.skills]);

  const skills = React.useMemo(
    () => (data?.skills ?? []).filter((s) => s.kind === "skill").sort((a, b) => a.name.localeCompare(b.name)),
    [data],
  );
  const known = skills.map((s) => s.name);
  const allowed = mate.skills;

  return (
    <section className="flex flex-col gap-1.5">
      <h3 className="text-xs font-medium text-muted-foreground">Skills</h3>
      {!shown ? (
        <p className="text-xs text-muted-foreground">Open {mate.name}'s conversation to see its skills.</p>
      ) : !data ? (
        <p className="text-xs text-muted-foreground">Reading…</p>
      ) : skills.length === 0 ? (
        <p className="text-xs text-muted-foreground">No skills yet. Add them in Evo Harness → Customize → Skills.</p>
      ) : (
        <>
          <p className="text-xs text-muted-foreground">
            {allowed === null ? `${mate.name} may use every skill.` : `${mate.name} may use only the skills switched on.`}
          </p>
          <ul className="flex flex-col gap-1">
            {skills.map((skill) => {
              const offEverywhere = skill.state === "off";
              return (
                <li key={skill.name} className="flex items-center gap-2 rounded-2xl bg-muted/40 px-3 py-2">
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-[13px] font-medium text-foreground">{skill.name}</div>
                    <div className="truncate text-xs text-muted-foreground" title={skill.description}>
                      {offEverywhere ? "Off for every chat in Customize" : skill.description || "No description"}
                    </div>
                  </div>
                  <Switch
                    checked={!offEverywhere && skillOn(skill, allowed)}
                    disabled={offEverywhere}
                    onCheckedChange={(on) => void update(mate.id, { skills: nextSkills(allowed, known, skill.name, on) })}
                    aria-label={skill.name}
                  />
                </li>
              );
            })}
          </ul>
        </>
      )}
    </section>
  );
}
