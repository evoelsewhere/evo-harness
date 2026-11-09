// The app each chat's agent controls (docs/features/computer-app-control,
// FR-10, FR-12): which app, its title, and whether the person stopped control.
// The shell says so with `computer://attached`, `computer://released` and
// `computer://state`; the app's card reads it here. A chat has one app at most.

import { create } from "zustand";

export type ControlState = "active" | "stopped";

export interface ControlledApp {
  chat: string;
  /** The executable, e.g. `EXCEL.EXE`. */
  app: string;
  /** The window's title when it was attached (frames bring newer ones). */
  title: string;
  windowId: number | null;
  /** The app draws web content. */
  web: boolean;
  state: ControlState;
}

export interface AttachedEvent {
  chat: string;
  app?: string | null;
  title?: string | null;
  windowId?: number | null;
  web?: boolean | null;
}

export interface ReleasedEvent {
  /** `null`: every chat's app was handed back (evo quitting, the feature off). */
  chat: string | null;
  reason?: string;
}

export interface StateEvent {
  chat: string;
  state: ControlState;
}

interface ComputerStore {
  apps: Record<string, ControlledApp>;
  attached(event: AttachedEvent): void;
  /** The app was handed back; a stopped chat keeps its entry (its card shows Allow again). */
  released(event: ReleasedEvent): void;
  setState(event: StateEvent): void;
  /** The newest title a frame showed. */
  setTitle(chat: string, title: string): void;
}

export const useComputerStore = create<ComputerStore>()((set) => ({
  apps: {},
  attached(event) {
    if (!event.chat) return;
    set((s) => ({
      apps: {
        ...s.apps,
        [event.chat]: {
          chat: event.chat,
          app: event.app ?? "",
          title: event.title ?? event.app ?? "App",
          windowId: event.windowId ?? null,
          web: event.web === true,
          state: "active",
        },
      },
    }));
  },
  released(event) {
    set((s) => {
      if (event.chat === null) return { apps: Object.fromEntries(Object.entries(s.apps).filter(([, app]) => app.state === "stopped")) };
      const app = s.apps[event.chat];
      if (!app || app.state === "stopped") return s;
      const { [event.chat]: _gone, ...rest } = s.apps;
      return { apps: rest };
    });
  },
  setState(event) {
    set((s) => {
      const app = s.apps[event.chat];
      if (!app || app.state === event.state) return s;
      return { apps: { ...s.apps, [event.chat]: { ...app, state: event.state } } };
    });
  },
  setTitle(chat, title) {
    set((s) => {
      const app = s.apps[chat];
      if (!app || !title || app.title === title) return s;
      return { apps: { ...s.apps, [chat]: { ...app, title } } };
    });
  },
}));
