import { describe, expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { Markdown } from "./markdown";

// rich-replies FR-37: a reply's file names render as buttons that open the file.
describe("Markdown file links", () => {
  test("a file named in a reply is a button when the view can open files", () => {
    const html = renderToStaticMarkup(
      createElement(Markdown, { onLocalLink: () => {} }, "The Team plan costs 29 USD a month (source: pricing-notes.md)."),
    );
    expect(html).toMatch(/<button[^>]*>pricing-notes\.md<\/button>/);
  });

  test("without an open handler the name stays text", () => {
    const html = renderToStaticMarkup(createElement(Markdown, null, "See pricing-notes.md."));
    expect(html).not.toContain("<button");
    expect(html).toContain("pricing-notes.md");
  });
});
