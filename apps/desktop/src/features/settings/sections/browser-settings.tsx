// Settings → Browser: the browser built into Evo (the Browser tool's pages,
// `stores/browser-store.ts`) and Evo Webbridge, which reaches the person's own
// Chrome or Edge (`webbridge-settings.tsx`).

import * as React from "react";
import { GlobeIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { db } from "@/lib/db";
import { useBrowserStore } from "@/stores/browser-store";
import { useWebbridgeStore } from "@/stores/webbridge-store";
import { SettingsGroup, SettingsHeader, SettingsRow } from "../settings-controls";
import { WebbridgeSettings } from "./webbridge-settings";

export function BrowserSettings() {
  return (
    <div className="flex flex-col gap-8">
      <SettingsHeader
        icon={GlobeIcon}
        title="Browser"
        description="The browser built into Evo, where you and the agent open pages beside a chat, and Evo Webbridge, which lets Evo work in your own Chrome or Edge."
      />
      <DefaultBrowserGroup />
      <BuiltInGroup />
      <WebbridgeSettings />
    </div>
  );
}

const DEFAULT_KEY = "webbridge.defaultBrowser";

/**
 * Where the agent's browser tools open pages when a call names no browser: the
 * built-in one, or a connected Chrome or Edge profile (Webbridge). A default
 * that is not connected falls back to the built-in browser.
 */
function DefaultBrowserGroup() {
  const status = useWebbridgeStore((s) => s.status);
  const [value, setValue] = React.useState("builtin");
  React.useEffect(() => {
    void db
      .settings()
      .then((all) => typeof all[DEFAULT_KEY] === "string" && setValue(all[DEFAULT_KEY] as string))
      .catch(() => undefined);
  }, []);
  const profiles = status?.profiles ?? [];
  const known = value === "builtin" || profiles.some((p) => p.key === value);
  const name = (browser: string) => (browser === "edge" ? "Edge" : "Chrome");
  async function save(next: string) {
    setValue(next);
    await db.setSetting(DEFAULT_KEY, next);
  }
  return (
    <SettingsGroup title="Default browser" description="Where the agent opens pages when you do not say which browser. You can always name one in the chat.">
      <SettingsRow
        labelId="browser-default"
        label="The agent browses in"
        description={
          value === "builtin"
            ? "The browser built into Evo, beside the chat."
            : known
              ? "Your own browser, signed in as you. Evo asks before it acts on a site, unless the site is allowed."
              : "That browser profile is not connected now, so the agent uses the built-in browser."
        }
        control={
          <select
            aria-labelledby="browser-default"
            className="h-8 max-w-56 rounded-md bg-background px-2 text-sm shadow-[0_0_0_1px_var(--border)]"
            value={value}
            onChange={(e) => void save(e.target.value)}
          >
            <option value="builtin">Built-in browser</option>
            {profiles.map((p) => (
              <option key={p.key} value={p.key}>
                {name(p.browser)} · {p.label}
              </option>
            ))}
            {!known && <option value={value}>A browser not connected now</option>}
          </select>
        }
      />
    </SettingsGroup>
  );
}

/** The built-in browser: its open pages and the recent pages its start page offers. */
function BuiltInGroup() {
  const all = useBrowserStore((s) => s.all);
  const history = useBrowserStore((s) => s.history);
  const closeTab = useBrowserStore((s) => s.closeTab);
  const chats = new Set(all.map((t) => t.chat ?? "")).size;

  return (
    <SettingsGroup title="Built-in browser" description="Pages open in the Browser tool (Ctrl+T) belong to a chat and keep running while you look at another chat.">
      <SettingsRow
        label="Open pages"
        description={all.length ? `${all.length} ${all.length === 1 ? "page" : "pages"} in ${chats} ${chats === 1 ? "chat" : "chats"}` : "No pages are open"}
        control={
          <Button variant="outline" size="sm" disabled={!all.length} onClick={() => all.forEach((t) => closeTab(t.id))}>
            Close all
          </Button>
        }
      />
      <SettingsRow
        label="Recent pages"
        description={history.length ? `${history.length} shown on the start page of a new tab` : "None yet"}
        control={
          <Button variant="outline" size="sm" disabled={!history.length} onClick={() => useBrowserStore.setState({ history: [] })}>
            Clear
          </Button>
        }
      />
    </SettingsGroup>
  );
}
