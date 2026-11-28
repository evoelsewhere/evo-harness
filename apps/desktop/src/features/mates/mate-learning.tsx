import * as React from "react";
import { Switch } from "@/components/ui/switch";
import type { Mate } from "@/lib/mates";
import { useLearningStore } from "@/stores/learning-store";

// A mate's own learning switches: its memory upkeep and its skill learning
// can be off whatever the app's switches say.
// Spec: docs/features/learning-layers/spec.md (FR-25).

export function MateLearning({ mate }: { mate: Mate }) {
  const learning = useLearningStore();
  React.useEffect(() => {
    if (!learning.loaded) void learning.load();
  }, [learning]);
  const own = learning.mates[mate.id] ?? { upkeep: true, skills: true };
  const rows = [
    {
      which: "upkeep" as const,
      label: "Memory upkeep",
      hint: `Tidy ${mate.name}'s memory each night and learn from its recent conversations.`,
      app: learning.matesUpkeep,
    },
    {
      which: "skills" as const,
      label: "Learn skills",
      hint: `Turn your corrections into ${mate.name}'s learned skills.`,
      app: learning.matesSkills,
    },
  ];
  return (
    <>
      {rows.map((row) => (
        <div key={row.which} className="flex items-center justify-between gap-3">
          <div>
            <div className="text-[13px] font-medium text-foreground">{row.label}</div>
            <div className="text-xs text-muted-foreground">
              {row.app ? row.hint : `${row.hint} Off for every mate in Settings › Evo Mates.`}
            </div>
          </div>
          <Switch
            checked={row.app && own[row.which]}
            disabled={!learning.loaded || !row.app}
            onCheckedChange={(on) => void learning.setMate(mate.id, row.which, on)}
            aria-label={row.label}
          />
        </div>
      ))}
    </>
  );
}
