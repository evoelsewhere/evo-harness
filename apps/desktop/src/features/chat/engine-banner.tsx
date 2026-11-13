import { KeyRoundIcon, RotateCwIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { UNCONFIGURED_PROVIDER } from "@/engine/protocol";
import { usePaneEngine } from "@/stores/engine-store";
import { useUiStore } from "@/stores/ui-store";

/** Shown above the composer when the engine is not usable. */
export function EngineBanner() {
  const status = usePaneEngine((s) => s.status);
  const error = usePaneEngine((s) => s.error);
  const lastLog = usePaneEngine((s) => [...s.logs].reverse().find((l) => l.chat === s.activeId)?.line);
  const provider = usePaneEngine((s) => s.state.provider);
  const start = usePaneEngine((s) => s.start);
  const openSettings = useUiStore((s) => s.openSettings);

  if (status === "ready" && provider === UNCONFIGURED_PROVIDER) {
    return (
      <div className="mb-3 flex items-center gap-3 rounded-lg bg-card px-3 py-2 text-sm shadow-hairline">
        <div className="min-w-0 flex-1">
          <p className="font-medium">Connect a model provider</p>
          <p className="truncate text-muted-foreground">Add an API key for Anthropic, OpenAI, Gemini, OpenRouter or another provider to start chatting.</p>
        </div>
        <Button size="sm" onClick={() => openSettings("providers")}>
          <KeyRoundIcon />
          Add provider
        </Button>
      </div>
    );
  }

  if (status !== "exited" && status !== "error") return null;
  return (
    <div className="mb-3 flex items-center gap-3 rounded-lg bg-card px-3 py-2 text-sm shadow-hairline">
      <div className="min-w-0 flex-1" data-selectable>
        <p className="font-medium">The engine is not running</p>
        <p className="truncate text-muted-foreground">{error ?? lastLog ?? "It stopped unexpectedly."}</p>
      </div>
      <Button size="sm" variant="outline" onClick={() => void start()}>
        <RotateCwIcon />
        Restart
      </Button>
    </div>
  );
}
