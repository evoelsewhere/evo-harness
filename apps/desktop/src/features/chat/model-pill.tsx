// The chat's model and effort as a pill, for a header instead of the
// composer: Evo Mates puts it next to the mate's name and keeps the message
// box to one line (docs/features/evo-mates/ux.md 2).

import { ModelSelector } from "@/components/ai-elements/model-selector";
import { UNCONFIGURED_PROVIDER } from "@/engine/protocol";
import { useEngineStore, usePaneEngine, useRestoring } from "@/stores/engine-store";
import { useUiStore } from "@/stores/ui-store";

export function ChatModelPill() {
  const status = usePaneEngine((s) => s.status);
  const busy = usePaneEngine((s) => s.transcript.busy);
  const models = usePaneEngine((s) => s.models);
  const state = usePaneEngine((s) => s.state);
  const thinkingLevels = usePaneEngine((s) => s.thinkingLevels);
  const defaultModel = usePaneEngine((s) => s.defaultModel);
  const pinnedModels = useUiStore((s) => s.pinnedModels);
  const restoring = useRestoring();
  const unconfigured = state.provider === UNCONFIGURED_PROVIDER;
  const loading =
    status === "starting" ||
    (!unconfigured && !state.provider && !(state.modelId ?? state.model) && (restoring || status === "ready"));
  const { setModel, setThinkingLevel, setDefaultModel } = useEngineStore.getState();
  return (
    <ModelSelector
      loading={loading}
      models={models}
      provider={unconfigured ? undefined : state.provider}
      modelId={unconfigured ? undefined : (state.modelId ?? state.model)}
      defaultModel={defaultModel}
      pinned={pinnedModels}
      thinkingLevel={state.thinkingLevel}
      thinkingLevels={thinkingLevels}
      disabled={status !== "ready" || busy}
      onModelChange={(model) => void setModel(model)}
      onPinnedChange={(keys) => useUiStore.getState().setPinnedModels(keys)}
      onDefaultChange={(model) => void setDefaultModel(model)}
      onThinkingChange={(level) => void setThinkingLevel(level)}
    />
  );
}
