// What a plugin keeps on this computer (docs/features/plugin-settings/spec.md,
// FR-19): its data folder, with Open and Clear.

import * as React from "react";
import { FolderOpenIcon } from "lucide-react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { Button } from "@/components/ui/button";
import { plugins, type InstalledPlugin, type PluginData } from "@/lib/plugins";
import { Properties } from "../customize-parts";
import { bytes, ErrorNote, Section, useBusy } from "./parts";

export function DataTab({ plugin }: { plugin: InstalledPlugin }) {
  const [data, setData] = React.useState<PluginData | null>(null);
  const [confirm, setConfirm] = React.useState(false);
  const { busy, error, run } = useBusy();
  const load = React.useCallback(() => void plugins.data(plugin.id).then(setData).catch(() => setData(null)), [plugin.id]);
  React.useEffect(load, [load]);
  return (
    <div className="flex flex-col gap-5">
      <Section
        title="On this computer"
        aside={
          data && (
            <Button size="xs" variant="ghost" onClick={() => void revealItemInDir(data.path).catch(() => undefined)}>
              <FolderOpenIcon /> Open folder
            </Button>
          )
        }
      >
        <div className="@container">
          <Properties
            items={[
              ["Data folder", data ? bytes(data.bytes) : "…"],
              ["Its own storage", data ? bytes(data.storageBytes) : "…"],
              ["Secrets kept", data ? String(data.secrets) : "…"],
              ["Accounts signed in", data ? String(data.accounts) : "…"],
            ]}
          />
        </div>
        <p className="text-[11px] text-muted-foreground">
          Its data stays across updates. Uninstalling removes it, with its secrets and accounts.
        </p>
      </Section>
      <div className="flex items-center gap-2">
        <span className="flex-1 text-xs text-muted-foreground">Clear data empties its folder: its storage, caches and environment. Settings, secrets and accounts stay.</span>
        {confirm ? (
          <Button
            size="sm"
            variant="destructive"
            onBlur={() => setConfirm(false)}
            disabled={busy !== null}
            onClick={() =>
              void run("clear", async () => {
                await plugins.clearData(plugin.id);
                setConfirm(false);
                load();
              })
            }
          >
            Clear its data
          </Button>
        ) : (
          <Button size="sm" variant="outline" onClick={() => setConfirm(true)}>
            Clear data
          </Button>
        )}
      </div>
      <ErrorNote error={error} />
    </div>
  );
}
