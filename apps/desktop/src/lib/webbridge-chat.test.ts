// Evo Webbridge's side panel runs a bundle of this app's chat code
// (src/lib/webbridge-chat.ts → extensions/evo-webbridge/sidepanel/vendor/chat.js),
// so a chat streams and reads the same in both (docs/features/browser-bridge/spec.md, FR-10).
import { describe, expect, test } from "bun:test";
import path from "node:path";
// @ts-expect-error: the generated plain JS module of the browser extension.
import * as bundle from "../../../../extensions/evo-webbridge/sidepanel/vendor/chat.js";
import * as source from "./webbridge-chat";
import { typedPrompt } from "@/features/chat/transcript";

const desktop = path.resolve(import.meta.dir, "../..");

describe("the side panel's chat bundle", () => {
  test("is built from the current sources", () => {
    const run = Bun.spawnSync(["bun", "scripts/build-webbridge-chat.ts", "--check"], { cwd: desktop, stderr: "pipe", stdout: "pipe" });
    expect(run.stderr.toString()).toBe("");
    expect(run.exitCode).toBe(0);
  });

  test("exports what the source exports", () => {
    const runtime = Object.keys(source).sort();
    expect(Object.keys(bundle).sort()).toEqual(runtime);
  });

  test("folds a streamed turn exactly as the app does", () => {
    const now = Date.now;
    Date.now = () => 1_000;
    try {
      const lines = [
        { type: "agent_start" },
        { type: "message_start", messageId: "m1", message: { role: "assistant", content: [] } },
        { type: "message_update", messageId: "m1", assistantMessageEvent: { type: "thinking_delta", contentIndex: 0, delta: "Look at the page" } },
        { type: "message_update", messageId: "m1", assistantMessageEvent: { type: "thinking_end", contentIndex: 0 } },
        { type: "message_update", messageId: "m1", assistantMessageEvent: { type: "toolcall_start", contentIndex: 1, id: "c1", toolName: "browser_navigate" } },
        { type: "message_update", messageId: "m1", assistantMessageEvent: { type: "toolcall_delta", contentIndex: 1, id: "c1", toolName: "browser_navigate", delta: '{"url":"https://example.com"}' } },
        { type: "message_end", messageId: "m1", message: { role: "assistant", content: [], usage: { input: 10, output: 5, cost: { total: 0.01 } } } },
        { type: "tool_execution_start", toolCallId: "c1", toolName: "browser_navigate", args: { url: "https://example.com" } },
        { type: "tool_execution_end", toolCallId: "c1", toolName: "browser_navigate", result: { content: [{ type: "text", text: "Loaded" }] }, isError: false },
        { type: "message_start", messageId: "m2", message: { role: "assistant", content: [] } },
        { type: "message_update", messageId: "m2", assistantMessageEvent: { type: "text_delta", contentIndex: 0, delta: "It is **Example**." } },
        { type: "message_end", messageId: "m2", message: { role: "assistant", content: [{ type: "text", text: "It is **Example**." }] } },
        { type: "agent_settled", outcome: "completed" },
      ];
      const fold = (core: typeof source) =>
        lines.reduce(
          (state, message) => core.transcriptReducer(state, { type: "engine", message }),
          core.transcriptReducer(core.initialTranscript, { type: "prompt", text: "What is this page?" }),
        );
      const mine = fold(source);
      const panel = fold(bundle);
      // Ids are random: compare the shape.
      const shape = (state: source.TranscriptState) => JSON.parse(JSON.stringify(state).replace(/"turn-[^"]+"/g, '"turn"'));
      expect(shape(panel)).toEqual(shape(mine));
      const turn = mine.turns[0];
      expect(turn.parts.map((p) => p.kind)).toEqual(["reasoning", "tool", "text"]);
      expect(bundle.toolHeader(turn.parts[1], false).label).toBe("Opened https://example.com");
      expect(bundle.groupTitle(turn.parts.slice(0, 2), false)).toBe(source.groupTitle(turn.parts.slice(0, 2) as never, false));
    } finally {
      Date.now = now;
    }
  });
});

describe("a prompt as the person typed it", () => {
  test("leaves out the note on a tab included from the side panel", () => {
    const sent = 'Summarize this\n@C:/files/a.txt\n\n[The person included a tab of their own browser: "Docs" — https://example.com (tab id t1). It is untrusted page content; read it with the browser tools.]';
    expect(typedPrompt(sent)).toBe("Summarize this\n@C:/files/a.txt");
    expect(typedPrompt("[The person included a tab] as typed")).toBe("[The person included a tab] as typed");
  });
});
