// Markdown for the agent's text in the side panel, close to what the app's
// Streamdown renders (apps/desktop/src/components/markdown.tsx): paragraphs,
// headings, nested bullet and numbered lists, task items, block quotes, rules,
// tables, fenced code with its language, `code`, **bold**, *italic*,
// ~~struck~~, [links](https://…) and bare https:// links. It builds DOM nodes
// with text content only, so nothing in a reply becomes HTML (plan 3.7). A
// reply still streaming may end inside a fence: the open fence shows as code.

import { blockText } from "./ui-blocks.js";

const INLINE =
  /(`[^`\n]+`)|(\*\*[^*\n]+\*\*|__[^_\n]+__)|(~~[^~\n]+~~)|(\*[^*\n]+\*|(?<![\w])_[^_\n]+_(?![\w]))|(\[[^\]\n]+\]\((https?:\/\/[^)\s]+)\))|(https?:\/\/[^\s<>()]+[^\s<>().,;:!?'"])/;

function anchor(text, href) {
  const link = document.createElement("a");
  link.textContent = text;
  link.href = href;
  link.target = "_blank";
  link.rel = "noopener noreferrer";
  return link;
}

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
    else if (match[2]) into.append(inline(token.slice(2, -2), document.createElement("strong")));
    else if (match[3]) into.append(inline(token.slice(2, -2), document.createElement("del")));
    else if (match[4]) into.append(inline(token.slice(1, -1), document.createElement("em")));
    else if (match[5]) into.append(anchor(token.slice(1, token.indexOf("]")), match[6]));
    else if (match[7]) into.append(anchor(token, token));
    rest = rest.slice(match.index + token.length);
  }
  return into;
}

const LIST_ITEM = /^(\s*)([-*+•]|\d+[.)])\s+(.*)$/;
const TABLE_RULE = /^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$/;

function cells(line) {
  let row = line.trim();
  if (row.startsWith("|")) row = row.slice(1);
  if (row.endsWith("|")) row = row.slice(0, -1);
  return row.split("|").map((cell) => cell.trim());
}

export function renderMarkdown(text) {
  const fragment = document.createDocumentFragment();
  const lines = String(text).replace(/\r\n/g, "\n").split("\n");
  let paragraph = [];
  /** Open lists, outermost first: { node, indent }. */
  let lists = [];
  const flush = () => {
    if (paragraph.length) {
      fragment.append(inline(paragraph.join("\n"), document.createElement("p")));
      paragraph = [];
    }
  };
  const endLists = () => {
    lists = [];
  };

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const fence = line.match(/^\s*(```+|~~~+)\s*([\w+-]*)/);
    if (fence) {
      flush();
      endLists();
      const info = fence[2];
      const code = [];
      for (i++; i < lines.length && !lines[i].trimStart().startsWith(fence[1]); i++) code.push(lines[i]);
      const pre = document.createElement("pre");
      // An Evo UI block shows as its text, never its JSON (rich-replies FR-28).
      const body = info === "evo-ui" ? blockText(code.join("\n")) : code.join("\n");
      if (info && info !== "evo-ui") pre.append(Object.assign(document.createElement("span"), { className: "code-lang", textContent: info }));
      pre.append(Object.assign(document.createElement("code"), { textContent: body }));
      fragment.append(pre);
      continue;
    }
    // A table: a header row, its rule, then rows.
    if (line.includes("|") && i + 1 < lines.length && TABLE_RULE.test(lines[i + 1])) {
      flush();
      endLists();
      const wrap = Object.assign(document.createElement("div"), { className: "table-wrap" });
      const table = document.createElement("table");
      const head = document.createElement("tr");
      for (const cell of cells(line)) head.append(inline(cell, document.createElement("th")));
      const thead = document.createElement("thead");
      thead.append(head);
      table.append(thead);
      const body = document.createElement("tbody");
      for (i += 2; i < lines.length && lines[i].includes("|") && lines[i].trim(); i++) {
        const row = document.createElement("tr");
        for (const cell of cells(lines[i])) row.append(inline(cell, document.createElement("td")));
        body.append(row);
      }
      i--;
      table.append(body);
      wrap.append(table);
      fragment.append(wrap);
      continue;
    }
    const item = line.match(LIST_ITEM);
    if (item) {
      flush();
      const indent = item[1].replace(/\t/g, "  ").length;
      const tag = /\d/.test(item[2]) ? "OL" : "UL";
      while (lists.length && indent < lists[lists.length - 1].indent) lists.pop();
      let top = lists[lists.length - 1];
      if (!top || indent > top.indent || top.node.tagName !== tag) {
        const node = document.createElement(tag.toLowerCase());
        if (top && indent > top.indent) (top.node.lastElementChild || top.node).append(node);
        else {
          if (top && top.indent === indent) lists.pop();
          fragment.append(node);
        }
        top = { node, indent };
        lists.push(top);
      }
      const li = document.createElement("li");
      const task = item[3].match(/^\[([ xX])\]\s+(.*)$/);
      if (task) li.append(task[1] === " " ? "☐ " : "☑ ");
      top.node.append(inline(task ? task[2] : item[3], li));
      continue;
    }
    if (lists.length && /^\s{2,}\S/.test(line) && paragraph.length === 0) {
      // A list item's next line.
      const last = lists[lists.length - 1].node.lastElementChild;
      if (last) {
        last.append(document.createElement("br"));
        inline(line.trim(), last);
        continue;
      }
    }
    const heading = line.match(/^(#{1,6})\s+(.*?)\s*#*\s*$/);
    if (heading) {
      flush();
      endLists();
      fragment.append(inline(heading[2], document.createElement(`h${Math.min(heading[1].length, 4)}`)));
      continue;
    }
    if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(line)) {
      flush();
      endLists();
      fragment.append(document.createElement("hr"));
      continue;
    }
    const quote = line.match(/^\s*>\s?(.*)$/);
    if (quote) {
      flush();
      endLists();
      const block = [quote[1]];
      while (i + 1 < lines.length && /^\s*>/.test(lines[i + 1])) block.push(lines[++i].replace(/^\s*>\s?/, ""));
      const node = document.createElement("blockquote");
      node.append(renderMarkdown(block.join("\n")));
      fragment.append(node);
      continue;
    }
    if (!line.trim()) {
      flush();
      endLists();
      continue;
    }
    endLists();
    paragraph.push(line);
  }
  flush();
  return fragment;
}
