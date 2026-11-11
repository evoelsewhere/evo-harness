import * as React from "react";
import { ChevronRightIcon, LoaderCircleIcon } from "lucide-react";
import { modelKey } from "@/components/ai-elements/model-selector";
import { ProviderIcon } from "@/components/provider-icon";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { db } from "@/lib/db";
import { cn } from "@/lib/utils";
import { useEngineStore } from "@/stores/engine-store";
import { useMemoryStore } from "@/stores/memory-store";
import { useSessionStore } from "@/stores/session-store";
import { useUiStore } from "@/stores/ui-store";
import { SettingsGroup, SettingsHeader, SettingsRow } from "../settings-controls";

// Settings → Memory: every memory setting in one place
// (docs/features/memory-topics-and-chat-recall/spec.md, FR-10).
//
// Two kinds of settings live here. The engine keeps "pause" and "sensitive
// subjects" with its memory files (`~/.evo/agent/settings.json`), so those go
// through the active chat's engine and need one to be ready. The app keeps
// "search past chats" and automatic indexing (stores/memory-store.ts); the
// shell's index worker follows them at once.

/** What the active chat's engine says about memory. */
interface EngineMemory {
  enabled: boolean;
  sensitive: boolean;
  total: number;
}

function useEngineMemory() {
  const ready = useEngineStore((s) => s.status === "ready");
  const [state, setState] = React.useState<EngineMemory | null>(null);
  const [error, setError] = React.useState<string | null>(null);

  const request = React.useCallback(async (command: string, args?: Record<string, unknown>) => {
    const engine = useEngineStore.getState();
    try {
      const next = await engine.request<Partial<EngineMemory> & { deleted?: number }>(command, args);
      setState({ enabled: next.enabled !== false, sensitive: next.sensitive === true, total: next.total ?? 0 });
      setError(null);
      if (command !== "list_memories") {
        // Other idle chats read the change from their next message, and
        // indexing that stopped for a paused memory may go on.
        await engine.reloadIdleResources().catch(() => undefined);
        await db.memoryIndexPoke().catch(() => undefined);
      }
      return true;
    } catch (e) {
      setError(String(e).replace(/^Error:\s*/, ""));
      return false;
    }
  }, []);

  React.useEffect(() => {
    if (ready) void request("list_memories");
  }, [ready, request]);

  return { state, error, ready, request };
}

