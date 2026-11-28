// A mate's conversation (docs/features/evo-mates/spec.md, FR-58 – FR-62):
// the messaging thread, the mate's approval or question when it asks
// (Harness's cards), and the pill message box. Reactions are kept by the
// shell and shown on their bubbles; Reply puts a quote above the box.

import * as React from "react";
import { ApprovalPanel, QuestionsPanel, usePendingPermission } from "@/features/chat";
import { log } from "@/lib/db";
import { matesApi, type Mate, type ReactionKind } from "@/lib/mates";
import { MateComposer } from "./composer";
import { FindBar } from "./find-bar";
import { MatePresenceFace } from "./presence-face";
import { excerpt, type ReplyTo } from "./message-markup";
import { SecretCard } from "./secret-card";
import { Starters, startersFor } from "./starters";
import { isGreeting, isHeartbeat } from "./thread-rows";
import { useEngineStore } from "@/stores/engine-store";
import { useMatesStore } from "@/stores/mates-store";
import { MateSuggestions } from "./suggestion-card";
import { MateThread } from "./thread";

/** What the card asks for, in the person's words (FR-27). */
function cardKind(kind: string): string {
  if (kind === "secret") return "A secret";
  if (kind === "questions") return "A question";
  return "Your approval";
}

/** A card the mate waits on, under a header with its face (FR-27). */
function NeedsYouFrame({ mate, kind, children }: { mate: Mate; kind: string; children: React.ReactNode }) {
  return (
    <section aria-label={`${mate.name} needs you`} className="overflow-hidden rounded-[22px] bg-card shadow-[0_0_0_1px_var(--border),0_8px_24px_-16px_rgb(0_0_0/0.3)]">
      <header className="flex items-center gap-2 px-3.5 pt-3 pb-1">
        <MatePresenceFace mate={mate} size={24} ring="var(--card)" />
        <span className="text-[13px] font-semibold text-foreground">{mate.name} needs you</span>
        <span className="rounded-full bg-amber-500/15 px-2 py-0.5 text-[11px] font-medium text-amber-700 dark:text-amber-300">{cardKind(kind)}</span>
      </header>
      <div className="[&>*]:shadow-none">{children}</div>
    </section>
  );
}

export function MateConversation({ mate, empty }: { mate: Mate; empty: React.ReactNode }) {
  const permission = usePendingPermission();
  const [reply, setReply] = React.useState<ReplyTo | null>(null);
  const [reactions, setReactions] = React.useState<Record<string, ReactionKind>>({});
  const templates = useMatesStore((s) => s.templates);
  const busy = useEngineStore((s) => s.activeId === mate.chatId && s.transcript.busy);
  const turnsCount = useEngineStore((s) => s.transcript.turns.length);
  // The person has written: neither a greeting nor a heartbeat is theirs.
  const wrote = useEngineStore((s) =>
    s.activeId === mate.chatId ? s.transcript.turns.some((t) => !isGreeting(t.prompt) && !isHeartbeat(t.prompt)) : true,
  );

  React.useEffect(() => {
    setReply(null);
    let gone = false;
    void matesApi
      .reactions(mate.chatId)
      .then((list) => !gone && setReactions(Object.fromEntries(list.map((r) => [r.message, r.reaction]))))
      .catch(() => undefined);
    return () => {
      gone = true;
    };
  }, [mate.chatId]);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="relative min-h-0 flex-1">
        <FindBar scopeId={`thread-${mate.chatId}`} version={turnsCount} />
        <MateThread
          key={mate.chatId}
          scopeId={`thread-${mate.chatId}`}
          waiting={Boolean(permission)}
          mate={mate}
          reactions={reactions}
          empty={empty}
          onReply={setReply}
          onReact={(bubble, kind) => {
            setReactions((current) => {
              const next = { ...current };
              if (kind) next[bubble.key] = kind;
              else delete next[bubble.key];
              return next;
            });
            void matesApi
              .react(mate.chatId, bubble.key, kind, excerpt(bubble.text))
              .catch((error) => log("warn", "mates", `could not save a reaction: ${String(error)}`));
          }}
        />
      </div>
      {permission && (
        <div className="mx-auto w-full max-w-3xl px-4 pb-2">
          <NeedsYouFrame mate={mate} kind={permission.kind}>
            {permission.kind === "secret" ? (
              <SecretCard key={permission.requestId} mate={mate} permission={permission} />
            ) : permission.kind === "questions" ? (
              <QuestionsPanel key={permission.requestId} permission={permission} asker={mate.name} />
            ) : (
              <ApprovalPanel permission={permission} />
            )}
          </NeedsYouFrame>
        </div>
      )}
      {!permission && <MateSuggestions mate={mate} />}
      {/* What to ask first (FR-74), until the person writes: a new mate greets first (FR-34). */}
      {!permission && !wrote && !busy && !mate.paused && (
        <div className="mx-auto flex w-full max-w-3xl justify-center px-4 pb-2">
          <Starters starters={startersFor(mate, templates)} />
        </div>
      )}
      <div hidden={Boolean(permission)}>
        <MateComposer mate={mate} reply={reply} onReplyDone={() => setReply(null)} />
      </div>
    </div>
  );
}
