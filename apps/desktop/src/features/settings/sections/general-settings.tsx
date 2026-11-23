import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import { HistoryIcon, SlidersHorizontalIcon } from "lucide-react";
import { Select, SelectContent, SelectGroup, SelectItem, SelectLabel, SelectSeparator, SelectTrigger, SelectValue } from "@/components/ui/select";
import { CompactionSlider, CompactionSwitch } from "@/components/compaction-control";
import { Switch } from "@/components/ui/switch";
import { notify } from "@/features/notifications/notify";
import { AppIcon } from "@/features/shell/open-in-menu";
import { useOpenApps, type OpenApp } from "@/lib/open-apps";
import { inTauri, isMac } from "@/lib/platform";
import { autoOpenGeneratedEnabled, setAutoOpenGenerated } from "@/stores/office-store";
import { setShellIntegration, shellIntegrationEnabled } from "@/features/terminal";
import { useUiStore } from "@/stores/ui-store";
import { SettingsGroup, SettingsHeader, SettingsRow } from "../settings-controls";

// Startup and background, applied by the shell (src-tauri/src/system.rs), and
// the Files setting to open documents the agent creates (stores/office-store.ts).

interface SystemSettings {
  runOnStartup: boolean;
  tray: boolean;
  keepAwake: boolean;
  /** Quick entry (docs/features/quick-entry/spec.md, FR-9). */
  quickEntry: boolean;
  quickEntryShortcut: string;
  /** False when another app holds the shortcut. */
  quickEntryShortcutOk: boolean;
  /** Minutes after which the box starts a new chat; null is Never (FR-18). */
  quickEntryNewChatAfter: number | null;
}

/** **Start a new chat after**'s choices (quick-entry FR-9, FR-18). */
const NEW_CHAT_AFTER: { value: string; label: string }[] = [
  { value: "5", label: "5 minutes" },
  { value: "15", label: "15 minutes" },
  { value: "30", label: "30 minutes" },
  { value: "60", label: "1 hour" },
  { value: "never", label: "Never" },
];

type Key = "runOnStartup" | "tray" | "keepAwake" | "quickEntry";

/** Where the icon lives: the menu bar on macOS, the tray elsewhere. */
const TRAY = isMac ? "menu bar" : "system tray";
const TRAY_AREA = isMac ? "menu bar" : "notification area";

function Toggle({
  id,
  settings,
  pending,
  onChange,
}: {
  id: Key;
  settings: SystemSettings | null;
  pending: Key | null;
  onChange(key: Key, enabled: boolean): void;
}) {
  return (
    <Switch
      aria-labelledby={`setting-${id}`}
      checked={settings?.[id] ?? false}
      disabled={!settings || pending === id}
      onCheckedChange={(checked) => onChange(id, checked)}
    />
  );
}

