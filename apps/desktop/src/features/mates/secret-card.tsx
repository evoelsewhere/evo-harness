// A mate's secure card (docs/features/evo-mates/spec.md, FR-43): a masked
// field for a secret the mate asked for (`request_secret`). The value goes
// straight to the shell's vault (`mates_secret_submit`); the card's answer
// carries only "Save" or "Cancel", so the value never reaches the
// transcript or the model.

import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import { EyeIcon, EyeOffIcon, KeyRoundIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { PermissionRequest } from "@/features/chat";
import { log } from "@/lib/db";
import type { Mate } from "@/lib/mates";
import { useEngineStore } from "@/stores/engine-store";

export function SecretCard({ mate, permission }: { mate: Mate; permission: PermissionRequest }) {
  const secret = permission.secret;
  const [value, setValue] = React.useState("");
  const [shown, setShown] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [saving, setSaving] = React.useState(false);
  const field = React.useRef<HTMLInputElement>(null);

  React.useEffect(() => {
    field.current?.focus();
  }, [permission.requestId]);

  if (!secret) return null;
  const [save, cancel] = [permission.options[0], permission.options[1]];

  const answer = (index: number) => {
    const option = permission.options[index];
    if (option) void useEngineStore.getState().answerPermission(permission.requestId, option, index);
  };

  const submit = async () => {
    if (!value || saving) return;
    setSaving(true);
    try {
      await invoke("mates_secret_submit", { chat: mate.chatId, name: secret.name, value });
      setValue("");
      answer(0);
    } catch (e) {
      setError(String(e).replace(/^Error:\s*/, ""));
      log("warn", "mates", "could not hand over a secret");
    } finally {
      setSaving(false);
    }
  };

  return (
    <form
      className="flex flex-col gap-3 rounded-2xl bg-card p-4 shadow-[0_0_0_1px_var(--border)]"
      onSubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
    >
      <div className="flex items-start gap-3">
        <div className="flex size-8 shrink-0 items-center justify-center rounded-full bg-brand/15 text-brand">
          <KeyRoundIcon className="size-4" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="text-[14px] font-medium text-foreground">{permission.title}</div>
          {secret.reason && <div className="text-xs text-muted-foreground">{permission.description}</div>}
        </div>
      </div>
      <label className="flex flex-col gap-1.5">
        <span className="text-xs text-muted-foreground">{secret.name}</span>
        <div className="flex items-center rounded-lg border border-border bg-background pr-1 focus-within:border-brand/70 focus-within:ring-2 focus-within:ring-brand/15">
          <input
            ref={field}
            type={shown ? "text" : "password"}
            value={value}
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => {
              setValue(event.target.value);
              setError(null);
            }}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                event.preventDefault();
                answer(1);
              }
            }}
            placeholder={secret.service ? `Your ${secret.service} secret` : "The secret"}
            className="h-9 min-w-0 flex-1 bg-transparent px-3 font-mono text-[13px] text-foreground outline-none placeholder:font-sans placeholder:text-muted-foreground"
          />
          <button
            type="button"
            aria-label={shown ? "Hide" : "Show"}
            onClick={() => setShown((s) => !s)}
            className="rounded-md p-1.5 text-muted-foreground hover:text-foreground"
          >
            {shown ? <EyeOffIcon className="size-4" /> : <EyeIcon className="size-4" />}
          </button>
        </div>
      </label>
      {error && <p className="text-xs text-destructive">{error}</p>}
      <p className="text-xs text-muted-foreground">
        Kept in {mate.name}'s vault on this computer. {mate.name} uses it by name and never sees it.
      </p>
      <div className="flex justify-end gap-2">
        <Button type="button" variant="ghost" size="sm" onClick={() => answer(1)}>
          {cancel?.title ?? "Cancel"}
        </Button>
        <Button type="submit" size="sm" disabled={!value || saving}>
          {save?.title ?? "Save"}
        </Button>
      </div>
    </form>
  );
}
