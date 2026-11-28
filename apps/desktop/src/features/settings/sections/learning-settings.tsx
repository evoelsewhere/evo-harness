import * as React from "react";
import { Switch } from "@/components/ui/switch";
import { Slider } from "@/components/ui/slider";
import { useLearningStore, type LearningApp } from "@/stores/learning-store";
import { SettingsGroup, SettingsRow } from "../settings-controls";

// Memory upkeep and skill learning for one app (Settings › Memory in Evo
// Harness, Settings › Evo Mates in Evo Mates), the review's step count, and
// in Evo Mates, Learn from channels.
// Spec: docs/features/learning-layers/spec.md (FR-22, FR-23, NFR-3).

/** Where the steps' text goes, said next to each switch (NFR-3). */
const SENT = "What it reads is sent only to the Background model's provider (a mate's own model when none is set).";

export function LearningSettings({ app }: { app: LearningApp }) {
  const learning = useLearningStore();
  React.useEffect(() => {
    if (!learning.loaded) void learning.load();
  }, [learning]);
  const upkeep = app === "harness" ? learning.harnessUpkeep : learning.matesUpkeep;
  const skills = app === "harness" ? learning.harnessSkills : learning.matesSkills;
  const who = app === "harness" ? "the agent" : "each mate";
  return (
    <SettingsGroup title="Learning">
      <SettingsRow
        labelId={`${app}-learning-upkeep`}
        label="Memory upkeep"
        description={`Every night after 3 am, and on Tidy now: ended facts move to an archive, repeated ones go and, with a model, an outdated fact is struck through. Then the dream reads the recent chats together, keeps patterns that show in several of them and suggests what could be set up. Every change is listed and can be undone. ${SENT}`}
        control={
          <Switch aria-labelledby={`${app}-learning-upkeep`} disabled={!learning.loaded} checked={upkeep} onCheckedChange={(on) => void learning.setSwitch(app, "upkeep", on)} />
        }
      />
      <SettingsRow
        labelId={`${app}-learning-skills`}
        label="Learn skills automatically"
        description={`After a reply that took real work, a review turns your corrections into ${who}'s learned skills${app === "mates" ? ", and after a channel job a retro gives each mate its lessons" : ""}. A "Learned" line under the reply shows the change, with Undo. Skills you installed are never changed. ${SENT}`}
        control={
          <Switch aria-labelledby={`${app}-learning-skills`} disabled={!learning.loaded} checked={skills} onCheckedChange={(on) => void learning.setSwitch(app, "skills", on)} />
        }
      />
      <SettingsRow
        labelId={`${app}-learning-steps`}
        label={`Review after ${learning.reviewSteps} tool steps`}
        description="How much work a conversation takes before its next skill review. Shared by both apps."
        control={
          <Slider
            aria-labelledby={`${app}-learning-steps`}
            className="w-40"
            min={3}
            max={100}
            step={1}
            value={learning.reviewSteps}
            onValueChange={(value) => void learning.setReviewSteps(Array.isArray(value) ? value[0] : value)}
          />
        }
      />
      {app === "mates" && (
        <SettingsRow
          labelId="mates-learn-from-channels"
          label="Learn from channels"
          description="Memory upkeep also reads the channels a mate is in: your own lines count, another mate's only for an outcome it checked or a decision you agreed to. Entries name the channel they came from."
          control={
            <Switch
              aria-labelledby="mates-learn-from-channels"
              disabled={!learning.loaded}
              checked={learning.learnFromChannels}
              onCheckedChange={(on) => void learning.setLearnFromChannels(on)}
            />
          }
        />
      )}
    </SettingsGroup>
  );
}
