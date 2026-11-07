// App setting (evo.db `settings`): start new terminals with shell integration
// (command marks and exit codes for the agent's terminal tools). Default on.
// Read fresh at every spawn, so a change applies to the next new tab.
// Spec: docs/features/agent-terminal (FR-16, T18).

import { db } from "@/lib/db";
import { inTauri } from "@/lib/platform";

export const SHELL_INTEGRATION_KEY = "terminal.shellIntegration";

export async function shellIntegrationEnabled(): Promise<boolean> {
  if (!inTauri) return true;
  try {
    return (await db.settings())[SHELL_INTEGRATION_KEY] !== false;
  } catch {
    return true;
  }
}

export const setShellIntegration = (enabled: boolean) => db.setSetting(SHELL_INTEGRATION_KEY, enabled);
