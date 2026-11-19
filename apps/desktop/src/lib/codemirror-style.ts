// The app's CodeMirror look: token colours and editor chrome from the CSS
// variables in index.css, so light and dark follow the theme. Shared by the
// Files editor and the Source control diff.

import { HighlightStyle } from "@codemirror/language";
import { EditorView } from "@codemirror/view";
import { tags as t } from "@lezer/highlight";

/** Token colours come from CSS variables (index.css), so light and dark follow the theme. */
export const highlight = HighlightStyle.define([
  { tag: [t.keyword, t.operatorKeyword, t.modifier, t.controlKeyword], color: "var(--syn-keyword)" },
  { tag: [t.string, t.special(t.string), t.regexp, t.character], color: "var(--syn-string)" },
  { tag: [t.number, t.bool, t.null, t.atom], color: "var(--syn-number)" },
  { tag: [t.comment, t.lineComment, t.blockComment, t.docComment], color: "var(--syn-comment)", fontStyle: "italic" },
  { tag: [t.function(t.variableName), t.function(t.propertyName), t.macroName], color: "var(--syn-function)" },
  { tag: [t.typeName, t.className, t.namespace, t.definition(t.typeName)], color: "var(--syn-type)" },
  { tag: [t.propertyName, t.attributeName], color: "var(--syn-property)" },
  { tag: [t.tagName, t.angleBracket], color: "var(--syn-tag)" },
  { tag: [t.heading, t.strong], fontWeight: "600", color: "var(--syn-heading)" },
  { tag: t.emphasis, fontStyle: "italic" },
  { tag: [t.link, t.url], color: "var(--syn-string)", textDecoration: "underline" },
  { tag: [t.meta, t.processingInstruction, t.annotation], color: "var(--syn-comment)" },
  { tag: t.invalid, color: "var(--destructive)" },
]);

export const theme = EditorView.theme({
  "&": { height: "100%", fontSize: "12.5px", backgroundColor: "transparent", color: "var(--foreground)" },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": { fontFamily: "var(--app-font-mono)", lineHeight: "1.6" },
  ".cm-content": { caretColor: "var(--foreground)", paddingBottom: "40vh" },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--foreground)" },
  ".cm-gutters": { backgroundColor: "transparent", border: "none", color: "var(--muted-foreground)" },
  ".cm-lineNumbers .cm-gutterElement": { padding: "0 10px 0 12px", minWidth: "3ch" },
  ".cm-activeLine": { backgroundColor: "color-mix(in oklab, var(--foreground) 4%, transparent)" },
  ".cm-activeLineGutter": { backgroundColor: "transparent", color: "var(--foreground)" },
  "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, ::selection":
    { backgroundColor: "color-mix(in oklab, var(--brand) 26%, transparent) !important" },
  ".cm-searchMatch": { backgroundColor: "#ffb22440", outline: "1px solid #ffb22480" },
  ".cm-searchMatch.cm-searchMatch-selected": { backgroundColor: "#ffb224a0" },
  ".cm-selectionMatch": { backgroundColor: "color-mix(in oklab, var(--brand) 12%, transparent)" },
  ".cm-matchingBracket": { backgroundColor: "color-mix(in oklab, var(--brand) 20%, transparent)", outline: "none" },
  ".cm-foldGutter .cm-gutterElement": { padding: "0 4px", cursor: "pointer" },
  ".cm-panels": { backgroundColor: "var(--popover)", color: "var(--popover-foreground)", fontSize: "12px" },
  ".cm-panels.cm-panels-bottom": { borderTop: "1px solid var(--border)" },
  ".cm-panel.cm-search": { padding: "6px 8px" },
  ".cm-panel.cm-search input, .cm-panel.cm-search button": { fontSize: "12px" },
  ".cm-textfield": {
    backgroundColor: "var(--background)",
    border: "1px solid var(--border)",
    borderRadius: "6px",
    padding: "2px 6px",
  },
  ".cm-button": {
    backgroundImage: "none",
    backgroundColor: "var(--muted)",
    border: "1px solid var(--border)",
    borderRadius: "6px",
    padding: "2px 8px",
  },
  ".cm-revealed-line": { backgroundColor: "color-mix(in oklab, var(--brand) 16%, transparent)", transition: "background-color 1s" },
});
