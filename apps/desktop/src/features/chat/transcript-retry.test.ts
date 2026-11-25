// A run the engine retries after a failed attempt stays busy
// (`retry_scheduled`; docs/features/evo-mates/spec.md, FR-59).
import { describe, expect, test } from "bun:test";
import { initialTranscript, transcriptReducer, type TranscriptAction } from "./transcript";

const run = (actions: TranscriptAction[]) => actions.reduce(transcriptReducer, initialTranscript);

describe("retried runs", () => {
  test("a scheduled retry keeps the run busy and the turn running", () => {
    const state = run([
      { type: "prompt", text: "hi" },
      { type: "engine", message: { type: "agent_start" } as any },
      {
        type: "engine",
        message: {
          type: "agent_end",
          messages: [{ role: "assistant", content: [], stopReason: "error", errorMessage: "http error 408" }],
        } as any,
      },
      { type: "engine", message: { type: "retry_scheduled", attempt: 1, maxRetries: 10, delayMs: 2000, error: "http error 408" } as any },
    ]);
    expect(state.busy).toBe(true);
    const turn = state.turns.at(-1)!;
    expect(turn.status).toBe("running");
    expect(turn.error).toBeUndefined();
    expect(turn.retry).toEqual({ attempt: 1, maxRetries: 10, delayMs: 2000, error: "http error 408" });
  });
});
