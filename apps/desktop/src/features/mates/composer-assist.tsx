// What both Evo Mates message boxes share (docs/features/evo-mates-messaging/
// spec.md, FR-12, FR-13): `:shortcode` completion, the `/` command menu,
// Ctrl+B / Ctrl+I / Ctrl+E Markdown keys, ↑ to edit the last message, and
// the emoji picker.

import * as React from "react";
import { SmileIcon } from "lucide-react";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { modKey } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { shortcodeAt, slashQuery, wrapSelection } from "./composer-keys";
import { EMOJI, GROUPS, searchEmoji, type Emoji } from "./emoji";
import { matchSlash, parseSlash, type SlashCommand } from "./slash-commands";

type Menu = { kind: "emoji"; start: number; items: Emoji[] } | { kind: "slash"; items: SlashCommand[] };

export function useComposerAssist({
  where,
  text,
  setText,
  field,
  onCommand,
  onEditLast,
}: {
  where: "mate" | "channel";
  text: string;
  setText(text: string): void;
  field: React.RefObject<HTMLTextAreaElement | null>;
  /** Run a typed command; the box is cleared after. */
  onCommand(command: SlashCommand, rest: string): void;
  /** ↑ in an empty box. */
  onEditLast?(): void;
}) {
  const [menu, setMenu] = React.useState<Menu | null>(null);
  const [pick, setPick] = React.useState(0);

  const update = (next: string, caret: number) => {
    const slash = slashQuery(next);
    if (slash !== null) {
      const items = matchSlash(slash, where);
      setMenu(items.length ? { kind: "slash", items } : null);
      setPick(0);
      return;
    }
    const code = shortcodeAt(next, caret);
    if (code) {
      const items = searchEmoji(code.query, 6);
      setMenu(items.length ? { kind: "emoji", start: code.start, items } : null);
      setPick(0);
      return;
    }
    setMenu(null);
  };

  const place = (next: string, caret: number) => {
    setText(next);
    requestAnimationFrame(() => {
      field.current?.focus();
      field.current?.setSelectionRange(caret, caret);
    });
  };

  const insert = (emoji: string) => {
    const el = field.current;
    const start = el?.selectionStart ?? text.length;
    const end = el?.selectionEnd ?? text.length;
    place(text.slice(0, start) + emoji + text.slice(end), start + emoji.length);
  };

  const choose = (index: number) => {
    if (!menu) return;
    if (menu.kind === "emoji") {
      const item = menu.items[index];
      const caret = field.current?.selectionStart ?? text.length;
      if (item) place(`${text.slice(0, menu.start)}${item.e} ${text.slice(caret)}`, menu.start + item.e.length + 1);
    } else {
      const item = menu.items[index];
      if (!item) return;
      if (item.args) place(`/${item.id} `, item.id.length + 2);
      else {
        setText("");
        onCommand(item, "");
      }
    }
    setMenu(null);
  };

  /** True when the key was the assist's. */
  const onKeyDown = (event: React.KeyboardEvent<HTMLTextAreaElement>): boolean => {
    if (menu) {
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        setPick((p) => (p + (event.key === "ArrowDown" ? 1 : menu.items.length - 1)) % menu.items.length);
        return true;
      }
      if (event.key === "Enter" || event.key === "Tab") {
        event.preventDefault();
        choose(pick);
        return true;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        setMenu(null);
        return true;
      }
    }
    const mod = modKey === "⌘" ? event.metaKey : event.ctrlKey;
    const mark = mod && !event.altKey ? ({ b: "**", i: "_", e: "`" } as Record<string, string>)[event.key.toLowerCase()] : undefined;
    if (mark) {
      // The window's Ctrl+B / Ctrl+I (fold, profile) must not also run.
      event.preventDefault();
      event.stopPropagation();
      const el = event.currentTarget;
      const next = wrapSelection(text, el.selectionStart, el.selectionEnd, mark);
      setText(next.text);
      requestAnimationFrame(() => el.setSelectionRange(next.start, next.end));
      return true;
    }
    if (event.key === "ArrowUp" && !text && onEditLast && !event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey) {
      event.preventDefault();
      onEditLast();
      return true;
    }
    if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
      const typed = parseSlash(text, where);
      if (typed && (!typed.command.args || typed.rest)) {
        event.preventDefault();
        setText("");
        onCommand(typed.command, typed.rest);
        return true;
      }
    }
    return false;
  };

  const menuView = menu ? (
    <div
      role="listbox"
      aria-label={menu.kind === "emoji" ? "Emoji" : "Commands"}
      className="absolute bottom-full left-4 z-20 mb-1 w-80 overflow-hidden rounded-2xl bg-popover p-1 shadow-lg ring-1 ring-border"
    >
      {menu.kind === "emoji"
        ? menu.items.map((item, i) => (
            <button
              key={item.n}
              type="button"
              role="option"
              aria-selected={i === pick}
              onMouseDown={(event) => {
                event.preventDefault();
                choose(i);
              }}
              className={cn("flex w-full items-center gap-2.5 rounded-xl px-2 py-1.5 text-left text-[13px]", i === pick && "bg-accent")}
            >
              <span className="text-base">{item.e}</span>
              <span className="text-muted-foreground">:{item.n}:</span>
            </button>
          ))
        : menu.items.map((item, i) => (
            <button
              key={item.id}
              type="button"
              role="option"
              aria-selected={i === pick}
              onMouseDown={(event) => {
                event.preventDefault();
                choose(i);
              }}
              className={cn("flex w-full flex-col items-start rounded-xl px-2.5 py-1.5 text-left", i === pick && "bg-accent")}
            >
              <span className="font-mono text-[12.5px] text-foreground">{item.usage}</span>
              <span className="text-xs text-muted-foreground">{item.hint}</span>
            </button>
          ))}
    </div>
  ) : null;

  return { update, onKeyDown, menuView, insert, closeMenu: () => setMenu(null) };
}

