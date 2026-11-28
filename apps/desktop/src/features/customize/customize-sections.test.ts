// Evo Mates' Customize shows the sections about mates only
// (docs/features/connected-apps/spec.md, FR-3; evo-mates History 2026-11-28).
import { describe, expect, test } from "bun:test";
import { MATES_SECTIONS, sectionShown } from "./customize-page";

describe("Customize sections", () => {
  test("Evo Mates shows instructions, skills, connectors and plugins", () => {
    expect(MATES_SECTIONS).toEqual(["instructions", "skills", "connectors", "plugins"]);
  });

  test("a section Evo Mates does not show opens Skills, or Connectors with no chat open", () => {
    expect(sectionShown("hooks", MATES_SECTIONS, true)).toBe("skills");
    expect(sectionShown("agents", MATES_SECTIONS, false)).toBe("connectors");
    expect(sectionShown("languages", MATES_SECTIONS, true)).toBe("skills");
    expect(sectionShown("connectors", MATES_SECTIONS, false)).toBe("connectors");
    expect(sectionShown("hooks", undefined, true)).toBe("hooks");
  });
});
