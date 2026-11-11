// Run with `bun test` from apps/desktop. Spec: docs/features/app-updates (FR-16).
import { describe, expect, test } from "bun:test";
import { changeCounts, changeKind, parseChangelog } from "./changelog";

const NOTES = `## What's changed

### Breaking changes

- **engine:** Drop the old config file ([0123456](https://github.com/o/r/commit/0123456789abcdef))

### Features

- **desktop:** New-version dialog with the release notes ([abcdef1](https://github.com/o/r/commit/abcdef1234567))
- Faster \`grep\` on big trees ([1111111](https://github.com/o/r/commit/1111111aaaa))

### Fixes

- **desktop:** Placeholder rows while Settings lists the apps ([a62c0ea](https://github.com/o/r/commit/a62c0ea2))

**Full Changelog**: https://github.com/o/r/compare/v4.3.0...v4.3.1
`;

describe("release notes in sections", () => {
  test("the release pipeline's notes are parsed", () => {
    const changelog = parseChangelog(NOTES);
    expect(changelog).not.toBeNull();
    expect(changelog!.sections.map((s) => [s.title, s.kind, s.entries.length])).toEqual([
      ["Breaking changes", "breaking", 1],
      ["Features", "features", 2],
      ["Fixes", "fixes", 1],
    ]);
    expect(changelog!.sections[1].entries[0]).toEqual({
      scope: "desktop",
      text: "New-version dialog with the release notes",
      commit: {
        sha: "abcdef1",
        url: "https://github.com/o/r/commit/abcdef1234567",
      },
    });
    // No scope: the whole subject is the text.
    expect(changelog!.sections[1].entries[1].scope).toBeUndefined();
    expect(changelog!.sections[1].entries[1].text).toBe("Faster `grep` on big trees");
    expect(changelog!.compareUrl).toBe("https://github.com/o/r/compare/v4.3.0...v4.3.1");
    expect(changelog!.summary).toEqual([]);
  });

  test("a release without user-facing changes keeps its line", () => {
    const changelog = parseChangelog("## What's changed\n\nNo user-facing changes in this release.\n");
    expect(changelog).toEqual({
      sections: [],
      summary: ["No user-facing changes in this release."],
      compareUrl: undefined,
    });
  });

  test("notes in another shape are left to the Markdown view", () => {
    expect(parseChangelog("")).toBeNull();
    expect(parseChangelog("- just a list\n- of things")).toBeNull();
    expect(parseChangelog("### Features\n\nSome paragraph instead of a list.")).toBeNull();
  });

  test("the counts under the title", () => {
    expect(changeCounts(parseChangelog(NOTES)!)).toBe("1 breaking change · 2 features · 1 fix");
  });

  test("section kinds", () => {
    expect(changeKind("Performance")).toBe("performance");
    expect(changeKind("Bug fixes")).toBe("fixes");
    expect(changeKind("Other changes")).toBe("other");
  });
});
