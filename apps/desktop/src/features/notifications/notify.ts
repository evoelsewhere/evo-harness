// `notify()` — the one way a feature tells the user something happened.
//
// Every notification lands in the bell (the notification center). On top of
// that it is delivered on one channel, picked from where the user is:
//
//   window focused      → an in-app toast (unless `toast: false`)
//   window not focused  → an OS notification (unless `system: false`)
//
// Do not disturb, or muting the source, silences both channels; the bell still
// collects everything. See /docs/features/notifications/spec.md at the repo root.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { inTauri } from "@/lib/platform";
import {
  useNotificationStore,
  type AppNotification,
  type NotificationAction,
  type NotificationLevel,
  type NotificationTarget,
} from "@/stores/notification-store";

export type { AppNotification, NotificationAction, NotificationLevel, NotificationTarget };

export interface NotifyOptions {
  title: string;
  body?: string;
  level?: NotificationLevel;
  /** The feature raising it: "engine", "scheduler", "plugins", … */
  source: string;
  target?: NotificationTarget;
  /** Replace the earlier notification with this key instead of adding one. */
  key?: string;
  /** Show a toast when the window is focused. Default true. */
  toast?: boolean;
  /** Send an OS notification when the window is not focused. Default true. */
  system?: boolean;
  /** Only record it in the bell, already read (a log entry, not news). Default false. */
  quiet?: boolean;
  /** Buttons on the toast and the bell row, run through `registerNotificationCommand()`. */
  actions?: NotificationAction[];
}

const newId = () => `n-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`;

/** Whether the user is looking at the app right now. */
export const windowFocused = () => document.visibilityState === "visible" && document.hasFocus();

/** Record a notification and deliver it. Returns its id. */
export function notify(options: NotifyOptions): string {
  const store = useNotificationStore.getState();
  const notification: AppNotification = {
    id: newId(),
    level: options.level ?? "info",
    title: options.title,
    body: options.body,
    source: options.source,
    createdAt: Date.now(),
    read: options.quiet ?? false,
    target: options.target,
    key: options.key,
    actions: options.actions,
  };
  const muted = options.quiet || store.doNotDisturb || store.mutedSources.includes(options.source);
  const focused = windowFocused();
  store.add(notification, !muted && focused && options.toast !== false);
  if (!muted && !focused && options.system !== false) void sendSystem(notification);
  return notification.id;
}

// ── Notices from the shell ───────────────────────────────────────────────────
//
// The shell announces questions and the ends of runs in chats no focused
// window shows, once for the whole app (docs/features/shell-hosted-engines/spec.md,
// FR-9, FR-10; src-tauri/src/supervisor/notices.rs). It sends the OS
// notification itself and gives one window the bell entry (`notify://entry`);
// entries it recorded while no window was open wait for the next window
// (`notifications_take`).

/** A bell entry the shell recorded: titles only, clicking it opens `chat`. */
export interface ShellBellEntry {
  id: string;
  title: string;
  body: string;
  level: NotificationLevel;
  source: string;
  chat: string;
  key: string;
  at: number;
}

const shellEntry = (entry: ShellBellEntry): AppNotification => ({
  id: entry.id,
  level: entry.level,
  title: entry.title,
  body: entry.body,
  source: entry.source,
  createdAt: entry.at,
  read: false,
  // A scheduled run that made no chat opens nothing.
  target: entry.chat ? { type: "session", id: entry.chat } : undefined,
  key: entry.key,
});

let shellNotices = false;

/** Take the shell's bell entries in this window (`label`), from now on. */
export function startShellNotices(label: string) {
  if (shellNotices || !inTauri) return;
  shellNotices = true;
  void listen<{ entry: ShellBellEntry; window?: string }>("notify://entry", ({ payload }) => {
    if (payload.window && payload.window !== label) return;
    const { entry } = payload;
    // The shell sent the OS notification; a toast when this window is in front.
    notify({ title: entry.title, body: entry.body, level: entry.level, source: entry.source, key: entry.key, target: entry.chat ? { type: "session", id: entry.chat } : undefined, system: false });
  }).catch(() => undefined);
  void invoke<ShellBellEntry[]>("notifications_take")
    .then((entries) => {
      const store = useNotificationStore.getState();
      for (const entry of entries) store.add(shellEntry(entry), false);
    })
    .catch(() => undefined);
}

// ── OS notifications ─────────────────────────────────────────────────────────

let permission: Promise<boolean> | null = null;

/** Ask once per run; the OS remembers the answer. */
function systemAllowed(): Promise<boolean> {
  permission ??= (async () => {
    if (inTauri) {
      if (await isPermissionGranted()) return true;
      return (await requestPermission()) === "granted";
    }
    if (typeof Notification === "undefined") return false;
    if (Notification.permission === "granted") return true;
    return (await Notification.requestPermission()) === "granted";
  })().catch(() => false);
  return permission;
}

async function sendSystem(notification: AppNotification) {
  if (!(await systemAllowed())) return;
  if (inTauri) sendNotification({ title: notification.title, body: notification.body });
  else new Notification(notification.title, { body: notification.body });
}
