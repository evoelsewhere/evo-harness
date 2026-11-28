import { create } from "zustand";
import { db, log } from "@/lib/db";

// The learning layers' settings and actions: memory upkeep (the nightly
// tidy-up and dream), learning skills (the skill review and, in Evo Mates,
// the retro), per app and per mate, and Tidy now. Keys are read by the shell
// when a step is due (src-tauri/src/learning/mod.rs).
// Spec: docs/features/learning-layers/spec.md (FR-6, FR-22 – FR-25, NFR-3).

export const HARNESS_UPKEEP_KEY = "learning.harness.upkeep";
export const HARNESS_SKILLS_KEY = "learning.harness.skills";
export const MATES_UPKEEP_KEY = "learning.mates.upkeep";
export const MATES_SKILLS_KEY = "learning.mates.skills";
export const REVIEW_STEPS_KEY = "learning.reviewSteps";
export const LEARN_FROM_CHANNELS_KEY = "mates.learnFromChannels";
export const DEFAULT_REVIEW_STEPS = 10;
/** A mate's own switches: `{upkeep?, skills?}`. */
export const mateKey = (id: string) => `learning.mate.${id}`;

export type LearningApp = "harness" | "mates";

export interface MateLearning {
  upkeep: boolean;
  skills: boolean;
}

/** One owner's Tidy now result, as the engine reports it. */
export interface TidyResult {
  owner: string;
  changed?: unknown[];
  archived?: number;
  struck?: number;
  skipped?: string | null;
  error?: string | null;
}

interface LearningStore {
  loaded: boolean;
  harnessUpkeep: boolean;
  harnessSkills: boolean;
  matesUpkeep: boolean;
  matesSkills: boolean;
  reviewSteps: number;
  learnFromChannels: boolean;
  mates: Record<string, MateLearning>;
  load(): Promise<void>;
  setSwitch(app: LearningApp, which: "upkeep" | "skills", on: boolean): Promise<void>;
  setReviewSteps(steps: number): Promise<void>;
  setLearnFromChannels(on: boolean): Promise<void>;
  /** A mate's own switch; off stays off whatever the app's switch says (FR-25). */
  setMate(id: string, which: keyof MateLearning, on: boolean): Promise<void>;
  /** Tidy now for each owner (`personal`, `project:<slug>`, `mate:<id>`). */
  tidyNow(owners: string[]): Promise<TidyResult[]>;
}

const clampSteps = (value: number) => Math.min(100, Math.max(3, Math.round(value)));

export const useLearningStore = create<LearningStore>((set, get) => {
  const save = (key: string, value: unknown) =>
    db
      .setSetting(key, value)
      .then(() => db.learningPoke().catch(() => undefined))
      .catch((error) => log("error", "db", `saving the learning setting ${key} failed: ${String(error)}`));

  return {
    loaded: false,
    harnessUpkeep: false,
    harnessSkills: false,
    matesUpkeep: true,
    matesSkills: true,
    reviewSteps: DEFAULT_REVIEW_STEPS,
    learnFromChannels: false,
    mates: {},

    async load() {
      const all = await db.settings().catch((): Record<string, unknown> => ({}));
      const flag = (key: string, fallback: boolean) => (typeof all[key] === "boolean" ? (all[key] as boolean) : fallback);
      const steps = all[REVIEW_STEPS_KEY];
      const mates: Record<string, MateLearning> = {};
      for (const [key, value] of Object.entries(all)) {
        if (!key.startsWith("learning.mate.") || !value || typeof value !== "object") continue;
        const own = value as Partial<MateLearning>;
        mates[key.slice("learning.mate.".length)] = { upkeep: own.upkeep !== false, skills: own.skills !== false };
      }
      set({
        loaded: true,
        harnessUpkeep: flag(HARNESS_UPKEEP_KEY, false),
        harnessSkills: flag(HARNESS_SKILLS_KEY, false),
        matesUpkeep: flag(MATES_UPKEEP_KEY, true),
        matesSkills: flag(MATES_SKILLS_KEY, true),
        reviewSteps: typeof steps === "number" ? clampSteps(steps) : DEFAULT_REVIEW_STEPS,
        learnFromChannels: flag(LEARN_FROM_CHANNELS_KEY, false),
        mates,
      });
    },

    async setSwitch(app, which, on) {
      const key = app === "harness" ? (which === "upkeep" ? HARNESS_UPKEEP_KEY : HARNESS_SKILLS_KEY) : which === "upkeep" ? MATES_UPKEEP_KEY : MATES_SKILLS_KEY;
      const field = app === "harness" ? (which === "upkeep" ? "harnessUpkeep" : "harnessSkills") : which === "upkeep" ? "matesUpkeep" : "matesSkills";
      set({ [field]: on } as Partial<LearningStore>);
      await save(key, on);
    },

    async setReviewSteps(steps) {
      const next = clampSteps(steps);
      set({ reviewSteps: next });
      await save(REVIEW_STEPS_KEY, next === DEFAULT_REVIEW_STEPS ? null : next);
    },

    async setLearnFromChannels(on) {
      set({ learnFromChannels: on });
      await save(LEARN_FROM_CHANNELS_KEY, on);
    },

    async setMate(id, which, on) {
      const current = get().mates[id] ?? { upkeep: true, skills: true };
      const next = { ...current, [which]: on };
      set({ mates: { ...get().mates, [id]: next } });
      await save(mateKey(id), next.upkeep && next.skills ? null : next);
    },

    tidyNow: (owners) => db.learningTidyNow(owners),
  };
});
