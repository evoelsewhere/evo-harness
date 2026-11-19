// A small Markdown renderer for the agent's text in the side panel. It builds
// DOM nodes with text content only, so nothing in a reply becomes HTML
// (plan 3.7): paragraphs, headings, bullet and numbered lists, fenced code,
// `code`, **bold**, *italic* and [links](https://…).

import { blockText } from "./ui-blocks.js";

const INLINE = /(`[^`\n]+`)|(\*\*[^*\n]+\*\*)|(\*[^*\n]+\*)|(\[[^\]\n]+\]\((https?:\/\/[^)\s]+)\))/;

function inline(text, into) {
  let rest = text;
  while (rest) {
    const match = rest.match(INLINE);
    if (!match) {
      into.append(document.createTextNode(rest));
      break;
    }
    if (match.index > 0) into.append(document.createTextNode(rest.slice(0, match.index)));
    const token = match[0];
    if (match[1]) into.append(Object.assign(document.createElement("code"), { textContent: token.slice(1, -1) }));
    else if (match[2]) into.append(Object.assign(document.createElement("strong"), { textContent: token.slice(2, -2) }));
    else if (match[3]) into.append(Object.assign(document.createElement("em"), { textContent: token.slice(1, -1) }));
    else if (match[4]) {
      const link = document.createElement("a");
      link.textContent = token.slice(1, token.indexOf("]"));
      link.href = match[5];
      link.target = "_blank";
      link.rel = "noopener noreferrer";
      into.append(link);
    }
    rest = rest.slice(match.index + token.length);
  }
  return into;
}

export function renderMarkdown(text) {
  const fragment = document.createDocumentFragment();
  const lines = String(text).replace(/\r\n/g, "\n").split("\n");
  let paragraph = [];
  let list = null;
  const flush = () => {
    if (paragraph.length) {
      fragment.append(inline(paragraph.join("\n"), document.createElement("p")));
      paragraph = [];
    }
  };
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (line.startsWith("```")) {
      flush();
      list = null;
      const info = line.slice(3).trim();
      const code = [];
      for (i++; i < lines.length && !lines[i].startsWith("```"); i++) code.push(lines[i]);
      const pre = document.createElement("pre");
      // An Evo UI block shows as its text, never its JSON (rich-replies FR-28).
      const text = info === "evo-ui" ? blockText(code.join("\n")) : code.join("\n");
      pre.append(Object.assign(document.createElement("code"), { textContent: text }));
      fragment.append(pre);
      continue;
    }
    const bullet = line.match(/^\s*[-*•]\s+(.*)$/);
    const number = line.match(/^\s*\d+[.)]\s+(.*)$/);
    if (bullet || number) {
      flush();
      const tag = bullet ? "UL" : "OL";
      if (!list || list.tagName !== tag) {
        list = document.createElement(tag.toLowerCase());
        fragment.append(list);
      }
      list.append(inline((bullet || number)[1], document.createElement("li")));
      continue;
    }
    const heading = line.match(/^#{1,4}\s+(.*)$/);
    if (heading) {
      flush();
      list = null;
      fragment.append(inline(heading[1], document.createElement("h3")));
      continue;
    }
    if (!line.trim()) {
      flush();
      list = null;
      continue;
    }
    list = null;
    paragraph.push(line);
  }
  flush();
  return fragment;
}
