import * as React from "react";
import { listen } from "@tauri-apps/api/event";
import { LearnedSkillActions } from "@/components/learned-skill-actions";
import { Switch } from "@/components/ui/switch";
import { db, type LearnedSkill } from "@/lib/db";
import { formatRelative } from "@/lib/format";
import type { Mate } from "@/lib/mates";

// The skills a mate learned itself, kept apart from the skills you
// installed: a switch, Undo last change and Delete for each
// (docs/features/learning-layers/spec.md, FR-15).

export function MateLearnedSkills({ mate }: { mate: Mate }) {
  const owner = `mate:${mate.id}`;
  const [skills, setSkills] = React.useState<LearnedSkill[] | null>(null);
  const load = React.useCallback(() => {
    void db
      .learnedSkills(owner)
      .then(setSkills)
      .catch(() => setSkills([]));
  }, [owner]);
  React.useEffect(() => {
    load();
    const stop = listen("learning://changed", load);
    return () => void stop.then((off) => off());
  }, [load]);
  if (!skills || skills.length === 0) return null;
  return (
    <section className="flex flex-col gap-1.5">
      <h3 className="text-xs font-medium text-muted-foreground">Learned by {mate.name}</h3>
      <p className="text-xs text-muted-foreground">Written by {mate.name} from your corrections. Skills you installed are never changed.</p>
      {skills.map((skill) => (
        <div key={skill.name} className="flex flex-col gap-1 rounded-2xl bg-muted/40 px-3 py-2">
          <div className="flex items-center gap-2">
            <div className="min-w-0 flex-1">
              <div className="truncate text-[13px] font-medium text-foreground">{skill.name}</div>
              <div className="line-clamp-2 text-xs text-muted-foreground">{skill.description}</div>
              {skill.changedAt && (
                <div className="text-[11px] text-muted-foreground">Changed {formatRelative(skill.changedAt)}</div>
              )}
            </div>
            <Switch
              checked={skill.on}
              aria-label={`Use ${skill.name}`}
              onCheckedChange={(on) => void db.learnedSkillSetOn(owner, skill.name, on).then(load)}
            />
          </div>
          <LearnedSkillActions owner={owner} name={skill.name} canUndo={skill.lastChange != null} onDone={load} />
        </div>
      ))}
    </section>
  );
}
