// Settings → Browser, Webbridge part: set Evo up in Chrome and Edge, see the
// connected profiles and name them (docs/features/browser-bridge/spec.md FR-2,
// FR-4, FR-23; plan 3.2). The page itself is `browser-settings.tsx`.

import * as React from "react";
import { openPath } from "@tauri-apps/plugin-opener";
import { CopyIcon, FolderOpenIcon, OctagonXIcon, PlusIcon } from "lucide-react";
import { StatusDot } from "@/components/status-dot";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { db } from "@/lib/db";
import { useSessionStore } from "@/stores/session-store";
import { inTauri } from "@/lib/platform";
import { useWebbridgeStore, type SitePermission, type WebbridgeBrowser, type WebbridgeProfile } from "@/stores/webbridge-store";
import { Segmented, SettingsGroup, SettingsRow } from "../settings-controls";

/** The Webbridge groups of the Browser page. */
export function WebbridgeSettings() {
  const status = useWebbridgeStore((s) => s.status);
  const error = useWebbridgeStore((s) => s.error);
  const load = useWebbridgeStore((s) => s.load);

  React.useEffect(() => {
    void load();
  }, [load]);

  return (
    <div className="flex flex-col gap-8">
      <header className="flex items-start gap-4 border-t pt-6">
        <div className="min-w-0 flex-1">
          <h2 className="text-base font-semibold tracking-tight">Evo Webbridge</h2>
          <p className="mt-0.5 max-w-xl text-[13px] text-muted-foreground">
            Let Evo work in your own Chrome or Edge: chat in the browser's side panel, and let the agent work in tabs it keeps in a group you can see and stop.
          </p>
        </div>
        {status && status.profiles.length > 0 && <StopAllButton />}
      </header>
      {!inTauri && <p className="text-[13px] text-muted-foreground">These settings need the desktop app.</p>}
      {error && <p className="text-[13px] text-destructive">{error}</p>}
      {status?.hostMissing && (
        <p className="text-[13px] text-muted-foreground">
          This build of Evo has no Webbridge host beside it, so browsers cannot reach it. ({status.hostMissing})
        </p>
      )}
      {status && (
        <SettingsGroup title="Browsers" description="Setting up a browser lets the Evo extension in it reach this app. Nothing is shared until you chat or the agent opens a tab.">
          {status.browsers.map((browser) => (
            <BrowserRow key={browser.id} browser={browser} profiles={status.profiles.filter((p) => p.browser === browser.id)} folder={status.extensionFolder} />
          ))}
        </SettingsGroup>
      )}
      {status && status.profiles.length > 0 && (
        <SettingsGroup title="Connected profiles" description="Each browser profile with the extension shows here. Give it a name the agent and you can tell apart.">
          {status.profiles.map((profile) => (
            <ProfileRow key={profile.key} profile={profile} />
          ))}
        </SettingsGroup>
      )}
      {status && <SitesGroup />}
      {status && <SidePanelGroup />}
      {status?.refused && (
        <SettingsGroup title="Refused connections">
          <p className="py-3 text-[13px] text-muted-foreground">{status.refused}</p>
        </SettingsGroup>
      )}
    </div>
  );
}

function BrowserRow({ browser, profiles, folder }: { browser: WebbridgeBrowser; profiles: WebbridgeProfile[]; folder: string | null }) {
  const busy = useWebbridgeStore((s) => s.busy);
  const setUp = useWebbridgeStore((s) => s.setUp);
  const remove = useWebbridgeStore((s) => s.remove);
  const [copied, setCopied] = React.useState(false);

  const connected = profiles.length > 0;
  const tone = connected ? "ok" : browser.registered ? "warn" : "idle";
  const description = connected
    ? `Connected · ${profiles.map((p) => p.label).join(", ")} · extension ${profiles[0].version}`
    : browser.registered
      ? "Set up. Add the Evo extension to this browser to connect."
      : "Not set up";

  async function copyFolder() {
    if (!folder) return;
    await navigator.clipboard.writeText(folder);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  }

  return (
    <SettingsRow
      label={
        <span className="inline-flex items-center gap-2">
          <StatusDot tone={tone} />
          {browser.label}
        </span>
      }
      description={description}
      control={
        browser.registered ? (
          <Button variant="outline" size="sm" disabled={busy !== null} onClick={() => void remove(browser.id)}>
            Remove
          </Button>
        ) : (
          <Button size="sm" disabled={busy !== null} onClick={() => void setUp(browser.id)}>
            Set up {browser.label}
          </Button>
        )
      }
    >
      {browser.registered && !connected && (
        <ol className="ml-4 list-decimal space-y-1 text-[13px] text-muted-foreground">
          <li>
            In {browser.label}, open <span className="font-mono text-foreground">{browser.extensionsPage}</span> and turn on Developer mode.
          </li>
          <li>Choose Load unpacked and pick the Evo extension folder{folder ? ":" : "."}</li>
          {folder && (
            <li className="list-none">
              <div className="mt-1 flex items-center gap-2">
                <span className="min-w-0 truncate font-mono text-[12px] text-foreground">{folder}</span>
                <Button variant="ghost" size="sm" onClick={() => void copyFolder()}>
                  <CopyIcon /> {copied ? "Copied" : "Copy"}
                </Button>
                <Button variant="ghost" size="sm" onClick={() => void openPath(folder).catch(() => {})}>
                  <FolderOpenIcon /> Open
                </Button>
              </div>
            </li>
          )}
          <li>Open the extension's menu: it says Connected to Evo.</li>
        </ol>
      )}
    </SettingsRow>
  );
}

