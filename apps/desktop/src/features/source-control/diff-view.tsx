// A file's diff in the Source control tool: inline (one editor, the old lines
// shown above the new) or side by side, read-only, with the file's syntax
// colours and unchanged stretches folded. Each git hunk carries its actions
// in a bar above its first line; selecting lines offers a comment for the
// agent (docs/features/source-control/spec.md, FR-2, FR-4, FR-5, FR-8; plan 3.5).

import * as React from "react";
import { LanguageDescription, syntaxHighlighting } from "@codemirror/language";
import { languages } from "@codemirror/language-data";
import { MergeView, unifiedMergeView } from "@codemirror/merge";
import { Compartment, EditorState, StateEffect, StateField, type Extension } from "@codemirror/state";
import { Decoration, EditorView, WidgetType, lineNumbers, type DecorationSet } from "@codemirror/view";
import { highlight, theme } from "@/lib/codemirror-style";
import type { Hunk } from "@/lib/scm";

/** What a hunk's bar offers; `null`: no bar (a turn's diff). */
export interface HunkActions {
  primary: { label: string; run(hunk: Hunk): void };
  discard?: { label: string; run(hunk: Hunk): void };
}

/** Lines the person selected in the new text. */
export interface LineSelection {
  from: number;
  to: number;
  text: string;
}

const diffTheme = EditorView.theme({
  ".cm-content": { paddingBottom: "24px" },
  ".cm-changedLine, .cm-insertedLine": { backgroundColor: "color-mix(in oklab, var(--color-status-ok) 9%, transparent) !important" },
  ".cm-deletedChunk, .cm-deletedLine": { backgroundColor: "color-mix(in oklab, var(--color-status-error) 10%, transparent) !important" },
  // Inline marks only inside changed lines; a wholly new line keeps the line tint alone.
  ".cm-changedLine .cm-changedText": { background: "color-mix(in oklab, var(--color-status-ok) 24%, transparent) !important" },
  ".cm-insertedLine .cm-changedText, .cm-insertedText": { background: "transparent !important" },
  ".cm-deletedText": { background: "color-mix(in oklab, var(--color-status-error) 24%, transparent) !important" },
  ".cm-collapsedLines": {
    color: "var(--muted-foreground)",
    background: "color-mix(in oklab, var(--muted) 70%, transparent)",
    fontFamily: "var(--app-font-sans)",
    fontSize: "11px",
  },
  ".cm-hunk-bar": {
    display: "flex",
    alignItems: "center",
    gap: "8px",
    padding: "3px 8px",
    marginTop: "6px",
    fontFamily: "var(--app-font-sans)",
    fontSize: "11px",
    color: "var(--muted-foreground)",
    borderTop: "1px solid var(--border)",
  },
  ".cm-hunk-bar button": {
    border: "1px solid var(--border)",
    borderRadius: "6px",
    padding: "0 8px",
    lineHeight: "18px",
    background: "var(--background)",
    color: "var(--foreground)",
    cursor: "pointer",
  },
  ".cm-hunk-bar button:hover": { background: "var(--accent)" },
  ".cm-hunk-bar button:disabled": { opacity: "0.5", cursor: "default" },
});

/** The bar above a hunk, with its buttons. */
class HunkBar extends WidgetType {
  constructor(
    readonly hunk: Hunk,
    readonly actions: React.RefObject<{ actions: HunkActions | null; busy: boolean }>,
  ) {
    super();
  }
  eq(other: HunkBar) {
    return other.hunk.id === this.hunk.id;
  }
  toDOM() {
    const bar = document.createElement("div");
    bar.className = "cm-hunk-bar";
    const label = document.createElement("span");
    const { oldStart, oldLines, newStart, newLines } = this.hunk;
    // Git's hunk ranges include the context lines around the change.
    label.textContent = newLines ? `Lines ${newStart}–${newStart + newLines - 1}` : `After line ${newStart}`;
    label.title = `Old lines ${oldStart}–${oldStart + Math.max(oldLines, 1) - 1}`;
    bar.append(label);
    const current = this.actions.current;
    for (const action of [current?.actions?.primary, current?.actions?.discard]) {
      if (!action) continue;
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = action.label;
      button.disabled = Boolean(current?.busy);
      button.onmousedown = (event) => event.preventDefault();
      button.onclick = () => action.run(this.hunk);
      bar.append(button);
    }
    return bar;
  }
  ignoreEvent() {
    return true;
  }
}

const setHunks = StateEffect.define<DecorationSet>();
const hunkBars = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(value, tr) {
    for (const effect of tr.effects) if (effect.is(setHunks)) return effect.value;
    return value.map(tr.changes);
  },
  provide: (field) => EditorView.decorations.from(field),
});