export function GeneralSettings() {
  const [settings, setSettings] = React.useState<SystemSettings | null>(null);
  const [pending, setPending] = React.useState<Key | null>(null);

  React.useEffect(() => {
    if (!inTauri) return;
    invoke<SystemSettings>("system_settings").then(setSettings).catch(() => setSettings(null));
  }, []);

  function change(key: Key, enabled: boolean) {
    setPending(key);
    setSettings((s) => (s ? { ...s, [key]: enabled } : s));
    invoke<SystemSettings>("system_set", { key, enabled })
      .then(setSettings)
      .catch((error: unknown) => {
        setSettings((s) => (s ? { ...s, [key]: !enabled } : s));
        notify({ source: "settings", level: "error", title: "Could not change the setting", body: String(error) });
      })
      .finally(() => setPending(null));
  }

  const toggle = (id: Key) => <Toggle id={id} settings={settings} pending={pending} onChange={change} />;

  /** How long the box keeps its chat (quick-entry FR-18). */
  async function changeNewChatAfter(value: string) {
    const minutes = value === "never" ? null : Number(value);
    try {
      setSettings(await invoke<SystemSettings>("system_set_new_chat_after", { minutes }));
    } catch (error) {
      notify({ source: "settings", level: "error", title: "Could not change the setting", body: String(error) });
    }
  }

  /** Record a quick entry shortcut; a refused one keeps the old (FR-9). */
  async function changeShortcut(shortcut: string): Promise<boolean> {
    try {
      setSettings(await invoke<SystemSettings>("system_set_shortcut", { shortcut }));
      return true;
    } catch (error) {
      notify({ source: "settings", level: "error", title: "Could not use that shortcut", body: String(error) });
      return false;
    }
  }

  // Files: open a document the agent creates (app setting `files.autoOpenGenerated`, read when one appears).
  const agentCards = useUiStore((s) => s.agentCards);
  const setAgentCards = useUiStore((s) => s.setAgentCards);
  const [autoOpen, setAutoOpen] = React.useState<boolean | null>(inTauri ? null : true);
  React.useEffect(() => {
    if (inTauri) void autoOpenGeneratedEnabled().then(setAutoOpen);
  }, []);
  function changeAutoOpen(enabled: boolean) {
    setAutoOpen(enabled);
    if (!inTauri) return;
    setAutoOpenGenerated(enabled).catch((error: unknown) => {
      setAutoOpen(!enabled);
      notify({ source: "settings", level: "error", title: "Could not change the setting", body: String(error) });
    });
  }

  // Terminal: shell integration (app setting `terminal.shellIntegration`, read at each new tab).
  const [integration, setIntegration] = React.useState<boolean | null>(inTauri ? null : true);
  React.useEffect(() => {
    if (inTauri) void shellIntegrationEnabled().then(setIntegration);
  }, []);
  function changeIntegration(enabled: boolean) {
    setIntegration(enabled);
    if (!inTauri) return;
    setShellIntegration(enabled).catch((error: unknown) => {
      setIntegration(!enabled);
      notify({ source: "settings", level: "error", title: "Could not change the setting", body: String(error) });
    });
  }

  return (
    <div className="flex flex-col gap-8">
      <SettingsHeader
        icon={SlidersHorizontalIcon}
        title="General"
        description={
          inTauri ? "How Evo starts and what it keeps doing in the background." : "These settings need the desktop app."
        }
      />
      <SettingsGroup title="Startup">
        <SettingsRow
          labelId="setting-runOnStartup"
          label="Run on startup"
          description={
            settings && !settings.tray
              ? `Open Evo when you sign in to this computer. With the ${TRAY} icon off, it opens its window.`
              : `Open Evo when you sign in to this computer. It starts in the ${TRAY} without opening a window.`
          }
          control={toggle("runOnStartup")}
        />
      </SettingsGroup>
      <SettingsGroup title="Background">
        <SettingsRow
          labelId="setting-tray"
          label={`Show in ${TRAY}`}
          description={`Keep the Evo icon in the ${TRAY_AREA}. Closing the window leaves Evo running there, so running chats and scheduled tasks carry on. When off, closing the last window quits Evo.`}
          control={toggle("tray")}
        />
        <SettingsRow
          labelId="setting-keepAwake"
          label="Keep computer awake"
          description="Stop this computer from going to sleep while Evo is running, so scheduled tasks and long agent runs keep going while you are away. The display can still turn off."
          control={toggle("keepAwake")}
        />
      </SettingsGroup>
      <SettingsGroup title="Quick entry">
        <SettingsRow
          labelId="setting-quickEntry"
          label="Quick entry"
          description={`A small message box that opens over any app with the shortcut below${settings?.tray === false ? "" : ` or a click on the ${TRAY} icon`}. Press Enter and Evo answers in the box, where you can ask follow-ups; Ctrl+Enter opens the chat in Evo instead. Esc or a click elsewhere hides it and keeps your text and the chat.`}
          control={toggle("quickEntry")}
        />
        <SettingsRow
          labelId="setting-quickEntryShortcut"
          label="Shortcut"
          description={
            settings && settings.quickEntry && !settings.quickEntryShortcutOk
              ? "Another app is using this shortcut, so it does nothing. Record a different one."
              : "Opens and closes the box from any app. Click to record a new one."
          }
          control={
            <ShortcutRecorder
              labelledBy="setting-quickEntryShortcut"
              value={settings?.quickEntryShortcut ?? null}
              disabled={!settings || !settings.quickEntry}
              onChange={changeShortcut}
            />
          }
        />
        <SettingsRow
          labelId="setting-quickEntryNewChatAfter"
          label="Start a new chat after"
          description="When you open the box this long after you last used its chat, it starts empty. The earlier chat stays in the sidebar, and a chat still working or waiting for you is kept."
          control={
            <Select
              value={settings ? (settings.quickEntryNewChatAfter == null ? "never" : String(settings.quickEntryNewChatAfter)) : "15"}
              items={NEW_CHAT_AFTER}
              disabled={!settings || !settings.quickEntry}
              onValueChange={(next) => next && void changeNewChatAfter(String(next))}
            >
              <SelectTrigger aria-labelledby="setting-quickEntryNewChatAfter" className="w-40">
                <SelectValue />
              </SelectTrigger>
              <SelectContent alignItemWithTrigger={false} align="end">
                {NEW_CHAT_AFTER.map((option) => (
                  <SelectItem key={option.value} value={option.value}>
                    {option.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          }
        />
      </SettingsGroup>
      <SettingsGroup title="Context">
        <SettingsRow
          labelId="setting-autoCompact"
          label="Auto-compact"
          description="When a chat's context fills up to the share of the model's window you choose, Evo summarises the older turns to make room, before the next answer. Applies to every chat, from its next run. You can also change it from the context panel."
          control={<CompactionSwitch labelledBy="setting-autoCompact" />}
        >
          <CompactionSlider />
        </SettingsRow>
      </SettingsGroup>
      <SettingsGroup title="Workspace">
        <OpenInDefaultRow />
      </SettingsGroup>
      <SettingsGroup title="Files">
        <SettingsRow
          labelId="setting-autoOpenGenerated"
          label="Open generated documents automatically"
          description="When the agent creates a Word, Excel or PowerPoint file while it works, open it in Files so you can watch it being built. Another side-panel tool you are using stays in front."
          control={
            <Switch
              aria-labelledby="setting-autoOpenGenerated"
              checked={autoOpen ?? true}
              disabled={autoOpen === null}
              onCheckedChange={changeAutoOpen}
            />
          }
        />
      </SettingsGroup>
      <SettingsGroup title="Agent view">
        <SettingsRow
          labelId="setting-agentCards"
          label="Show cards while the agent works"
          description="Pages and shells the agent uses float over the chat as small cards, so you can watch without opening the side panel. When off, they wait as chips above the message box and open when you click them."
          control={<Switch aria-labelledby="setting-agentCards" checked={agentCards} onCheckedChange={setAgentCards} />}
        />
      </SettingsGroup>
      <SettingsGroup title="Terminal">
        <SettingsRow
          labelId="setting-terminalShellIntegration"
          label="Shell integration"
          description="Let new terminal tabs report where each command starts and ends, and its exit code, so the agent can tell when a command finished. Applies to the next new tab."
          control={
            <Switch
              aria-labelledby="setting-terminalShellIntegration"
              checked={integration ?? true}
              disabled={integration === null}
              onCheckedChange={changeIntegration}
            />
          }
        />
      </SettingsGroup>
    </div>
  );
}

/** A shortcut as the person reads it: "Ctrl+Alt+Space", "⌥ Space" on macOS. */
function shortcutLabel(shortcut: string): string {
  const names: Record<string, string> = isMac
    ? { CTRL: "⌃", CONTROL: "⌃", ALT: "⌥", OPTION: "⌥", SHIFT: "⇧", SUPER: "⌘", CMD: "⌘", COMMAND: "⌘", COMMANDORCONTROL: "⌘" }
    : { CTRL: "Ctrl", CONTROL: "Ctrl", ALT: "Alt", OPTION: "Alt", SHIFT: "Shift", SUPER: "Win", CMD: "Win", COMMAND: "Win", COMMANDORCONTROL: "Ctrl" };
  const parts = shortcut.split("+").map((part) => {
    const key = part.trim();
    return names[key.toUpperCase()] ?? key.replace(/^Key(?=[A-Z]$)/, "").replace(/^Digit(?=\d$)/, "");
  });
  return parts.join(isMac ? " " : "+");
}

/** The shortcut for a key press: its modifiers, then the key's code; null without a modifier or a key. */
function shortcutOf(event: React.KeyboardEvent): string | null {
  if (["Control", "Alt", "Shift", "Meta", "AltGraph", "OS"].includes(event.key)) return null;
  const mods = [
    event.ctrlKey && "Ctrl",
    event.altKey && "Alt",
    event.shiftKey && "Shift",
    event.metaKey && "Super",
  ].filter(Boolean);
  if (mods.length === 0 || !event.code) return null;
  return [...mods, event.code].join("+");
}

/** A button that shows the shortcut and records a new one when clicked. */
function ShortcutRecorder({
  labelledBy,
  value,
  disabled,
  onChange,
}: {
  labelledBy: string;
  value: string | null;
  disabled: boolean;
  onChange(shortcut: string): Promise<boolean>;
}) {
  const [recording, setRecording] = React.useState(false);
  const [saving, setSaving] = React.useState(false);

  function onKeyDown(event: React.KeyboardEvent<HTMLButtonElement>) {
    if (!recording) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape" && !event.ctrlKey && !event.altKey && !event.metaKey) {
      setRecording(false);
      return;
    }
    const shortcut = shortcutOf(event);
    if (!shortcut) return;
    setRecording(false);
    setSaving(true);
    void onChange(shortcut).finally(() => setSaving(false));
  }

  return (
    <button
      type="button"
      aria-labelledby={labelledBy}
      disabled={disabled || saving}
      onClick={() => setRecording((on) => !on)}
      onBlur={() => setRecording(false)}
      onKeyDown={onKeyDown}
      className="h-8 min-w-36 rounded-md bg-background px-3 font-mono text-[13px] shadow-hairline disabled:opacity-50 data-[recording=true]:text-muted-foreground"
      data-recording={recording}
    >
      {recording ? "Press the keys…" : value ? shortcutLabel(value) : "—"}
    </button>
  );
}

/** Select value for "follow the app used last". */
const LAST_USED = "__last";

/**
 * The app the toolbar's "Open in" button opens the workspace in: one of the
 * apps the shell found, or the one used last (the default).
 */
function OpenInDefaultRow() {
  const apps = useOpenApps();
  const defaultId = useUiStore((s) => s.openInDefault);
  const setDefault = useUiStore((s) => s.setOpenInDefault);
  // A default whose app is gone (uninstalled) reads as "Last used", as the toolbar does.
  const chosen = apps.find((a) => a.id === defaultId);
  const value = chosen ? chosen.id : LAST_USED;
  const places = apps.filter((a) => a.kind !== "terminal");
  const terminals = apps.filter((a) => a.kind === "terminal");
  const items = [{ value: LAST_USED, label: "Last used" }, ...apps.map((a) => ({ value: a.id, label: a.name }))];

  const option = (app: OpenApp) => (
    <SelectItem key={app.id} value={app.id}>
      <AppIcon app={app} />
      {app.name}
    </SelectItem>
  );

  return (
    <SettingsRow
      labelId="setting-openInDefault"
      label="Default Open in app"
      description="Where the toolbar's Open in button opens the workspace folder. Last used follows the app you picked from its menu most recently."
      control={
        <Select
          value={value}
          items={items}
          disabled={!inTauri || apps.length === 0}
          onValueChange={(next) => setDefault(next && next !== LAST_USED ? String(next) : null)}
        >
          <SelectTrigger aria-labelledby="setting-openInDefault" className="w-52">
            <SelectValue>
              {(current: string) => {
                const app = apps.find((a) => a.id === current);
                return (
                  <span className="flex min-w-0 items-center gap-2">
                    {app ? <AppIcon app={app} /> : <HistoryIcon className="size-4 shrink-0 text-muted-foreground" />}
                    <span className="truncate">{app?.name ?? "Last used"}</span>
                  </span>
                );
              }}
            </SelectValue>
          </SelectTrigger>
          <SelectContent alignItemWithTrigger={false} align="end">
            <SelectItem value={LAST_USED}>
              <HistoryIcon className="text-muted-foreground" />
              Last used
            </SelectItem>
            {places.length > 0 && (
              <>
                <SelectSeparator />
                <SelectGroup>
                  <SelectLabel>Apps</SelectLabel>
                  {places.map(option)}
                </SelectGroup>
              </>
            )}
            {terminals.length > 0 && (
              <>
                <SelectSeparator />
                <SelectGroup>
                  <SelectLabel>Terminals</SelectLabel>
                  {terminals.map(option)}
                </SelectGroup>
              </>
            )}
          </SelectContent>
        </Select>
      }
    />
  );
}
