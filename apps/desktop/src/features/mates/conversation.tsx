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
import { MateSuggestions } from "./suggestion-card";
import { MateThread } from "./thread";

export function MateConversation({ mate, empty }: { mate: Mate; empty: React.ReactNode }) {
  const permission = usePendingPermission();
  const [reply, setReply] = React.useState<ReplyTo | null>(null);
  const [reactions, setReactions] = React.useState<Record<string, ReactionKind>>({});

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
      <div hidden={Boolean(permission)}>
        <MateComposer mate={mate} reply={reply} onReplyDone={() => setReply(null)} />
      </div>
    </div>
  );
}
