// Settings → Webbridge: set Evo up in Chrome and Edge, see the connected
// profiles and name them (docs/features/browser-bridge/spec.md FR-2, FR-4,
// FR-23; plan 3.2).

import * as React from "react";
import { openPath } from "@tauri-apps/plugin-opener";
import { CopyIcon, FolderOpenIcon, GlobeIcon } from "lucide-react";
import { StatusDot } from "@/components/status-dot";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { inTauri } from "@/lib/platform";
import { useWebbridgeStore, type WebbridgeBrowser, type WebbridgeProfile } from "@/stores/webbridge-store";
import { SettingsGroup, SettingsHeader, SettingsRow } from "../settings-controls";

export function WebbridgeSettings() {
  const status = useWebbridgeStore((s) => s.status);
  const error = useWebbridgeStore((s) => s.error);
  const load = useWebbridgeStore((s) => s.load);

  React.useEffect(() => {
    void load();
  }, [load]);

  return (
    <div className="flex flex-col gap-8">
      <SettingsHeader
        icon={GlobeIcon}
        title="Webbridge"
        description="Let Evo work in your own Chrome or Edge: chat in the browser's side panel, and let the agent work in tabs it keeps in a group you can see and stop."
      />
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
