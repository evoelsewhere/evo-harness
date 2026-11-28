import { describe, expect, test } from "bun:test";
import { FILE_LINK, linkFiles, parseFileRef } from "./file-refs";

const link = (label: string, path: string) => `[${label}](${FILE_LINK}${encodeURIComponent(path)})`;

describe("linkFiles (docs/features/rich-replies/spec.md, FR-37)", () => {
  test("a file name in the text becomes a link", () => {
    expect(linkFiles("The Team plan costs 29 USD a month (source: pricing-notes.md).")).toBe(
      `The Team plan costs 29 USD a month (source: ${link("pricing-notes.md", "pricing-notes.md")}).`,
    );
  });

  test("a path with a line, a nested path and a Windows path", () => {
    expect(linkFiles("See src/app.ts:42 and docs\\plan.md")).toBe(
      `See ${link("src/app.ts:42", "src/app.ts:42")} and ${link("docs\\plan.md", "docs\\plan.md")}`,
    );
  });

  test("a path in backticks becomes a link around the code", () => {
    expect(linkFiles("Open `crates/evo-web/src/fetch.rs`.")).toBe(
      `Open [\`crates/evo-web/src/fetch.rs\`](${FILE_LINK}${encodeURIComponent("crates/evo-web/src/fetch.rs")}).`,
    );
  });

  test("a relative markdown link opens the file", () => {
    expect(linkFiles("[the notes](docs/notes.md)")).toBe(link("the notes", "docs/notes.md"));
  });

  test("code that is not a path stays code", () => {
    expect(linkFiles("Run `cargo test` then `x.len()`.")).toBe("Run `cargo test` then `x.len()`.");
  });

  test("nothing changes inside code blocks, web links or web addresses", () => {
    const fenced = "```\nopen notes.md\n```";
    expect(linkFiles(fenced)).toBe(fenced);
    expect(linkFiles("[site](https://example.com/a.md)")).toBe("[site](https://example.com/a.md)");
    expect(linkFiles("Go to https://evo.dev/docs/start.md now")).toBe("Go to https://evo.dev/docs/start.md now");
    expect(linkFiles("Visit example.com or evo.dev")).toBe("Visit example.com or evo.dev");
  });

  test("numbers, versions and abbreviations are not files", () => {
    const text = "Version 1.5, v2.0.1, e.g. this, Node.js and 3.14 are not files.";
    expect(linkFiles(text)).toBe(text);
  });
});

describe("parseFileRef", () => {
  test("splits a trailing line number", () => {
    expect(parseFileRef("src/app.ts:42")).toEqual({ path: "src/app.ts", line: 42 });
    expect(parseFileRef("src/app.ts:42:7")).toEqual({ path: "src/app.ts", line: 42 });
    expect(parseFileRef("notes.md")).toEqual({ path: "notes.md" });
    expect(parseFileRef("C:\\work\\notes.md")).toEqual({ path: "C:\\work\\notes.md" });
  });
});
