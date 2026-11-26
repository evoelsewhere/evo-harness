import * as React from "react";
import { UsersRoundIcon } from "lucide-react";
import { Slider } from "@/components/ui/slider";
import { db, log } from "@/lib/db";
import { inTauri } from "@/lib/platform";
import { SettingsGroup, SettingsHeader, SettingsRow } from "../settings-controls";

// Settings → Evo Mates (docs/features/evo-mates/spec.md, FR-18): how many
// times mates in a channel may hand work to each other for one message
// before every mate stops. Saved in the app settings; a channel reads it
// when its role is written again (`src-tauri/src/mates/channels.rs`).

const HANDOFF_LIMIT = "mates.handoffLimit";
const DEFAULT = 6;
const MAX_RUNNING = "mates.maxRunning";
const RUNNING_DEFAULT = 4;

export function MatesSettings() {
  const [limit, setLimit] = React.useState(DEFAULT);
  const [running, setRunning] = React.useState(RUNNING_DEFAULT);

  React.useEffect(() => {
    if (!inTauri) return;
    void db
      .settings()
      .then((all) => {
        const saved = all[HANDOFF_LIMIT];
        if (typeof saved === "number" && saved >= 1 && saved <= 50) setLimit(saved);
        const max = all[MAX_RUNNING];
        if (typeof max === "number" && max >= 1 && max <= 16) setRunning(max);
      })
      .catch((error) => log("warn", "ui", `could not read the Evo Mates settings: ${String(error)}`));
  }, []);

  const save = (value: number) => {
    setLimit(value);
    void db.setSetting(HANDOFF_LIMIT, value === DEFAULT ? null : value).catch((error) => {
      log("error", "ui", `could not save the hand-off limit: ${String(error)}`);
    });
  };

  const saveRunning = (value: number) => {
    setRunning(value);
    void db.setSetting(MAX_RUNNING, value === RUNNING_DEFAULT ? null : value).catch((error) => {
      log("error", "ui", `could not save the running limit: ${String(error)}`);
    });
  };

  return (
    <div className="flex flex-col gap-8">
      <SettingsHeader
        icon={UsersRoundIcon}
        title="Evo Mates"
        description="How your mates work together in channels."
      />
      <SettingsGroup title="Mates">
        <SettingsRow
          labelId="mates-max-running"
          label={`Mates working at once: ${running}`}
          description="When more mates have work, the others wait their turn and show Queued. Your own messages never wait."
          control={
            <Slider
              aria-labelledby="mates-max-running"
              className="w-40"
              min={1}
              max={16}
              step={1}
              value={running}
              onValueChange={(value) => saveRunning(Math.round(Array.isArray(value) ? value[0] : value))}
            />
          }
        />
      </SettingsGroup>
      <SettingsGroup title="Channels">
        <SettingsRow
          labelId="mates-handoff-limit"
          label={`Hand-offs per message: ${limit}`}
          description="How many times mates may hand work to each other for one of your messages. Past it, every mate in the channel stops and asks you how to go on. Applies to a channel from the next change to it."
          control={
            <Slider
              aria-labelledby="mates-handoff-limit"
              className="w-40"
              min={1}
              max={20}
              step={1}
              value={limit}
              onValueChange={(value) => save(Math.round(Array.isArray(value) ? value[0] : value))}
            />
          }
        />
      </SettingsGroup>
    </div>
  );
}
