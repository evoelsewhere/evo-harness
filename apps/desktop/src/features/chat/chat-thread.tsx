import * as React from "react";
import {
  MessageScroller,
  MessageScrollerProvider,
  MessageScrollerButton,
  MessageScrollerContent,
  MessageScrollerItem,
  MessageScrollerViewport,
  useMessageScroller,
} from "@/components/ui/message-scroller";
import { Button } from "@/components/ui/button";
import { useEngineStore, usePaneEngine, useRestoring } from "@/stores/engine-store";
import { useSessionStore } from "@/stores/session-store";
import { ChatSkeleton } from "./chat-skeleton";
import { AssistantMessage, UserMessage } from "./chat-turn";
import type { Turn } from "./transcript";
import { LearnedLine } from "@/components/learned-line";
import { usePaneChat } from "@/stores/engine-store";

/**
 * One turn of the thread. A delta replaces only the running turn's object, so
 * every earlier turn skips rendering while a reply streams.
 */
const ThreadTurn = React.memo(function ThreadTurn({ turn }: { turn: Turn }) {
  const chat = usePaneChat();
  return (
    <div className="flex flex-col gap-6">
      <MessageScrollerItem messageId={`${turn.id}-prompt`} scrollAnchor>
        <UserMessage turn={turn} />
      </MessageScrollerItem>
      <MessageScrollerItem messageId={`${turn.id}-reply`}>
        <AssistantMessage turn={turn} />
        {/* A learned skill changed after this reply (learning-layers FR-18). */}
        {chat && turn.status !== "running" && <LearnedLine chat={chat} turnEntry={turn.entryId} className="mt-2" />}
      </MessageScrollerItem>
    </div>
  );
});

/**
 * Whether this pane shows a new chat's empty screen: nothing said yet, nothing
 * loading and no load error. The chat view then centres the composer.
 */
export function useThreadEmpty(): boolean {
  const empty = usePaneEngine((s) => s.transcript.turns.length === 0);
  const restoring = useRestoring();
  const activeId = usePaneEngine((s) => s.activeId);
  const focused = useEngineStore((s) => s.activeId === activeId);
  const switching = useSessionStore((s) => {
    if (!focused || !s.currentId || s.currentId === activeId) return false;
    const target = s.sessions.find((chat) => chat.id === s.currentId) ?? s.archived.find((chat) => chat.id === s.currentId);
    return Boolean(target && !target.draft);
  });
  const failed = usePaneEngine((s) => (s.status === "ready" ? s.error : undefined));
  return empty && !switching && !restoring && !failed;
}

/**
 * Chats whose next new turn the thread brings into view: the person just sent
 * it, maybe while reading older turns. Auto-scroll alone only follows a thread
 * that is already at its end, and the turn appears once the engine has it.
 */
const followNext = new Set<string>();

/** Bring this chat's next turn into view when it appears (the composer sent it). */
export function followNextTurn(chatId: string | null | undefined, on = true) {
  if (!chatId) return;
  if (on) followNext.add(chatId);
  else followNext.delete(chatId);
}

/** The scrolling thread of one chat; its scroller state is fresh per mount. */
function ThreadScroller() {
  const turns = usePaneEngine((s) => s.transcript.turns);
  const busy = usePaneEngine((s) => s.transcript.busy);
  const { scrollToMessage } = useMessageScroller();
  const lastTurn = turns.at(-1)?.id;
  const chatId = usePaneEngine((s) => s.activeId);
  React.useEffect(() => {
    if (!chatId || !lastTurn || !followNext.has(chatId)) return;
    followNext.delete(chatId);
    // After the turn has laid out, so its prompt can sit at the top.
    const frame = requestAnimationFrame(() => scrollToMessage(`${lastTurn}-prompt`, { align: "start", behavior: "smooth" }));
    return () => cancelAnimationFrame(frame);
  }, [chatId, lastTurn]);
  // Quick entry's Open in Evo / Review in Evo: go to the turn, or to the
  // latest one for a question, which shows by the composer (quick-entry FR-15).
  const openTarget = useEngineStore((s) => (s.openTarget && s.openTarget.chat === chatId ? s.openTarget : null));
  React.useEffect(() => {
    if (!openTarget || openTarget.kind === "panel") return;
    const turn = openTarget.kind === "turn" ? openTarget.id : lastTurn;
    if (!turn) return;
    const frame = requestAnimationFrame(() => scrollToMessage(`${turn}-prompt`, { align: "start", behavior: "smooth" }));
    return () => cancelAnimationFrame(frame);
  }, [openTarget?.at]);
  return (
    <MessageScroller>
      <MessageScrollerViewport>
        <MessageScrollerContent aria-busy={busy} className="mx-auto w-full max-w-(--chat-width) gap-8 px-6 py-6">
          {turns.map((turn) => (
            <ThreadTurn key={turn.id} turn={turn} />
          ))}
        </MessageScrollerContent>
      </MessageScrollerViewport>
      <MessageScrollerButton />
    </MessageScroller>
  );
}

/** The thread (shadcn/ui AI Elements · Chat): the person's turn anchors the scroll. */
export function ChatThread() {
  const turns = usePaneEngine((s) => s.transcript.turns);
  const restoring = useRestoring();
  // A chat was clicked and is on its way (the sidebar shows it, the thread not yet).
  const activeId = usePaneEngine((s) => s.activeId);
  // Only the focused pane follows the sidebar's pick (split view, FR-4).
  const focused = useEngineStore((s) => s.activeId === activeId);
  const switching = useSessionStore((s) => {
    if (!focused || !s.currentId || s.currentId === activeId) return false;
    const target = s.sessions.find((chat) => chat.id === s.currentId) ?? s.archived.find((chat) => chat.id === s.currentId);
    return Boolean(target && !target.draft);
  });
  // A saved chat that could not be loaded (the engine is up but said no).
  const failed = usePaneEngine((s) => (s.status === "ready" ? s.error : undefined));

  // A saved chat is loading: its shape, not the previous chat or the empty "new chat" screen.
  if (switching || (restoring && turns.length === 0)) return <ChatSkeleton />;

  if (turns.length === 0) {
    if (failed) {
      return (
        <div className="grid h-full place-items-center px-6">
          <div className="flex max-w-md flex-col items-center gap-3 text-center">
            <p className="text-sm text-muted-foreground" data-selectable>
              {failed}
            </p>
            <Button size="sm" variant="outline" onClick={() => void useEngineStore.getState().start()}>
              Try again
            </Button>
          </div>
        </div>
      );
    }
    // The empty screen sits with the composer in the chat view (useThreadEmpty).
    return null;
  }

  // The provider applies "open at the end" only once, so each chat (and each
  // remount after loading) gets a fresh one and opens at its latest message.
  return (
    <MessageScrollerProvider key={activeId} autoScroll>
      <ThreadScroller />
    </MessageScrollerProvider>
  );
}
