// Quick entry: a floating message box over any app. Enter answers in the box:
// the first keystroke loads the chat stores and renderer
// (`@/features/quick-entry`), and from then on the box shows one chat with
// follow-ups. Ctrl+Enter hands the text to the main window as before. Esc or
// a click elsewhere hides the box and keeps its text and its chat
// (docs/features/quick-entry/spec.md, FR-5, FR-7, FR-8, FR-10, FR-11, FR-18,
// FR-21, FR-22; src-tauri/src/quick_entry.rs).

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

type BoxModule = typeof import("@/features/quick-entry");

let loading: Promise<BoxModule> | null = null;

/** The conversation module, loaded once and connected (NFR-1). */
function loadBox(): Promise<BoxModule> {
  if (!loading) {
    loading = import("@/features/quick-entry").then(async (box) => {
      await box.bootBox();
      return box;
    });
    loading.catch(() => {
      loading = null;
    });
  }
  return loading;
}

const isMac = typeof navigator !== "undefined" && navigator.platform.toLowerCase().includes("mac");

export function QuickEntry() {
  const [text, setText] = React.useState("");
  const [box, setBox] = React.useState<BoxModule | null>(null);
  /** The chat the box shows; null: empty (FR-11, FR-18). */
  const [chat, setChat] = React.useState<string | null>(null);
  const [pinned, setPinned] = React.useState(false);
  const [visible, setVisible] = React.useState(true);
  const [sending, setSending] = React.useState(false);
  const field = React.useRef<HTMLTextAreaElement | null>(null);
  const card = React.useRef<HTMLDivElement | null>(null);
  const chatRef = React.useRef<string | null>(null);
  chatRef.current = chat;

  const load = React.useCallback(() => {
    const promise = loadBox();
    void promise.then(setBox).catch(() => undefined);
    return promise;
  }, []);

  const focusField = React.useCallback(() => {
    const element = field.current;
    if (!element) return;
    element.focus();
    const end = element.value.length;
    element.setSelectionRange(end, end);
  }, []);

  // Theme now and whenever it changes; on each opening the shell says which
  // chat to show (or none), and the caret goes in the field (FR-18, FR-22).
  React.useEffect(() => {
    applyTheme();
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    media.addEventListener("change", applyTheme);
    window.addEventListener("storage", applyTheme);
    const shown = listen<{ chat?: string | null; pinned?: boolean } | null>("quick-entry://shown", ({ payload }) => {
      applyTheme();
      setVisible(true);
      setPinned(Boolean(payload?.pinned));
      const next = payload?.chat ?? null;
      if (next) {
        setChat(next);
        void load().then((loaded) => loaded.showChat(next));
      } else if (chatRef.current) {
        setChat(null);
        void load().then((loaded) => loaded.letGo(false));
      }
      requestAnimationFrame(focusField);
    });
    const hidden = listen("quick-entry://hidden", () => setVisible(false));
    // A notification of the box's chat was clicked (FR-19).
    const notice = listen<{ chat: string }>("quick-entry://notice", ({ payload }) => {
      if (payload?.chat) void load().then((loaded) => loaded.noticeClicked(payload.chat));
    });
    focusField();
    return () => {
      media.removeEventListener("change", applyTheme);
      window.removeEventListener("storage", applyTheme);
      for (const stop of [shown, hidden, notice]) void stop.then((off) => off());
    };
  }, [focusField, load]);

  // The field grows with the text: the field's own height follows its
  // content, and the window follows the card (FR-8, FR-11).
  React.useLayoutEffect(() => {
    const element = field.current;
    if (!element) return;
    element.style.height = "auto";
    element.style.height = `${element.scrollHeight}px`;
  }, [text, chat]);
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

  /** The earlier way: hide, and the main window starts the chat (Ctrl+Enter). */
  function handToMain(sent: string) {
    void invoke("quick_entry_submit", { text: sent }).catch(() => setText(sent));
  }

  async function send(toEvo: boolean) {
    const sent = text.trim();
    if (!sent || sending) return;
    if (!chat && toEvo) {
      setText("");
      handToMain(sent);
      return;
    }
    setText("");
    setSending(true);
    try {
      const loaded = await load();
      if (chat) {
        await loaded.followUp(sent);
        if (toEvo) loaded.openInEvo(chat);
      } else {
        const started = await loaded.startChat(sent);
        if (started) setChat(started);
        else handToMain(sent);
      }
    } catch {
      // The conversation could not load: the main window answers instead.
      handToMain(sent);
    } finally {
      setSending(false);
      requestAnimationFrame(focusField);
    }
  }

  function newChat() {
    if (!chat) return;
    setChat(null);
    box?.letGo();
    requestAnimationFrame(focusField);
  }

  function onKeyDown(event: React.KeyboardEvent<HTMLTextAreaElement>) {
    // Typing with an input method (Vietnamese, Japanese…): Enter picks the word.
    if (event.nativeEvent.isComposing) return;
    const mod = isMac ? event.metaKey : event.ctrlKey;
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void send(mod);
    } else if (event.key === "Escape") {
      event.preventDefault();
      void invoke("quick_entry_hide").catch(() => undefined);
    } else if (mod && !event.shiftKey && !event.altKey && event.key.toLowerCase() === "n") {
      event.preventDefault();
      newChat();
    } else if (mod && !event.shiftKey && !event.altKey && event.key.toLowerCase() === "o" && chat && box) {
      event.preventDefault();
      box.openInEvo(chat);
    }
  }

  const Conversation = box?.QuickConversation;
  const fieldElement = (
    <div className={chat ? "flex items-start gap-2" : "flex items-start gap-3"}>
      {/* The logo moves the box (FR-17). */}
      {!chat && <img src="/evo.svg" alt="" data-tauri-drag-region="" className="mt-0.5 size-5 shrink-0 select-none" draggable={false} />}
      <textarea
        ref={field}
        value={text}
        rows={1}
        autoFocus
        spellCheck
        aria-label={chat ? "Ask a follow-up" : "Ask Evo"}
        placeholder={chat ? "Ask a follow-up" : "What can Evo do for you?"}
        onChange={(event) => {
          setText(event.target.value);
          // Start loading the conversation while the person types (NFR-1).
          if (!box) void load().catch(() => undefined);
        }}
        onKeyDown={onKeyDown}
        className="scrollbar-slim max-h-[264px] min-h-6 flex-1 resize-none overflow-y-auto bg-transparent text-[15px] leading-6 outline-none placeholder:text-muted-foreground focus-visible:shadow-none"
      />
      {chat && box && <StopButton box={box} chat={chat} />}
    </div>
  );

  return (
    // The window is transparent: only the card shows.
    <div className="h-full w-full bg-transparent p-3" style={{ background: "transparent" }}>
      <div ref={card} className="overflow-hidden rounded-2xl bg-popover text-popover-foreground shadow-modal">
        {chat && Conversation ? (
          <Conversation chat={chat} pinned={pinned} visible={visible} field={fieldElement} onNewChat={newChat} />
        ) : (
          <div className="px-4 py-3">{fieldElement}</div>
        )}
      </div>
    </div>
  );
}

/** Stop the running turn (FR-11), shown once the conversation is loaded. */
function StopButton({ box, chat }: { box: BoxModule; chat: string }) {
  const running = box.useRunning(chat);
  if (!running) return null;
  return (
    <button
      type="button"
      aria-label="Stop"
      onClick={() => box.stop()}
      className="flex size-7 shrink-0 items-center justify-center rounded-full bg-foreground text-background transition-opacity hover:opacity-85"
    >
      <span className="size-2.5 rounded-[2px] bg-current" />
    </button>
  );
}
