import * as React from "react";
import { ContextMenu, ContextMenuContent, ContextMenuTrigger } from "@/components/ui/context-menu";
import { DropdownMenuItem, DropdownMenuSeparator } from "@/components/ui/dropdown-menu";
import { usePaneChat, useEngineStore } from "@/stores/engine-store";
import { useSessionStore } from "@/stores/session-store";
import { composerDraft } from "./chat-composer";
import { messagePlainText, quoted, replyMarkdown } from "./message-text";
import type { Turn } from "./transcript";

// A chat message's right-click menu: copy it (as text or Markdown, or just
// the selected part), quote it in the composer, edit it (the person's own),
// and fork the chat from it.

interface MessageMenuProps {
  turn: Turn;
  /** The message as Markdown (a reply), or as the person typed it (a prompt). */
  text: string;
  /** A reply: offer "Copy message as Markdown" beside the plain text. */
  markdown?: boolean;
  /** The person's message: offer Edit. */
  onEdit?: () => void;
  editDisabled?: boolean;
  children: React.ReactNode;
}

/** The text selected inside `root`, if any. */
function selectionIn(root: HTMLElement | null): string {
  const selection = window.getSelection();
  if (!root || !selection || selection.isCollapsed || selection.rangeCount === 0) return "";
  const range = selection.getRangeAt(0);
  if (!root.contains(range.commonAncestorContainer)) return "";
  return selection.toString().trim();
}

export function MessageMenu({ turn, text, markdown, onEdit, editDisabled, children }: MessageMenuProps) {
  const ref = React.useRef<HTMLDivElement>(null);
  const [selected, setSelected] = React.useState("");
  const chatId = usePaneChat();
  // An incognito chat leaves no copy of itself; a draft has nothing saved to copy.
  const forkable = useSessionStore((s) => {
    const chat = s.sessions.find((x) => x.id === chatId);
    return Boolean(chat && !chat.incognito && !chat.draft);
  });
  const plain = React.useMemo(() => (markdown ? messagePlainText(text) : text), [markdown, text]);
  const copy = (value: string) => void navigator.clipboard.writeText(value);
  const quote = (value: string) => composerDraft.insert(quoted(value));

  return (
    <ContextMenu onOpenChange={(open) => open && setSelected(selectionIn(ref.current))}>
      <ContextMenuTrigger ref={ref} className="contents">
        {children}
      </ContextMenuTrigger>
      <ContextMenuContent className="w-60">
        {selected && (
          <>
            <DropdownMenuItem onClick={() => copy(selected)}>Copy</DropdownMenuItem>
            <DropdownMenuItem onClick={() => quote(selected)}>Quote selection</DropdownMenuItem>
            <DropdownMenuSeparator />
          </>
        )}
        <DropdownMenuItem disabled={!plain} onClick={() => copy(plain)}>
          Copy message
        </DropdownMenuItem>
        {markdown && (
          <DropdownMenuItem disabled={!text} onClick={() => copy(replyMarkdown(text))}>
            Copy message as Markdown
          </DropdownMenuItem>
        )}
        <DropdownMenuSeparator />
        <DropdownMenuItem disabled={!text} onClick={() => quote(markdown ? replyMarkdown(text) : text)}>
          Quote in composer
        </DropdownMenuItem>
        {onEdit && (
          <DropdownMenuItem disabled={editDisabled} onClick={onEdit}>
            {editDisabled ? "Edit (when the agent is done)" : "Edit message"}
          </DropdownMenuItem>
        )}
        {forkable && chatId && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              disabled={turn.status === "running"}
              onClick={() => void useEngineStore.getState().forkChatAt(chatId, turn.id)}
            >
              <span className="flex flex-col">
                <span>Fork from here</span>
                <span className="text-xs text-muted-foreground">Starts a new chat, keeps this one</span>
              </span>
            </DropdownMenuItem>
          </>
        )}
      </ContextMenuContent>
    </ContextMenu>
  );
}
