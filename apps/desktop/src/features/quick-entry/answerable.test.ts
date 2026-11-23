// docs/features/quick-entry/spec.md (FR-14; tasks T19).
import { describe, expect, test } from "bun:test";
import type { PermissionRequest } from "@/features/chat/transcript";
import { answerable } from "./answerable";

const request = (extra: Partial<PermissionRequest>): PermissionRequest => ({
  requestId: "r1",
  title: "Run a command?",
  options: [{ title: "Allow" }, { title: "Deny" }],
  kind: "confirm",
  ...extra,
});

const question = (text: string) => ({ question: text, options: [{ label: "Yes" }, { label: "No" }], multiSelect: false });

describe("what the quick entry box answers itself", () => {
  test("an approval of a command or a tool call is answered in the box", () => {
    expect(answerable(request({ approval: { kind: "command", tool: "bash" } }))).toBe("inBox");
    expect(answerable(request({}))).toBe("inBox");
  });

  test("an edit is answered in the box with the full review offered", () => {
    expect(answerable(request({ title: "Edit src/a.ts?", approval: { kind: "edit", tool: "edit" } }))).toBe("inBoxWithReview");
  });

  test("one question is answered in the box, several go to Evo", () => {
    expect(answerable(request({ kind: "questions", options: [], questions: [question("Which one?")] }))).toBe("inBox");
    expect(
      answerable(request({ kind: "questions", options: [], questions: [question("A?"), question("B?"), question("C?")] })),
    ).toBe("elsewhere");
    expect(answerable(request({ kind: "questions", options: [], questions: [] }))).toBe("elsewhere");
  });

  test("the app's own permission requests (an app, the browser, a plugin) are answered in the box", () => {
    expect(
      answerable(request({ requestId: "shell:42", title: "Let the agent act in an app on this computer?", options: [{ title: "Allow" }, { title: "Deny" }] })),
    ).toBe("inBox");
    // One without choices cannot be answered by a click.
    expect(answerable(request({ requestId: "shell:43", options: [] }))).toBe("elsewhere");
  });

  test("other kinds of prompt go to Evo", () => {
    expect(answerable(request({ kind: "input", options: [] }))).toBe("elsewhere");
    expect(answerable(request({ kind: "select" }))).toBe("elsewhere");
  });
});
