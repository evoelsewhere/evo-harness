import { describe, expect, test } from "bun:test";
import { initialTranscript, transcriptReducer } from "./transcript";

const open = (ui: Record<string, unknown>) => {
  const started = transcriptReducer(initialTranscript, { type: "prompt", text: "go" });
  const state = transcriptReducer(started, {
    type: "engine",
    message: { type: "ui_dialog_request", requestId: "approve:1", toolCallId: "1", ui },
  });
  return state.turns.at(-1)?.permission;
};

describe("approval prompts", () => {
  test("carry the engine's approval details", () => {
    const permission = open({
      kind: "confirm",
      title: "Run a command?",
      description: "npm install",
      approval: { kind: "command", tool: "bash", agent: "explore-2" },
      options: [{ title: "Allow" }, { title: "Allow commands for this chat", permission: "allow-chat" }, { title: "Deny", permission: "reject" }],
    });
    expect(permission?.approval).toEqual({ kind: "command", tool: "bash", agent: "explore-2" });
    expect(permission?.options.map((o) => o.permission)).toEqual([undefined, "allow-chat", "reject"]);
  });

  test("older prompts have no approval details", () => {
    const permission = open({ kind: "confirm", title: "Allow this hook?", options: [{ title: "Allow" }, { title: "Deny" }] });
    expect(permission?.approval).toBeUndefined();
    expect(open({ kind: "confirm", title: "x", approval: { kind: "other" } })?.approval).toBeUndefined();
  });
});
