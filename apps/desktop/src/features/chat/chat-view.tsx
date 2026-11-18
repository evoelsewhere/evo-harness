import { MessageScrollerProvider } from "@/components/ui/message-scroller";
import { CardLayer, ChipRow } from "@/features/cards";
import { cn } from "@/lib/utils";
import { ApprovalPanel, usePendingPermission } from "./approval-panel";
import { ChatComposer } from "./chat-composer";
import { ChatEmpty, useChatPlace } from "./chat-empty";
import { ChatThread, useThreadEmpty } from "./chat-thread";
import { QuestionsPanel } from "./questions-panel";
import { TodoBar } from "./todo-bar";
import { EngineBanner } from "./engine-banner";

/**
 * The main column: thread above, composer pinned below. A new chat centres the
 * composer under its greeting, in a soft glow of the accent colour; the composer
 * stays the same element when the first turn moves it down, so focus and drafts hold.
 */
export function ChatView() {
  const permission = usePendingPermission();
  const empty = useThreadEmpty();
  const place = useChatPlace(empty);
  return (
    <MessageScrollerProvider autoScroll>
      <div
        className={cn(
          "@container relative flex h-full min-h-0 min-w-0 flex-col overflow-hidden",
          empty && "justify-center pb-[12vh]",
        )}
      >
        {empty && (
          <div
            aria-hidden
            className="chat-empty-glow pointer-events-none absolute inset-0"
          />
        )}
        <div className={cn("relative min-h-0", empty ? "shrink-0" : "flex-1")}>
          {empty ? <ChatEmpty place={place} /> : <ChatThread />}
        </div>
        <div className={cn("relative mx-auto w-full max-w-(--chat-width) px-6", !empty && "pb-5")}>
          <EngineBanner />
          {/* A permission request takes the composer's place until it is answered. */}
          {/* (The composer stays mounted so a draft survives the prompt.) */}
          <TodoBar />
          <ChipRow />
          {permission?.kind === "questions" ? (
            <QuestionsPanel key={permission.requestId} permission={permission} />
          ) : (
            permission && <ApprovalPanel permission={permission} />
          )}
          <div hidden={Boolean(permission)}>
            <ChatComposer />
          </div>
        </div>
        <CardLayer />
      </div>
    </MessageScrollerProvider>
  );
}
