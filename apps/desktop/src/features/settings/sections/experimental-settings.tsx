import * as React from "react";
import { FlaskConicalIcon } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { db, log } from "@/lib/db";
import { inTauri } from "@/lib/platform";
import { SettingsGroup, SettingsHeader, SettingsRow } from "../settings-controls";

// Settings → Experimental: features that may change or go away
// (docs/features/code-mode/spec.md, FR-1, FR-2). Saved in the app settings;
// the shell starts every chat's engine with them and passes a change to the
// running engines (src-tauri/src/engine.rs, src-tauri/src/supervisor/settings.rs).

/** Code mode: the agent may run a JavaScript program that calls its tools. */
const CODE_MODE = "experimental.codeMode";

export function ExperimentalSettings() {
  const [codeMode, setCodeMode] = React.useState(false);

  React.useEffect(() => {
    if (!inTauri) return;
    void db
      .settings()
      .then((all) => setCodeMode(all[CODE_MODE] === true))
      .catch((error) => log("warn", "ui", `could not read the experimental settings: ${String(error)}`));
  }, []);

  const saveCodeMode = (on: boolean) => {
    setCodeMode(on);
    void db.setSetting(CODE_MODE, on).catch((error) => {
      setCodeMode(!on);
      log("error", "ui", `could not save code mode: ${String(error)}`);
    });
  };

  return (
    <div className="flex flex-col gap-8">
      <SettingsHeader
        icon={FlaskConicalIcon}
        title="Experimental"
        description="Features that are still being tried out. They may change or be removed in a later version."
      />
      <SettingsGroup title="Agent">
        <SettingsRow
          labelId="experimental-code-mode"
          label="Code mode"
          description="The agent can write a short program that calls its tools, runs them in parallel and returns only what it needs. Each call still asks for approval as your approval mode says, and hooks and the sandbox still apply. Applies from the next message in every chat."
          control={<Switch aria-labelledby="experimental-code-mode" checked={codeMode} onCheckedChange={saveCodeMode} />}
        />
      </SettingsGroup>
    </div>
  );
}
