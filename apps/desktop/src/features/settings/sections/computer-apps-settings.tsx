import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import { AppWindowIcon, PlusIcon, SearchIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { db } from "@/lib/db";
import { inTauri, isWindows } from "@/lib/platform";
import { Segmented, SettingsGroup, SettingsHeader, SettingsRow } from "../settings-controls";

// Settings → Computer apps (docs/features/computer-app-control FR-1, FR-24):
// whether the agent may control apps on this computer, whether each acting
// call asks first, and which apps it may or may not use. The shell reads these
// keys on every call (src-tauri/src/agent_tools/computer/tool.rs); switching
// the feature on or off reaches every chat's tools at once.

const ENABLED = "computer_apps.enabled";
const PERMISSION = "computer_apps.permission";
const ALLOWED = "computer_apps.allowed";
const BLOCKED = "computer_apps.blocked";

type Permission = "ask" | "allow";
type Rule = "any" | "allow" | "block";

interface CatalogApp {
  exe: string;
  name: string;
  running: boolean;
  icon: string | null;
}

interface Saved {
  enabled: boolean;
  permission: Permission;
  allowed: string[];
  blocked: string[];
}

const names = (value: unknown): string[] => (Array.isArray(value) ? value.filter((v): v is string => typeof v === "string" && v.trim() !== "") : []);

/** `Notepad.EXE` and `notepad` are the same app to the lists (the shell matches the same way). */
const normal = (exe: string) => exe.trim().toLowerCase().replace(/\.exe$/, "");

export function ComputerAppsSettings() {
  const [saved, setSaved] = React.useState<Saved | null>(null);
  const [apps, setApps] = React.useState<CatalogApp[] | null>(null);
  const [query, setQuery] = React.useState("");
  const [typed, setTyped] = React.useState("");

  React.useEffect(() => {
    if (!inTauri) return;
    void db.settings().then((all) =>
      setSaved({
        enabled: all[ENABLED] === true,
        permission: all[PERMISSION] === "allow" ? "allow" : "ask",
        allowed: names(all[ALLOWED]),
        blocked: names(all[BLOCKED]),
      }),
    );
  }, []);

  React.useEffect(() => {
    if (!inTauri || !isWindows || !saved?.enabled || apps) return;
    void invoke<{ apps: CatalogApp[] }>("computer_list_apps")
      .then((result) => setApps(result.apps))
      .catch(() => setApps([]));
  }, [saved?.enabled, apps]);

  const change = (patch: Partial<Saved>) => {
    if (!saved) return;
    const next = { ...saved, ...patch };
    setSaved(next);
    if (patch.enabled !== undefined) void db.setSetting(ENABLED, next.enabled);
    if (patch.permission !== undefined) void db.setSetting(PERMISSION, next.permission);
    if (patch.allowed !== undefined) void db.setSetting(ALLOWED, next.allowed);
    if (patch.blocked !== undefined) void db.setSetting(BLOCKED, next.blocked);
  };

  const ruleOf = (exe: string): Rule => {
    if (!saved) return "any";
    if (saved.blocked.some((b) => normal(b) === normal(exe))) return "block";
    if (saved.allowed.some((a) => normal(a) === normal(exe))) return "allow";
    return "any";
  };
  const setRule = (exe: string, rule: Rule) => {
    if (!saved) return;
    const without = (list: string[]) => list.filter((item) => normal(item) !== normal(exe));
    change({
      allowed: rule === "allow" ? [...without(saved.allowed), exe] : without(saved.allowed),
      blocked: rule === "block" ? [...without(saved.blocked), exe] : without(saved.blocked),
    });
  };

  // Apps in the lists that the catalog does not know (typed names) still show.
  const listed = React.useMemo(() => {
    const known = apps ?? [];
    const extra = [...(saved?.allowed ?? []), ...(saved?.blocked ?? [])]
      .filter((exe) => !known.some((app) => normal(app.exe) === normal(exe)))
      .map((exe) => ({ exe, name: exe, running: false, icon: null }));
    const all = [...extra, ...known];
    const q = query.trim().toLowerCase();
    return q ? all.filter((app) => app.name.toLowerCase().includes(q) || app.exe.toLowerCase().includes(q)) : all;
  }, [apps, saved?.allowed, saved?.blocked, query]);

  if (!isWindows) {
    return (
      <div className="flex flex-col gap-8">
        <SettingsHeader icon={AppWindowIcon} title="Computer apps" description="Let the agent work in apps on this computer, in the background." />
        <p className="text-[13px] text-muted-foreground">Computer app control is available on Windows for now.</p>
      </div>
    );
  }

  const allowListOn = (saved?.allowed.length ?? 0) > 0;
  return (
    <div className="flex flex-col gap-8">
      <SettingsHeader
        icon={AppWindowIcon}
        title="Computer apps"
        description="Let the agent work in one app at a time on this computer. The app moves off your screen while it does, and you watch it in a card; your mouse and keyboard stay yours."
      />

      <SettingsGroup title="Control">
        <SettingsRow
          labelId="computer-apps-enabled"
          label="Let the agent control apps"
          description="Adds the app control tool to every chat. Turning it off hands every app back at once."
          control={<Switch aria-labelledby="computer-apps-enabled" checked={saved?.enabled ?? false} disabled={!saved} onCheckedChange={(enabled) => change({ enabled })} />}
        />
        <SettingsRow
          labelId="computer-apps-permission"
          label="Approval"
          description={
            saved?.permission === "allow"
              ? "Follows the chat's approval mode, like a command."
              : "Every click, key and text the agent sends asks you first, whatever the chat's approval mode."
          }
          control={
            <Segmented<Permission>
              labelledBy="computer-apps-permission"
              value={saved?.permission ?? "ask"}
              onValueChange={(permission) => change({ permission })}
              options={[
                { value: "ask", label: "Ask every time" },
                { value: "allow", label: "Allow without asking" },
              ]}
            />
          }
        />
      </SettingsGroup>

      <SettingsGroup
        title="Apps"
        description={
          allowListOn
            ? "Only the apps marked Allowed can be controlled. A blocked app is never listed to the agent."
            : "Any app can be controlled except the blocked ones. Mark an app Allowed to allow only the apps you choose."
        }
      >
        <div className="flex items-center gap-2 py-3">
          <div className="relative flex-1">
            <SearchIcon className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
            <Input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search apps" className="h-8 pl-8 text-[13px]" aria-label="Search apps" />
          </div>
          <Input value={typed} onChange={(event) => setTyped(event.target.value)} placeholder="app.exe" className="h-8 w-36 text-[13px]" aria-label="Executable name" />
          <Button
            variant="outline"
            size="sm"
            disabled={!typed.trim()}
            onClick={() => {
              setRule(typed.trim(), "block");
              setTyped("");
            }}
          >
            <PlusIcon className="size-3.5" />
            Block
          </Button>
        </div>
        {saved?.enabled && apps === null && <p className="py-3 text-[13px] text-muted-foreground">Looking for apps…</p>}
        {!saved?.enabled && <p className="py-3 text-[13px] text-muted-foreground">Turn app control on to list the apps on this computer.</p>}
        {listed.map((app) => (
          <SettingsRow
            key={app.exe}
            label={
              <span className="flex items-center gap-2">
                {app.icon ? <img src={app.icon} alt="" className="size-4" /> : <AppWindowIcon className="size-4 text-muted-foreground" />}
                <span>{app.name}</span>
                <span className="text-xs text-muted-foreground">{app.exe}</span>
                {app.running && <span className="text-xs text-muted-foreground">· running</span>}
              </span>
            }
            control={
              <Segmented<Rule>
                value={ruleOf(app.exe)}
                onValueChange={(rule) => setRule(app.exe, rule)}
                options={[
                  { value: "any", label: "Default" },
                  { value: "allow", label: "Allowed" },
                  { value: "block", label: "Blocked" },
                ]}
              />
            }
          />
        ))}
      </SettingsGroup>
    </div>
  );
}
