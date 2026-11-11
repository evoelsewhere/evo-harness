// App updates as the shell reports them (src-tauri/src/updates.rs), for the
// About row and the update notifications. The shell owns the update; this
// store mirrors its state, passes on the person's clicks, and — in the main
// window only — raises the notifications. See
// /docs/features/app-updates/spec.md at the repo root.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { notify } from "@/features/notifications/notify";
import { registerNotificationCommand } from "@/features/notifications/open-target";
import { openLink, REPOSITORY_URL } from "@/features/shell/app-commands";
import { db } from "@/lib/db";
import { inTauri } from "@/lib/platform";
import { useUiStore } from "@/stores/ui-store";

export type UpdateStage = "check" | "download" | "verify" | "install";

export type UpdateStatus =
  | { kind: "unavailable" }
  | { kind: "idle" }
  | { kind: "upToDate" }
  | { kind: "checking"; manual: boolean }
  | { kind: "downloading"; version: string; received: number; total: number | null }
  | { kind: "ready"; version: string }
  | { kind: "notice"; version: string }
  | { kind: "installing"; version: string }
  | { kind: "failed"; stage: UpdateStage; message: string; version?: string };

/** The newer release the last check found (FR-16). */
export interface UpdateRelease {
  version: string;
  /** Markdown; null when the release carries only a link to its page. */
  notes: string | null;
  publishedAt: number | null;
}

export interface UpdateState {
  current: string;
  lastCheckedAt: number | null;
  status: UpdateStatus;
  /** Missing from a shell older than FR-16. */
  release?: UpdateRelease | null;
}

type InstallReply = { kind: "started" } | { kind: "confirm"; working: number } | { kind: "notReady" };

/** The release's notes on the download page (FR-11). */
export const releaseNotesUrl = (version: string) => `${REPOSITORY_URL}/releases/tag/v${version}`;
/** Where to install by hand (FR-12, FR-13). */
export const DOWNLOAD_URL = `${REPOSITORY_URL}/releases/latest`;

/**
 * App setting (evo.db `settings`): check for updates automatically (FR-2).
 * Default on. The shell reads it before every automatic check and checks at
 * once when it is turned on.
 */
export const AUTO_CHECK_KEY = "updates.autoCheck";

export async function autoCheckEnabled(): Promise<boolean> {
  if (!inTauri) return true;
  try {
    return (await db.settings())[AUTO_CHECK_KEY] !== false;
  } catch {
    return true;
  }
}

/**
 * App setting: the version the new-version dialog last opened for by itself,
 * so a person who closed it is not asked again for that version (FR-17).
 */
const DIALOG_SHOWN_KEY = "updates.dialogShownFor";

const INSTALL_COMMAND = "updates.install";
/** One bell entry for updates: a newer version replaces it (FR-7). */
const NOTIFICATION_KEY = "updates";

interface UpdatesStore {
  /** null until the shell answered; stays null in the browser preview. */
  state: UpdateState | null;
  /** Chats working when the person chose Restart to update (FR-9). */
  confirm: { working: number } | null;
  /** Check automatically (FR-2); null until read. */
  autoCheck: boolean | null;
  /** The new-version dialog (FR-16, FR-17). */
  dialogOpen: boolean;
  check(): Promise<void>;
  install(force?: boolean): Promise<void>;
  dismissConfirm(): void;
  loadAutoCheck(): Promise<void>;
  setAutoCheck(enabled: boolean): Promise<void>;
  /** What's new: the dialog when the notes are known, else the release page. */
  openWhatsNew(): void;
  closeDialog(): void;
}

