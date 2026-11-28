// A mate's conversation (docs/features/evo-mates/spec.md, FR-58 – FR-62):
// the messaging thread, the mate's approval or question when it asks
// (Harness's cards), and the pill message box. Reactions are kept by the
// shell and shown on their bubbles; Reply puts a quote above the box.

import * as React from "react";
import { ApprovalPanel, QuestionsPanel, usePendingPermission } from "@/features/chat";
import { log } from "@/lib/db";
import { matesApi, type Mate, type ReactionKind } from "@/lib/mates";
import { MateComposer } from "./composer";
import { excerpt, type ReplyTo } from "./message-markup";
import { SecretCard } from "./secret-card";
import { Starters, startersFor } from "./starters";
import { isGreeting, isHeartbeat } from "./thread-rows";
import { useEngineStore } from "@/stores/engine-store";
import { useMatesStore } from "@/stores/mates-store";
import { MateSuggestions } from "./suggestion-card";
import { MateThread } from "./thread";

export function MateConversation({ mate, empty }: { mate: Mate; empty: React.ReactNode }) {
  const permission = usePendingPermission();
  const [reply, setReply] = React.useState<ReplyTo | null>(null);
  const [reactions, setReactions] = React.useState<Record<string, ReactionKind>>({});
  const templates = useMatesStore((s) => s.templates);
  const busy = useEngineStore((s) => s.activeId === mate.chatId && s.transcript.busy);
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
      <div className="min-h-0 flex-1">
        <MateThread
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
          {permission.kind === "secret" ? (
            <SecretCard key={permission.requestId} mate={mate} permission={permission} />
          ) : permission.kind === "questions" ? (
            <QuestionsPanel key={permission.requestId} permission={permission} asker={mate.name} />
          ) : (
            <ApprovalPanel permission={permission} />
          )}
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