/** The emoji button and its picker (FR-12). */
export function EmojiButton({ onPick }: { onPick(emoji: string): void }) {
  const [open, setOpen] = React.useState(false);
  const [query, setQuery] = React.useState("");
  const shown = query.trim() ? searchEmoji(query, 48) : null;
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger
        render={
          <button
            type="button"
            aria-label="Emoji"
            className="my-1 flex size-7 shrink-0 items-center justify-center rounded-full text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          />
        }
      >
        <SmileIcon className="size-4" />
      </PopoverTrigger>
      <PopoverContent side="top" align="end" className="w-80 p-2">
        <input
          autoFocus
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="Search emoji"
          aria-label="Search emoji"
          className="mb-2 h-8 w-full rounded-lg bg-muted px-2.5 text-[13px] outline-none placeholder:text-muted-foreground"
        />
        <div className="max-h-64 overflow-x-hidden overflow-y-auto">
          {(shown ? [{ group: "Found", items: shown }] : GROUPS.map((group) => ({ group, items: EMOJI.filter((e) => e.g === group) }))).map(({ group, items }) => (
            <section key={group} className="mb-1">
              <h4 className="px-1 py-1 text-[11px] font-medium text-muted-foreground">{group}</h4>
              <div className="grid grid-cols-8 gap-0.5">
                {items.map((item) => (
                  <button
                    key={item.n}
                    type="button"
                    title={`:${item.n}:`}
                    aria-label={item.n.replace(/_/g, " ")}
                    onClick={() => {
                      onPick(item.e);
                      setOpen(false);
                      setQuery("");
                    }}
                    className="flex size-8 items-center justify-center rounded-lg text-lg hover:bg-accent"
                  >
                    {item.e}
                  </button>
                ))}
              </div>
            </section>
          ))}
        </div>
      </PopoverContent>
    </Popover>
  );
}