function hunkDecorations(state: EditorState, hunks: Hunk[], actions: HunkBar["actions"]): DecorationSet {
  if (!actions.current?.actions) return Decoration.none;
  const ranges = hunks.map((hunk) => {
    const line = state.doc.line(Math.min(Math.max(1, hunk.newStart), state.doc.lines));
    return Decoration.widget({ widget: new HunkBar(hunk, actions), block: true, side: -1 }).range(line.from);
  });
  return Decoration.set(ranges, true);
}

const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;

interface Props {
  path: string;
  old: string;
  neu: string;
  hunks: Hunk[];
  layout: "inline" | "split";
  actions: HunkActions | null;
  busy: boolean;
  onSelect?(selection: LineSelection | null): void;
}

/** Side by side needs this much width (px); a narrower panel shows the diff inline. */
const SPLIT_MIN_WIDTH = 520;

export function DiffView({ path, old, neu, hunks, layout: wanted, actions, busy, onSelect }: Props) {
  const host = React.useRef<HTMLDivElement>(null);
  const [narrow, setNarrow] = React.useState(false);
  React.useEffect(() => {
    const node = host.current;
    if (!node) return;
    const observer = new ResizeObserver(([entry]) => setNarrow(entry.contentRect.width < SPLIT_MIN_WIDTH));
    observer.observe(node);
    return () => observer.disconnect();
  }, []);
  const layout = wanted === "split" && !narrow ? "split" : "inline";
  const editor = React.useRef<EditorView | null>(null);
  const language = React.useRef(new Compartment());
  const live = React.useRef<{ actions: HunkActions | null; busy: boolean }>({ actions, busy });
  live.current = { actions, busy };
  const selectRef = React.useRef(onSelect);
  selectRef.current = onSelect;

  // One view per file, text and layout; hunk bars follow the hunks.
  React.useLayoutEffect(() => {
    const parent = host.current;
    if (!parent) return;
    const common = (): Extension[] => [
      lineNumbers(),
      EditorState.readOnly.of(true),
      EditorView.editable.of(false),
      syntaxHighlighting(highlight),
      theme,
      diffTheme,
      // Long lines wrap: the hunk bars and their buttons stay in view in a narrow panel.
      EditorView.lineWrapping,
      language.current.of([]),
    ];
    const selection = EditorView.updateListener.of((update) => {
      if (!update.selectionSet) return;
      const range = update.state.selection.main;
      if (range.empty) return selectRef.current?.(null);
      const doc = update.state.doc;
      const first = doc.lineAt(range.from);
      const last = doc.lineAt(range.to === doc.lineAt(range.to).from && range.to > range.from ? range.to - 1 : range.to);
      selectRef.current?.({ from: first.number, to: last.number, text: doc.sliceString(first.from, last.to) });
    });
    let views: EditorView[];
    let target: EditorView;
    if (layout === "split") {
      const merge = new MergeView({
        a: { doc: old, extensions: common() },
        b: { doc: neu, extensions: [...common(), hunkBars, selection] },
        parent,
        highlightChanges: true,
        gutter: true,
        collapseUnchanged: { margin: 3, minSize: 6 },
      });
      views = [merge.a, merge.b];
      target = merge.b;
      editor.current = target;
      const loaded = LanguageDescription.matchFilename(languages, fileName(path));
      if (loaded) {
        void loaded.load().then((support) => {
          if (editor.current !== target) return;
          for (const view of views) view.dispatch({ effects: language.current.reconfigure(support) });
        });
      }
      return () => {
        editor.current = null;
        merge.destroy();
      };
    }
    target = new EditorView({
      parent,
      state: EditorState.create({
        doc: neu,
        extensions: [
          ...common(),
          unifiedMergeView({
            original: old,
            mergeControls: false,
            gutter: true,
            highlightChanges: true,
            syntaxHighlightDeletions: true,
            collapseUnchanged: { margin: 3, minSize: 6 },
          }),
          hunkBars,
          selection,
        ],
      }),
    });
    editor.current = target;
    const loaded = LanguageDescription.matchFilename(languages, fileName(path));
    if (loaded) {
      void loaded.load().then((support) => {
        if (editor.current === target) target.dispatch({ effects: language.current.reconfigure(support) });
      });
    }
    return () => {
      editor.current = null;
      target.destroy();
    };
  }, [path, old, neu, layout]);

  // The hunk bars: drawn again when the hunks, the actions' presence or `busy` change.
  const hasActions = Boolean(actions);
  React.useEffect(() => {
    const view = editor.current;
    if (!view) return;
    view.dispatch({ effects: setHunks.of(hunkDecorations(view.state, hunks, live)) });
  }, [hunks, hasActions, busy, old, neu, layout]);

  return <div ref={host} className="absolute inset-0 overflow-hidden [&_.cm-editor]:h-full [&_.cm-mergeView]:h-full [&_.cm-mergeView]:overflow-y-auto" />;
}