function ProfileRow({ profile }: { profile: WebbridgeProfile }) {
  const rename = useWebbridgeStore((s) => s.rename);
  const [label, setLabel] = React.useState(profile.label);
  React.useEffect(() => setLabel(profile.label), [profile.label]);
  const browser = profile.browser === "edge" ? "Edge" : "Chrome";

  function commit() {
    const next = label.trim();
    if (next && next !== profile.label) void rename(profile.key, next);
    else setLabel(profile.label);
  }

  return (
    <SettingsRow
      label={`${browser} · ${profile.label}`}
      description={
        profile.debugger
          ? `Tabs are named ${profile.browser}:${profile.profile}:<tab> for the agent.`
          : "This browser's administrator turned off the debugging API, so Evo cannot work in its tabs."
      }
      control={
        <Input
          aria-label={`Name of the ${browser} profile`}
          className="h-8 w-44"
          value={label}
          maxLength={40}
          onChange={(e) => setLabel(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.key === "Enter") e.currentTarget.blur();
          }}
        />
      }
    />
  );
}

function StopAllButton() {
  const busy = useWebbridgeStore((s) => s.busy);
  const stopAll = useWebbridgeStore((s) => s.stopAll);
  return (
    <Button variant="outline" size="sm" disabled={busy !== null} onClick={() => void stopAll()}>
      <OctagonXIcon /> Stop all
    </Button>
  );
}

const PERMISSIONS: { value: SitePermission; label: string }[] = [
  { value: "ask", label: "Ask" },
  { value: "allow", label: "Allow" },
  { value: "block", label: "Block" },
];

/** Site permissions (FR-12): Ask by default; Allow skips the act question; Block refuses in every mode. */
function SitesGroup() {
  const sites = useWebbridgeStore((s) => s.sites);
  const setSite = useWebbridgeStore((s) => s.setSite);
  const [draft, setDraft] = React.useState("");
  const [permission, setPermission] = React.useState<SitePermission>("block");

  function add() {
    const value = draft.trim();
    if (!value) return;
    const site = /^[a-z][a-z0-9+.-]*:\/\//i.test(value) ? value : `https://${value}`;
    void setSite(site, permission);
    setDraft("");
  }

  return (
    <SettingsGroup
      title="Site permissions"
      description="One list for every browser and profile. Every other site is Ask: the agent asks once per site and chat before it clicks or types there. Allow skips that question; Block keeps Evo off the site in every mode."
    >
      {sites.length === 0 && <p className="py-3 text-[13px] text-muted-foreground">No site has a permission yet.</p>}
      {sites.map((row) => (
        <SettingsRow
          key={row.site}
          label={<span className="font-mono text-[13px]">{row.site}</span>}
          control={<Segmented value={row.permission} onValueChange={(next) => void setSite(row.site, next)} options={PERMISSIONS} />}
        />
      ))}
      <div className="flex items-center gap-2 py-3.5">
        <Input
          aria-label="Site"
          placeholder="example.com"
          className="h-8 flex-1"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") add();
          }}
        />
        <Segmented value={permission} onValueChange={(next) => setPermission(next as SitePermission)} options={PERMISSIONS.filter((p) => p.value !== "ask")} />
        <Button size="sm" variant="outline" disabled={!draft.trim()} onClick={add}>
          <PlusIcon /> Add
        </Button>
      </div>
    </SettingsGroup>
  );
}

const VOICE_KEY = "webbridge.voiceInput";
const WORKSPACE_KEY = "webbridge.newChatWorkspace";

/** The browser's side panel: where new chats start, and voice input (spec FR-10, FR-29). */
function SidePanelGroup() {
  const workspaces = useSessionStore((s) => s.workspaces);
  const [settings, setSettings] = React.useState<Record<string, unknown>>({});
  React.useEffect(() => {
    void db.settings().then(setSettings).catch(() => undefined);
  }, []);
  async function save(key: string, value: unknown) {
    setSettings((current) => ({ ...current, [key]: value }));
    await db.setSetting(key, value);
  }
  const workspace = typeof settings[WORKSPACE_KEY] === "string" ? (settings[WORKSPACE_KEY] as string) : "";
  return (
    <SettingsGroup title="Side panel" description="Chat with Evo next to any page from the browser's side panel. It shows the same chats as this app.">
      <SettingsRow
        labelId="webbridge-workspace"
        label="New chats start in"
        control={
          <select
            aria-labelledby="webbridge-workspace"
            className="h-8 rounded-md bg-background px-2 text-sm shadow-[0_0_0_1px_var(--border)]"
            value={workspace}
            onChange={(e) => void save(WORKSPACE_KEY, e.target.value || null)}
          >
            <option value="">The last used workspace</option>
            {workspaces.map((w) => (
              <option key={w.id} value={w.id}>
                {w.name}
              </option>
            ))}
          </select>
        }
      />
      <SettingsRow
        labelId="webbridge-voice"
        label="Voice input"
        description="Shows a microphone in the side panel. Your browser turns speech into text with its maker's service; only the text reaches Evo, and nothing is sent until you send it."
        control={<Switch aria-labelledby="webbridge-voice" checked={settings[VOICE_KEY] === true} onCheckedChange={(on) => void save(VOICE_KEY, on)} />}
      />
    </SettingsGroup>
  );
}