function Confirm({
  open,
  onOpenChange,
  title,
  description,
  action,
  destructive,
  onConfirm,
}: {
  open: boolean;
  onOpenChange(open: boolean): void;
  title: string;
  description: string;
  action: string;
  destructive?: boolean;
  onConfirm(): void;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent showCloseButton={false}>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            variant={destructive ? "destructive" : "default"}
            autoFocus
            onClick={() => {
              onOpenChange(false);
              onConfirm();
            }}
          >
            {action}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/** The model that reads idle chats, from the models of the connected providers. */
function IndexModelPicker() {
  const models = useEngineStore((s) => s.models);
  const ready = useEngineStore((s) => s.status === "ready");
  const chosen = useMemoryStore((s) => s.indexModel);
  const setIndexModel = useMemoryStore((s) => s.setIndexModel);
  const chosenKey = chosen ? `${chosen.provider}/${chosen.modelId}` : "";
  const current = models.find((m) => modelKey(m) === chosenKey);
  const groups = React.useMemo(() => {
    const byProvider = new Map<string, typeof models>();
    for (const model of models) byProvider.set(model.provider, [...(byProvider.get(model.provider) ?? []), model]);
    return [...byProvider.entries()];
  }, [models]);

  return (
    <div className="flex items-center gap-2">
      {chosen && (
        <Button variant="ghost" size="sm" onClick={() => void setIndexModel(null)}>
          Clear
        </Button>
      )}
      <DropdownMenu>
        <DropdownMenuTrigger disabled={!ready || models.length === 0} render={<Button variant="outline" size="sm" className="max-w-64" />}>
          {chosen && <ProviderIcon id={chosen.provider} size="xs" />}
          <span className="truncate">{current ? current.name || current.id : (chosen?.modelId ?? "Choose a model")}</span>
          <ChevronRightIcon className="rotate-90" />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="max-h-96 w-64">
          <DropdownMenuRadioGroup
            value={chosenKey}
            onValueChange={(value) => {
              const model = models.find((m) => modelKey(m) === value);
              if (model) void setIndexModel({ provider: model.provider, modelId: model.id });
            }}
          >
            {groups.map(([provider, items], index) => (
              <React.Fragment key={provider}>
                {index > 0 && <DropdownMenuSeparator />}
                <DropdownMenuGroup>
                  <DropdownMenuLabel className="flex items-center gap-2">
                    <ProviderIcon id={provider} size="xs" />
                    {provider}
                  </DropdownMenuLabel>
                  {items.map((model) => (
                    <DropdownMenuRadioItem key={modelKey(model)} value={modelKey(model)}>
                      <span className="truncate">{model.name || model.id}</span>
                    </DropdownMenuRadioItem>
                  ))}
                </DropdownMenuGroup>
              </React.Fragment>
            ))}
          </DropdownMenuRadioGroup>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}

/** Whole minutes from 1 to 120, committed on Enter or when the field loses focus. */
function MinutesField({ value, disabled, onCommit }: { value: number; disabled?: boolean; onCommit(value: number): void }) {
  const [text, setText] = React.useState(String(value));
  React.useEffect(() => setText(String(value)), [value]);
  const commit = () => {
    const parsed = Number.parseInt(text, 10);
    if (Number.isFinite(parsed) && parsed >= 1 && parsed <= 120) onCommit(parsed);
    else setText(String(value));
  };
  return (
    <div className="flex items-center gap-2">
      <Input
        aria-label="Minutes a chat is idle before it is indexed"
        inputMode="numeric"
        className="h-8 w-16 text-right tabular-nums"
        value={text}
        disabled={disabled}
        onChange={(event) => setText(event.target.value)}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Enter") event.currentTarget.blur();
        }}
      />
      <span className="text-[13px] text-muted-foreground">min</span>
    </div>
  );
}

/** What the index worker is doing, in one line. */
function IndexStatusLine() {
  const status = useMemoryStore((s) => s.status);
  const indexing = useMemoryStore((s) => s.indexing);
  const lastPass = useMemoryStore((s) => s.lastPass);
  const title = useSessionStore((s) => (status?.chat ? s.sessions.find((chat) => chat.id === status.chat)?.title : undefined));
  const running = status?.state === "running";
  if (!indexing && (!status || status.state === "off") && !lastPass) return null;

  let text: string;
  let failed = false;
  if (running) {
    text = `Indexing${title ? ` “${title}”` : ""}${status.waiting > 0 ? ` · ${status.waiting} more waiting` : ""}`;
  } else if (indexing) {
    text = "Looking for chats with new messages…";
  } else if (status?.state === "stopped") {
    text = `Indexing stopped: ${status.message ?? "the index model cannot be used"}`;
    failed = true;
  } else if (lastPass?.failed) {
    text = `A chat could not be indexed: ${lastPass.failed}. Indexing tries again later.`;
    failed = true;
  } else if (lastPass) {
    text = lastPass.read > 0 ? `Indexed ${lastPass.read} ${lastPass.read === 1 ? "chat" : "chats"}. Up to date.` : "Up to date: no chat has new messages to index.";
  } else {
    text = "Up to date: waiting for a chat to go idle.";
  }
  return (
    <p role="status" className={cn("flex items-center gap-1.5 text-[13px]", failed ? "text-destructive" : "text-muted-foreground")}>
      {(running || indexing) && <LoaderCircleIcon className="size-3.5 shrink-0 animate-spin" />}
      {text}
    </p>
  );
}

export function MemorySettings() {
  const { state, error, ready, request } = useEngineMemory();
  const memory = useMemoryStore();
  const { load } = memory;
  const [confirmReset, setConfirmReset] = React.useState(false);
  const [earlier, setEarlier] = React.useState<number | null>(null);
  const [confirmEarlier, setConfirmEarlier] = React.useState(false);

  React.useEffect(() => {
    void load();
  }, [load]);

  const needsChat = !ready || !state;
  const indexBusy = memory.indexing || memory.status?.state === "running";
  const manage = () => {
    useUiStore.getState().openPanel("memory");
    useUiStore.getState().toggleRightPanel(true);
    useUiStore.getState().closeSettings();
  };
  const askEarlier = async () => {
    setEarlier(await memory.earlierCount());
    setConfirmEarlier(true);
  };

  return (
    <div className="flex flex-col gap-8">
      <SettingsHeader
        title="Memory"
        description="What the agent keeps across chats: short topics it updates as you work, and what it may look up in earlier chats."
      />
      {error && <p className="text-[13px] text-destructive">{error}</p>}

      <SettingsGroup
        title="Topics"
        description={needsChat ? "Open a chat to change these: they are kept by the engine with the memory files." : undefined}
      >
        <SettingsRow
          labelId="memory-pause"
          label="Pause memory"
          description="Keeps what is remembered, but no chat reads or saves topics and nothing is indexed until you switch this off."
          control={
            <Switch
              aria-labelledby="memory-pause"
              disabled={needsChat}
              checked={state ? !state.enabled : false}
              onCheckedChange={(paused) => void request("set_memory_enabled", { enabled: !paused })}
            />
          }
        />
        <SettingsRow
          labelId="memory-sensitive"
          label="Include sensitive subjects"
          description="Off: health, race or ethnicity, religion, political views, sexual orientation and gender identity are not saved. Identity numbers, account numbers, passwords and keys are never saved."
          control={
            <Switch
              aria-labelledby="memory-sensitive"
              disabled={needsChat}
              checked={state?.sensitive ?? false}
              onCheckedChange={(on) => void request("set_memory_settings", { sensitive: on })}
            />
          }
        />
        <SettingsRow
          label="Your memory"
          description={state ? `${state.total} ${state.total === 1 ? "topic" : "topics"} in every project together. Read, edit and forget them in the Memory panel.` : "Read, edit and forget topics in the Memory panel."}
          control={
            <Button variant="outline" size="sm" disabled={!ready} onClick={manage}>
              Manage
            </Button>
          }
        />
      </SettingsGroup>

      <SettingsGroup title="Past chats">
        <SettingsRow
          labelId="memory-search"
          label="Search past chats"
          description="Lets the agent look up earlier chats of the same project when you ask about them. It never reads another project's chats or an incognito chat."
          control={
            <Switch
              aria-labelledby="memory-search"
              disabled={!memory.loaded}
              checked={memory.searchPastChats}
              onCheckedChange={(on) => void memory.setSearchPastChats(on)}
            />
          }
        />
      </SettingsGroup>

      <SettingsGroup title="Automatic indexing">
        <SettingsRow
          labelId="memory-auto-index"
          label="Automatic indexing"
          description="When a chat has been idle, the index model reads what is new in it, updates the topics and writes a summary of the chat. The text of indexed chats is sent to that model's provider, and indexing uses tokens. Every change is listed in the Memory panel and can be undone."
          control={
            <Switch
              aria-labelledby="memory-auto-index"
              disabled={!memory.loaded || !memory.indexModel}
              checked={memory.autoIndex}
              onCheckedChange={(on) => void memory.setAutoIndex(on)}
            />
          }
        />
        <SettingsRow
          label="Index model"
          description={memory.indexModel ? "The model that reads idle chats. A change applies from the next run." : "Choose a model to switch automatic indexing on."}
          control={<IndexModelPicker />}
        />
        <SettingsRow
          label="Index after idle"
          description="How long a chat is left alone before its new messages are indexed."
          control={<MinutesField value={memory.indexIdleMinutes} disabled={!memory.loaded} onCommit={(minutes) => void memory.setIndexIdleMinutes(minutes)} />}
        />
        <SettingsRow
          label="Index now"
          description="Reads every chat with new messages without waiting for it to go idle."
          control={
            <Button variant="outline" size="sm" disabled={!memory.autoIndex || indexBusy} onClick={() => void memory.indexNow()}>
              {indexBusy && <LoaderCircleIcon className="animate-spin" />}
              {indexBusy ? "Indexing…" : "Index now"}
            </Button>
          }
        >
          <IndexStatusLine />
        </SettingsRow>
        <SettingsRow
          label="Index earlier chats"
          description="Chats from before automatic indexing was switched on are not read. This reads them once, after telling you how many there are."
          control={
            <Button variant="outline" size="sm" disabled={!memory.indexModel} onClick={() => void askEarlier()}>
              Index earlier chats…
            </Button>
          }
        />
      </SettingsGroup>

      <SettingsGroup title="Reset">
        <SettingsRow
          label="Reset memory"
          description="Deletes every topic, in every project and your personal ones, and the list of indexing changes. Chats and their summaries stay."
          control={
            <Button variant="destructive" size="sm" disabled={needsChat || (state?.total ?? 0) === 0} onClick={() => setConfirmReset(true)}>
              Reset memory…
            </Button>
          }
        />
      </SettingsGroup>

      <Confirm
        open={confirmReset}
        onOpenChange={setConfirmReset}
        title="Reset memory?"
        description={`${state?.total ?? 0} ${(state?.total ?? 0) === 1 ? "topic is" : "topics are"} deleted from this computer, in every project. This cannot be undone.`}
        action="Reset memory"
        destructive
        onConfirm={() => void request("reset_memory")}
      />
      <Confirm
        open={confirmEarlier}
        onOpenChange={setConfirmEarlier}
        title="Index earlier chats?"
        description={
          earlier
            ? `${earlier} earlier ${earlier === 1 ? "chat has" : "chats have"} not been read. Their text is sent to the index model's provider and reading them uses tokens.`
            : "Every earlier chat has been read already."
        }
        action={earlier ? `Index ${earlier} ${earlier === 1 ? "chat" : "chats"}` : "Close"}
        onConfirm={() => {
          if (earlier) void memory.indexEarlier();
        }}
      />
    </div>
  );
}
