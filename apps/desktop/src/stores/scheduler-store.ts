// Scheduled tasks: a prompt run as a new chat in a folder, on a schedule
// (`lib/schedule.ts`). Saved in the app database (`scheduler.tasks`), which the
// shell alone writes: it runs due tasks every 20 seconds and, at launch, a task
// missed while evo was closed once, with or without a window
// (src-tauri/src/agent_tools/scheduler/run.rs;
// docs/features/shell-hosted-engines/spec.md, FR-7). This store shows the
// tasks for the Scheduler page and edits them through the shell, so the page,
// the agent's tools and the runner never overwrite each other. Runs need evo
// running — the tray icon and Keep awake (Settings → General) let tasks carry
// on with the window closed or the computer idle.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { notify } from "@/features/notifications/notify";
import { db } from "@/lib/db";
import type { Schedule } from "@/lib/schedule";

const KEY = "scheduler.tasks";

export interface TaskRun {
  id: string;
  at: number;
  chatId: string | null;
  trigger: "schedule" | "manual" | "missed";
  error?: string;
}

export interface ScheduledTask {
  id: string;
  name: string;
  prompt: string;
  /** The folder the chat works in; `null`: this computer. */
  folder: string | null;
  schedule: Schedule;
  enabled: boolean;
  createdAt: number;
  updatedAt: number;
  /** Who made it: you, or an agent (with your approval). */
  createdBy: "you" | "agent";
  nextRunAt: number | null;
  runs: TaskRun[];
}

export type TaskInput = Pick<ScheduledTask, "name" | "prompt" | "folder" | "schedule"> & { enabled?: boolean };

interface SchedulerStore {
  tasks: ScheduledTask[];
  loaded: boolean;
  /** The task shown on the Scheduler page. */
  selectedId: string | null;
  select(id: string | null): void;
  load(): Promise<void>;
  create(input: TaskInput): Promise<ScheduledTask | null>;
  update(id: string, patch: Partial<TaskInput>): Promise<ScheduledTask | null>;
  remove(id: string): Promise<void>;
  /** Run a task now (the shell starts its chat); the new chat's id. */
  run(id: string): Promise<string | null>;
}

const failed = (title: string) => (error: unknown) => {
  notify({ title, body: String(error), level: "error", source: "scheduler" });
  return null;
};

let listening = false;

export const useSchedulerStore = create<SchedulerStore>()((set, get) => {
  /** Show what the shell saved. */
  const put = (task: ScheduledTask) =>
    set((s) => ({
      tasks: s.tasks.some((t) => t.id === task.id) ? s.tasks.map((t) => (t.id === task.id ? task : t)) : [...s.tasks, task],
    }));
  const save = (task: Partial<TaskInput> & { id?: string }) =>
    invoke<ScheduledTask>("scheduler_save", { task })
      .then((saved) => {
        put(saved);
        return saved;
      })
      .catch(failed("Could not save the scheduled task"));

  return {
    tasks: [],
    loaded: false,
    selectedId: null,
    select: (selectedId) => set({ selectedId }),

    async load() {
      // The shell changes the tasks (a run, the agent's tools, another
      // window's page) and says so.
      if (!listening) {
        listening = true;
        void listen("scheduler://changed", () => void useSchedulerStore.getState().load().catch(() => undefined));
      }
      const settings = await db.settings();
      const saved = Array.isArray(settings[KEY]) ? (settings[KEY] as ScheduledTask[]) : [];
      set({ tasks: saved, loaded: true });
    },

    create: (input) => save(input),

    async update(id, patch) {
      if (!get().tasks.some((t) => t.id === id)) return null;
      return save({ ...patch, id });
    },

    async remove(id) {
      await invoke("scheduler_delete", { id }).catch(failed("Could not delete the scheduled task"));
      set((s) => ({ tasks: s.tasks.filter((t) => t.id !== id), selectedId: s.selectedId === id ? null : s.selectedId }));
    },

    async run(id) {
      // The shell records the run and says how it went (bell and notification).
      return invoke<string | null>("scheduler_run_now", { id }).catch(failed("Could not run the scheduled task"));
    },
  };
});
