// Quick entry: a floating message box over any app. Enter hands the text to
// the shell, which has the main window start a new chat with it; Esc or a
// click elsewhere hides the box and keeps the text
// (docs/features/quick-entry/spec.md, FR-5, FR-7, FR-8, FR-10;
// src-tauri/src/quick_entry.rs).

import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** The colour scheme the main window chose (`theme-provider.tsx`). */
function applyTheme() {
  let theme: string | null = null;
  try {
    theme = localStorage.getItem("evo.theme");
  } catch {
    // Storage can be unavailable: follow the system.
  }
  const dark = theme === "dark" || (theme !== "light" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.classList.toggle("dark", dark);
  document.documentElement.style.colorScheme = dark ? "dark" : "light";
}

/** The window's margin around the card (`m-3` on each side). */
const MARGIN = 24;

export function QuickEntry() {
  const [text, setText] = React.useState("");
  const field = React.useRef<HTMLTextAreaElement | null>(null);
  const card = React.useRef<HTMLDivElement | null>(null);

  const focusField = React.useCallback(() => {
    const element = field.current;
    if (!element) return;
    element.focus();
    const end = element.value.length;
    element.setSelectionRange(end, end);
  }, []);

  // Theme now and whenever it changes; the caret goes in the field each time the box shows.
  React.useEffect(() => {
    applyTheme();
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    media.addEventListener("change", applyTheme);
    window.addEventListener("storage", applyTheme);
    const shown = listen("quick-entry://shown", () => {
      applyTheme();
      focusField();
    });
    focusField();
    return () => {
      media.removeEventListener("change", applyTheme);
      window.removeEventListener("storage", applyTheme);
      void shown.then((stop) => stop());
    };
  }, [focusField]);

  // The field grows with the text: the field's own height follows its
  // content, and the window follows the card (FR-8).
  React.useLayoutEffect(() => {
    const element = field.current;
    if (!element) return;
    element.style.height = "auto";
    element.style.height = `${element.scrollHeight}px`;
  }, [text]);
  React.useEffect(() => {
    const element = card.current;
    if (!element) return;
    let last = 0;
    const observer = new ResizeObserver(() => {
      const height = Math.ceil(element.getBoundingClientRect().height) + MARGIN;
      if (height === last) return;
      last = height;
      void invoke("quick_entry_resize", { height }).catch(() => undefined);
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  function send() {
    if (!text.trim()) return;
    const sent = text;
    setText("");
    void invoke("quick_entry_submit", { text: sent }).catch(() => setText(sent));
  }

  function onKeyDown(event: React.KeyboardEvent<HTMLTextAreaElement>) {
    // Typing with an input method (Vietnamese, Japanese…): Enter picks the word.
    if (event.nativeEvent.isComposing) return;
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      send();
    } else if (event.key === "Escape") {
      event.preventDefault();
      void invoke("quick_entry_hide").catch(() => undefined);
    }
  }

  return (
    // The window is transparent: only the card shows.
    <div className="h-full w-full bg-transparent p-3" style={{ background: "transparent" }}>
      <div
        ref={card}
        className="flex items-start gap-3 rounded-2xl bg-popover px-4 py-3 text-popover-foreground shadow-modal"
      >
        <img src="/evo.svg" alt="" className="mt-0.5 size-5 shrink-0 select-none" draggable={false} />
        <textarea
          ref={field}
          value={text}
          rows={1}
          autoFocus
          spellCheck
          aria-label="Ask Evo"
          placeholder="What can Evo do for you?"
          onChange={(event) => setText(event.target.value)}
          onKeyDown={onKeyDown}
          className="scrollbar-slim max-h-[264px] min-h-6 flex-1 resize-none overflow-y-auto bg-transparent text-[15px] leading-6 outline-none placeholder:text-muted-foreground focus-visible:shadow-none"
        />
      </div>
    </div>
  );
}
