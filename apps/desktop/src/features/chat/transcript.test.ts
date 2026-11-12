// Parity of a turn's cost and tokens: the transcript's own count (what a
// window shows) and the engine's `agent_settled.turn` (what the shell
// records) for the same run (docs/features/shell-hosted-engines/spec.md,
// FR-6; T24). The fixture is the stdout of the engine test
// `turn_totals_count_a_run_with_tools_and_a_foreground_subagent`
// (crates/evo-engine/src/modes/turn_totals_tests.rs): a run on a priced
// faux model with a foreground subagent and a tool call. Write it again with
// `EVO_TURN_FIXTURE=<this folder>/fixtures/turn-totals.jsonl cargo test -p evo-engine turn_totals_count_a_run`.
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import path from "node:path";
import type { TurnTotals } from "@/engine/protocol";
import { initialTranscript, transcriptReducer, type TranscriptAction } from "./transcript";

const lines = readFileSync(path.join(import.meta.dir, "fixtures", "turn-totals.jsonl"), "utf8")
  .split("\n")
  .filter((line) => line.trim())
  .map((line) => JSON.parse(line) as Record<string, any>);

describe("turn totals", () => {
  test("the transcript and the engine count the same run alike", () => {
    const actions: TranscriptAction[] = [
      { type: "prompt", text: "What is the answer?" },
      ...lines.map((message): TranscriptAction => ({ type: "engine", message: message as any })),
    ];
    const state = actions.reduce(transcriptReducer, initialTranscript);
    const meta = state.turns.at(-1)!.meta;
    const turn = lines.find((line) => line.type === "agent_settled")!.turn as TurnTotals;
    expect(turn).toBeDefined();
    // Not a trivial run: priced responses and a subagent.
    expect(turn.costUsd).toBeGreaterThan(0);
    expect(turn.childCostUsd).toBeGreaterThan(0);
    expect(turn.childTokens).toBeGreaterThan(0);

    const shownTokens = (meta.inputTokens ?? 0) + (meta.outputTokens ?? 0) + (meta.cacheRead ?? 0) + (meta.cacheWrite ?? 0);
    const recordedTokens = turn.input + turn.output + turn.cacheRead + turn.cacheWrite + turn.childTokens;
    expect(shownTokens).toBe(recordedTokens);
    expect(meta.cost ?? 0).toBeCloseTo(turn.costUsd + turn.childCostUsd, 12);
    expect(meta.provider).toBe(turn.provider);
    expect(meta.model).toBe(turn.model);
  });
});
