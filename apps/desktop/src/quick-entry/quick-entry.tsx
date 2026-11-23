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
import type { PromptAttachment } from "@/components/ai-elements/prompt-input";

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

/**
 * No margin around the card: the window is the card, framed by the system's
 * shadow, so nothing invisible around it takes clicks.
 */
const MARGIN = 0;

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
  /** The person resized the box: it fills the window (FR-23). */
  const [fixed, setFixed] = React.useState(false);
  const [visible, setVisible] = React.useState(true);
  const [sending, setSending] = React.useState(false);
  /** Pictures and files to send with the message (FR-24). */
  const [files, setFiles] = React.useState<PromptAttachment[]>([]);
  const [dropping, setDropping] = React.useState(false);
  const [problem, setProblem] = React.useState<string | null>(null);
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
    const shown = listen<{ chat?: string | null; pinned?: boolean; fixed?: boolean } | null>("quick-entry://shown", ({ payload }) => {
      applyTheme();
      setVisible(true);
      setPinned(Boolean(payload?.pinned));
      setFixed(Boolean(payload?.fixed));
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
    const sized = listen<{ fixed?: boolean }>("quick-entry://sized", ({ payload }) => setFixed(Boolean(payload?.fixed)));
    // A notification of the box's chat was clicked (FR-19).
    const notice = listen<{ chat: string }>("quick-entry://notice", ({ payload }) => {
      if (payload?.chat) void load().then((loaded) => loaded.noticeClicked(payload.chat));
    });
    focusField();
    return () => {
      media.removeEventListener("change", applyTheme);
      window.removeEventListener("storage", applyTheme);
      for (const stop of [shown, hidden, sized, notice]) void stop.then((off) => off());
    };
  }, [focusField, load]);

  /** Add attachments, skipping one already attached. */
  const addFiles = React.useCallback((list: PromptAttachment[]) => {
    if (list.length === 0) return;
    setProblem(null);
    setFiles((current) => [...current, ...list.filter((file) => !current.some((c) => c.id === file.id))]);
  }, []);

  function removeFile(id: string) {
    setFiles((current) => {
      const gone = current.find((file) => file.id === id);
      if (gone) box?.releaseAttachment(gone);
      return current.filter((file) => file.id !== id);
    });
  }

  /**
   * Pictures and files from the clipboard (FR-24). They are copied at once:
   * the clipboard is gone when the event returns, and the module that reads
   * them may still be loading. Very long text becomes an attachment.
   */
  function onPaste(event: React.ClipboardEvent<HTMLTextAreaElement>) {
    const data = event.clipboardData;
    const picked = [...data.files];
    if (picked.length === 0) {
      for (const item of data.items) {
        const file = item.kind === "file" ? item.getAsFile() : null;
        if (file) picked.push(file);
      }
    }
    if (picked.length > 0) {
      event.preventDefault();
      const copy = new DataTransfer();
      for (const file of picked) copy.items.add(file);
      void load()
        .then((loaded) => loaded.attachmentsFromTransfer(copy))
        .then(addFiles)
        .catch((error) => setProblem(String(error)));
      return;
    }
    const pasted = data.getData("text/plain");
    if (pasted && box?.isLongPaste(pasted)) {
      event.preventDefault();
      addFiles([box.textAttachment(pasted)]);
    }
  }

  /**
   * Files dropped on the box from the system attach to the message (FR-24).
   * They arrive as an HTML drop: the files are copied at once, as for a paste.
   */
  function onDrop(event: React.DragEvent<HTMLDivElement>) {
    setDropping(false);
    const picked = [...event.dataTransfer.files];
    if (picked.length === 0) return;
    event.preventDefault();
    const copy = new DataTransfer();
    for (const file of picked) copy.items.add(file);
    void load()
      .then((loaded) => loaded.attachmentsFromTransfer(copy))
      .then(addFiles)
      .catch((error) => setProblem(String(error)));
    requestAnimationFrame(focusField);
  }

  function onDragOver(event: React.DragEvent<HTMLDivElement>) {
    if (![...event.dataTransfer.types].includes("Files")) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = "copy";
    setDropping(true);
  }

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
    const sentFiles = files;
    if ((!sent && sentFiles.length === 0) || sending) return;
    // Text alone goes to the main window the earlier way; with pictures or
    // files the box starts the chat and then opens it there.
    if (!chat && toEvo && sentFiles.length === 0) {
      setText("");
      handToMain(sent);
      return;
    }
    setText("");
    setFiles([]);
    setProblem(null);
    setSending(true);
    try {
      const loaded = await load();
      if (chat) {
        await loaded.followUp(sent, sentFiles);
        if (toEvo) loaded.openInEvo(chat);
      } else {
        const started = await loaded.startChat(sent, sentFiles);
        if (started) {
          setChat(started);
          if (toEvo) loaded.openInEvo(started);
        } else if (sentFiles.length === 0) handToMain(sent);
        else throw new Error("Could not start a chat.");
      }
    } catch (error) {
      if (sentFiles.length === 0 && !chat) {
        // The conversation could not load: the main window answers instead.
        handToMain(sent);
      } else {
        // Put everything back so nothing is lost.
        setText((current) => current || sent);
        setFiles((current) => (current.length ? current : sentFiles));
        setProblem(error instanceof Error ? error.message : String(error));
      }
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
    <div className="flex flex-col gap-2">
      {files.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {files.map((file) => (
            <span
              key={file.id}
              title={file.path ?? file.name}
              className="flex h-8 max-w-56 items-center gap-1.5 rounded-md bg-muted pr-1 pl-1 text-xs text-muted-foreground"
            >
              {file.kind === "image" && file.preview ? (
                <img src={file.preview} alt="" className="size-6 shrink-0 rounded object-cover" />
              ) : (
                <span aria-hidden className="flex size-6 shrink-0 items-center justify-center rounded bg-background text-[10px] font-medium uppercase">
                  {file.kind === "text" ? "txt" : (file.name.split(".").pop() ?? "").slice(0, 3)}
                </span>
              )}
              <span className="min-w-0 truncate">{file.name}</span>
              <button
                type="button"
                aria-label={`Remove ${file.name}`}
                onClick={() => removeFile(file.id)}
                className="flex size-5 shrink-0 items-center justify-center rounded hover:bg-background hover:text-foreground"
              >
                ×
              </button>
            </span>
          ))}
        </div>
      )}
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
          onPaste={onPaste}
          className="scrollbar-slim max-h-[264px] min-h-6 flex-1 resize-none overflow-y-auto bg-transparent text-[15px] leading-6 outline-none placeholder:text-muted-foreground focus-visible:shadow-none"
        />
        {chat && box && <StopButton box={box} chat={chat} />}
      </div>
      {problem && (
        <p role="alert" className="text-xs text-destructive">
          {problem}
        </p>
      )}
    </div>
  );

  return (
    // The window is transparent: only the card shows.
    <div
      className="h-full w-full bg-transparent"
      style={{ background: "transparent" }}
      onDragOver={onDragOver}
      onDragLeave={(event) => {
        // Left the box, not moved between its parts.
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDropping(false);
      }}
      onDrop={onDrop}
    >
      <div
        ref={card}
        className={
          (chat && fixed
            ? "flex h-full flex-col overflow-hidden rounded-lg bg-popover text-popover-foreground"
            : "overflow-hidden rounded-lg bg-popover text-popover-foreground") +
          // Files held over the box (FR-24).
          (dropping ? " ring-2 ring-inset ring-brand/50" : "")
        }
      >
        {chat && Conversation ? (
          <Conversation chat={chat} pinned={pinned} visible={visible} fill={fixed} field={fieldElement} onNewChat={newChat} />
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
