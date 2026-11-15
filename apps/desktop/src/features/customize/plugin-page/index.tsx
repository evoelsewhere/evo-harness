// One installed plugin's page in Customize → Plugins
// (docs/features/plugin-settings/spec.md, FR-1): its header and actions, and
// the tabs Overview · Settings · Accounts · Models · Permissions · Runtime · Data.

import * as React from "react";
import { BlocksIcon, FolderOpenIcon, RefreshCwIcon, Trash2Icon } from "lucide-react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { plugins, type InstalledPlugin } from "@/lib/plugins";
import { cn } from "@/lib/utils";
import { Badge, IconAction } from "../customize-parts";
import { askConsent } from "../plugin-consent";
import { AccountsTab } from "./accounts-tab";
import { AiTab } from "./ai-tab";
import { DataTab } from "./data-tab";
import { OverviewTab } from "./overview";
import { ErrorNote, useBusy } from "./parts";
import { PermissionsTab } from "./permissions";
import { RuntimeTab } from "./runtime-tab";
import { SettingsForm } from "./settings-form";

/**
 * Turn a plugin on or off. Turning on one that asks for permissions not yet
 * approved asks for them instead (plugin-ui FR-3).
 */
export async function setPluginEnabled(plugin: InstalledPlugin, on: boolean): Promise<void> {
  if (on && plugin.unapproved.length > 0) {
    askConsent({ id: plugin.id, needsConsent: plugin.unapproved, pendingVersion: plugin.pending?.version }, plugin.name);
    return;
  }
  await plugins.setEnabled(plugin.id, on);
}

type Tab = "overview" | "settings" | "accounts" | "ai" | "permissions" | "runtime" | "data";

export function PluginPage({ plugin }: { plugin: InstalledPlugin }) {
  const { busy, error, run } = useBusy();
  const [confirm, setConfirm] = React.useState(false);
  const [tab, setTab] = React.useState<Tab>(plugin.waiting.some((w) => w.part === "setup") ? "settings" : "overview");
  const local = plugin.marketplace === "local";
  const needsSetup = plugin.waiting.some((w) => w.part === "setup");
  return (
    <div className="flex flex-col gap-5">
      <header className="flex items-start gap-3">
        <span className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-brand/12 text-brand">
          <BlocksIcon className="size-5" />
        </span>
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <div className="flex min-w-0 flex-wrap items-center gap-2">
            <h2 className="truncate text-base font-semibold">{plugin.name}</h2>
            <Badge>v{plugin.version.length > 14 ? plugin.version.slice(0, 12) : plugin.version}</Badge>
            <Badge>{local ? "Folder" : plugin.marketplace}</Badge>
            {needsSetup && <Badge tone="brand">Needs setup</Badge>}
          </div>
          {plugin.description && <p className="text-[13px] leading-relaxed text-muted-foreground">{plugin.description}</p>}
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <IconAction label="Show in folder" onClick={() => void revealItemInDir(plugin.path).catch(() => undefined)}>
            <FolderOpenIcon />
          </IconAction>
          {!local && (
            <IconAction label="Update" onClick={() => void run("update", async () => askConsent(await plugins.update(plugin.id), plugin.name))}>
              <RefreshCwIcon className={cn(busy === "update" && "animate-spin")} />
            </IconAction>
          )}
          {confirm ? (
            <Button size="sm" variant="destructive" onBlur={() => setConfirm(false)} onClick={() => void run("remove", () => plugins.uninstall(plugin.id))}>
              Uninstall
            </Button>
          ) : (
            <IconAction label="Uninstall" onClick={() => setConfirm(true)}>
              <Trash2Icon />
            </IconAction>
          )}
          <Switch
            aria-label={plugin.enabled ? "Turn off" : "Turn on"}
            checked={plugin.enabled}
            onCheckedChange={(on) => void run("toggle", () => setPluginEnabled(plugin, on))}
          />
        </div>
      </header>
      <ErrorNote error={error} />
      <Tabs value={tab} onValueChange={(value) => setTab(value as Tab)} className="gap-4">
        <TabsList>
          <TabsTrigger value="overview">Overview</TabsTrigger>
          <TabsTrigger value="settings">Settings</TabsTrigger>
          {plugin.accountStatus.length > 0 && (
            <TabsTrigger value="accounts">
              Accounts
              {plugin.accountStatus.some((a) => a.state === "needsSignIn") && <span aria-label="Needs sign-in" className="ml-1 size-1.5 rounded-full bg-destructive" />}
            </TabsTrigger>
          )}
          {plugin.permissions.some((p) => p.id === "ai") && <TabsTrigger value="ai">Models</TabsTrigger>}
          <TabsTrigger value="permissions">
            Permissions
            {plugin.unapproved.length > 0 && <span aria-label="To review" className="ml-1 size-1.5 rounded-full bg-brand" />}
          </TabsTrigger>
          <TabsTrigger value="runtime">Runtime</TabsTrigger>
          <TabsTrigger value="data">Data</TabsTrigger>
        </TabsList>
        <TabsContent value="overview">
          <OverviewTab plugin={plugin} />
        </TabsContent>
        <TabsContent value="settings">
          <SettingsForm plugin={plugin} />
        </TabsContent>
        <TabsContent value="accounts">
          <AccountsTab plugin={plugin} />
        </TabsContent>
        <TabsContent value="ai">
          <AiTab plugin={plugin} />
        </TabsContent>
        <TabsContent value="permissions">
          <PermissionsTab plugin={plugin} />
        </TabsContent>
        <TabsContent value="runtime">
          <RuntimeTab plugin={plugin} />
        </TabsContent>
        <TabsContent value="data">
          <DataTab plugin={plugin} />
        </TabsContent>
      </Tabs>
    </div>
  );
}
