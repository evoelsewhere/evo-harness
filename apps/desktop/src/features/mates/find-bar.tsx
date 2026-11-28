// The find bar (docs/features/evo-mates-ui-v2/spec.md, FR-15): over the
// shown conversation or channel; marks every match with the browser's text
// highlights (`::highlight(mates-find)`), shows "n of m", and Enter /
// Shift+Enter step through the matches. Esc closes it and clears the marks.

import * as React from "react";
import { ChevronDownIcon, ChevronUpIcon, SearchIcon, XIcon } from "lucide-react";
import { useMatesUiStore } from "@/stores/mates-ui-store";
import { findLabel, matchOffsets, stepMatch } from "./find";

interface HighlightRegistry {
  set(name: string, value: unknown): void;
  delete(name: string): void;
}
type HighlightCtor = new (...ranges: Range[]) => unknown;

function registry(): { highlights: HighlightRegistry; Highlight: HighlightCtor } | null {
  const css = (globalThis as { CSS?: { highlights?: HighlightRegistry } }).CSS;
  const Highlight = (globalThis as { Highlight?: HighlightCtor }).Highlight;
  return css?.highlights && Highlight ? { highlights: css.highlights, Highlight } : null;
}

/** Ranges of every match inside the messages of `scope` (`[data-find-text]` elements). */
function rangesIn(scope: Element, query: string): Range[] {
  const ranges: Range[] = [];
  for (const block of scope.querySelectorAll("[data-find-text]")) {
    const walker = document.createTreeWalker(block, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const text = node.textContent ?? "";
      for (const at of matchOffsets(text, query)) {
        const range = document.createRange();
        range.setStart(node, at);
        range.setEnd(node, at + query.trim().length);
        ranges.push(range);
      }
    }
  }
  return ranges;
}

function clearMarks() {
  const reg = registry();
  reg?.highlights.delete("mates-find");
  reg?.highlights.delete("mates-find-current");
}

/** The bar; `version` changes when the shown messages change, so the marks follow. */
export function FindBar({ scopeId, version }: { scopeId: string; version: unknown }) {
  const open = useMatesUiStore((s) => s.findOpen);
  const setOpen = useMatesUiStore((s) => s.setFindOpen);
  const [query, setQuery] = React.useState("");
  const [index, setIndex] = React.useState(0);
  const [total, setTotal] = React.useState(0);
  const field = React.useRef<HTMLInputElement>(null);

  React.useEffect(() => {
    const focus = () => field.current?.select();
    window.addEventListener("mates:focus-find", focus);
    return () => window.removeEventListener("mates:focus-find", focus);
  }, []);

  React.useEffect(() => {
    if (open) requestAnimationFrame(() => field.current?.focus());
    else {
      setQuery("");
      setIndex(0);
      clearMarks();
    }
  }, [open]);

  // Close when the conversation changes.
  React.useEffect(() => () => setOpen(false), [scopeId, setOpen]);

  React.useEffect(() => {
    if (!open) return;
    const scope = document.getElementById(scopeId);
    const reg = registry();
    if (!scope || !reg || !query.trim()) {
      setTotal(0);
      clearMarks();
      return;
    }
    const ranges = rangesIn(scope, query);
    setTotal(ranges.length);
    const current = ranges[Math.min(index, ranges.length - 1)];
    reg.highlights.set("mates-find", new reg.Highlight(...ranges));
    if (current) {
      reg.highlights.set("mates-find-current", new reg.Highlight(current));
      const element = current.startContainer.parentElement;
      element?.scrollIntoView({ block: "center" });
    } else reg.highlights.delete("mates-find-current");
  }, [open, query, index, scopeId, version]);

  React.useEffect(() => clearMarks, []);

  if (!open) return null;
  const label = findLabel(Math.min(index, Math.max(total - 1, 0)), total, query);
  return (
    <div
      role="search"
      className="absolute top-2 right-3 z-20 flex h-9 items-center gap-1 rounded-full bg-popover pr-1 pl-3 shadow-lg ring-1 ring-foreground/10 animate-in fade-in slide-in-from-top-1 duration-150"
    >
      <SearchIcon className="size-3.5 shrink-0 text-muted-foreground" />
      <input
        ref={field}
        value={query}
        aria-label="Find in this conversation"
        placeholder="Find…"
        onChange={(event) => {
          setQuery(event.target.value);
          setIndex(0);
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            setIndex((i) => stepMatch(i, total, event.shiftKey ? -1 : 1));
          } else if (event.key === "Escape") {
            event.preventDefault();
            event.stopPropagation();
            setOpen(false);
          }
        }}
        className="w-40 bg-transparent text-[13px] text-foreground outline-none placeholder:text-muted-foreground"
      />
      <span aria-live="polite" className="min-w-14 text-right text-[11px] text-muted-foreground tabular-nums">
        {label}
      </span>
      <button
        type="button"
        aria-label="Previous match"
        disabled={total === 0}
        onClick={() => setIndex((i) => stepMatch(i, total, -1))}
        className="flex size-7 items-center justify-center rounded-full text-muted-foreground hover:bg-muted hover:text-foreground disabled:opacity-40"
      >
        <ChevronUpIcon className="size-4" />
      </button>
      <button
        type="button"
        aria-label="Next match"
        disabled={total === 0}
        onClick={() => setIndex((i) => stepMatch(i, total, 1))}
        className="flex size-7 items-center justify-center rounded-full text-muted-foreground hover:bg-muted hover:text-foreground disabled:opacity-40"
      >
        <ChevronDownIcon className="size-4" />
      </button>
      <button
        type="button"
        aria-label="Close find"
        onClick={() => setOpen(false)}
        className="flex size-7 items-center justify-center rounded-full text-muted-foreground hover:bg-muted hover:text-foreground"
      >
        <XIcon className="size-3.5" />
      </button>
    </div>
  );
}