export const useUpdatesStore = create<UpdatesStore>()((set, get) => ({
  state: null,
  confirm: null,
  autoCheck: inTauri ? null : true,
  dialogOpen: false,
  check: async () => {
    if (!inTauri) return;
    lastCheckManual = true;
    apply(await invoke<UpdateState>("updates_check"));
  },
  install: async (force = false) => {
    if (!inTauri) return;
    set({ confirm: null });
    const reply = await invoke<InstallReply>("updates_install", { force });
    if (reply.kind === "confirm") set({ confirm: { working: reply.working } });
  },
  dismissConfirm: () => set({ confirm: null }),
  loadAutoCheck: async () => set({ autoCheck: await autoCheckEnabled() }),
  setAutoCheck: async (enabled) => {
    const before = get().autoCheck;
    set({ autoCheck: enabled });
    if (!inTauri) return;
    try {
      await db.setSetting(AUTO_CHECK_KEY, enabled);
    } catch (error) {
      set({ autoCheck: before });
      throw error;
    }
  },
  openWhatsNew: () => {
    const release = get().state?.release;
    if (release) set({ dialogOpen: true });
    else {
      const version = versionOf(get().state?.status);
      openLink(version ? releaseNotesUrl(version) : DOWNLOAD_URL);
    }
  },
  closeDialog: () => set({ dialogOpen: false }),
}));

/** The version a status names, if any. */
export function versionOf(status: UpdateStatus | undefined): string | undefined {
  return status && "version" in status ? status.version : undefined;
}

/** Whether the last check was the person's (About), not the timer's. */
let lastCheckManual = false;

/**
 * An automatic check found a new version: open the dialog, once per version
 * across restarts (FR-17). A check by hand shows it in the About row instead.
 */
async function announce(version: string) {
  try {
    const settings = await db.settings();
    if (settings[DIALOG_SHOWN_KEY] === version) return;
    await db.setSetting(DIALOG_SHOWN_KEY, version);
  } catch {
    // The dialog is a convenience: without the setting, show it this once.
  }
  if (versionOf(useUpdatesStore.getState().state?.status) === version) {
    useUpdatesStore.setState({ dialogOpen: true });
  }
}

/** Versions already announced in this run, by kind. */
const announced = new Set<string>();

function apply(next: UpdateState) {
  useUpdatesStore.setState({ state: next });
  const status = next.status;
  const once = (id: string) => !announced.has(id) && (announced.add(id), true);
  if (status.kind === "checking") lastCheckManual = status.manual;
  if (
    (status.kind === "ready" || status.kind === "notice") &&
    !lastCheckManual &&
    useUpdatesStore.getState().autoCheck !== false &&
    once(`dialog:${status.version}`)
  ) {
    void announce(status.version);
  }
  if (status.kind === "ready" && once(`ready:${status.version}`)) {
    notify({
      source: "updates",
      title: `Evo Harness ${status.version} is ready`,
      body: "Restart to update. Your chats and settings are kept.",
      key: NOTIFICATION_KEY,
      target: { type: "settings", section: "about" },
      actions: [{ label: "Restart to update", command: INSTALL_COMMAND }],
    });
  } else if (status.kind === "notice" && once(`notice:${status.version}`)) {
    notify({
      source: "updates",
      title: `Evo Harness ${status.version} is available`,
      body: "Download it from the releases page to update.",
      key: NOTIFICATION_KEY,
      target: { type: "url", href: DOWNLOAD_URL },
    });
  } else if (status.kind === "failed" && status.stage === "verify" && once(`verify:${status.version ?? ""}`)) {
    notify({
      source: "updates",
      level: "warning",
      title: "An update could not be verified",
      body: "It was deleted and not installed. evo will try again later.",
      key: NOTIFICATION_KEY,
      target: { type: "settings", section: "about" },
    });
  }
}

let started = false;

/** Follow the shell's update state. The main window only (FR-7). */
export async function startUpdates() {
  if (started || !inTauri) return;
  started = true;
  // The notification's button: About shows the confirmation if chats work.
  registerNotificationCommand(INSTALL_COMMAND, () => {
    useUiStore.getState().openSettings("about");
    void useUpdatesStore.getState().install(false);
  });
  void useUpdatesStore.getState().loadAutoCheck();
  await listen<UpdateState>("updates://state", (event) => apply(event.payload));
  try {
    apply(await invoke<UpdateState>("updates_state"));
  } catch {
    // An older shell without updates: the About row says so.
  }
}
