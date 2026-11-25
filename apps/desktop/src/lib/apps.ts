// The two front ends of the one app: Evo Harness (the main window) and
// Evo Mates (its own window) (docs/features/evo-mates/spec.md, FR-3; ADR 0028).

import { invoke } from "@tauri-apps/api/core";
import { inTauri } from "@/lib/platform";

export type AppFront = "harness" | "mates";

/**
 * Bring the other front end's window forward, opening it when it is closed;
 * with `chat`, Evo Mates shows that mate's conversation (evo-mates FR-12).
 */
export function openApp(target: AppFront, chat?: string): Promise<void> {
  if (!inTauri) return Promise.resolve();
  return invoke<void>("app_open", { target, chat: chat ?? null }).catch(() => undefined);
}
