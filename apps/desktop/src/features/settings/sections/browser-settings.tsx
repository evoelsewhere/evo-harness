// Settings → Browser: the browser built into Evo (the Browser tool's pages,
// `stores/browser-store.ts`) and Evo Webbridge, which reaches the person's own
// Chrome or Edge (`webbridge-settings.tsx`).

import { GlobeIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useBrowserStore } from "@/stores/browser-store";
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
      <BuiltInGroup />
      <WebbridgeSettings />
    </div>
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
