// docs/features/source-control/spec.md (FR-10, FR-12; T21).
import { describe, expect, test } from "bun:test";
import { initialTranscript, noteKind, transcriptReducer, withCheckpoints, type TurnCheckpoint } from "./transcript";

const checkpoint = (id: string, promptEntryId?: string): TurnCheckpoint => ({
  id,
  promptEntryId,
  runId: `run-${id}`,
  startedAt: 1,
  start: "aaa",
  end: "bbb",
  files: [{ path: "src/a.ts", status: "modified", added: 2, removed: 1 }],
  complete: true,
});

const running = () =>
  transcriptReducer(transcriptReducer(initialTranscript, { type: "prompt", text: "edit a" }), {
    type: "engine",
    message: { type: "agent_start" },
  });

describe("turn checkpoints", () => {
  test("agent_settled puts its checkpoint on the turn", () => {
    const settled = transcriptReducer(running(), {
      type: "engine",
      message: { type: "agent_settled", outcome: "completed", checkpoint: checkpoint("c-1") },
    });
    expect(settled.turns.at(-1)?.checkpoint?.id).toBe("c-1");
    expect(settled.turns.at(-1)?.status).toBe("done");
  });

  test("a settlement after agent_end still brings the checkpoint", () => {
    const ended = transcriptReducer(running(), { type: "engine", message: { type: "agent_end", messages: [] } });
    const settled = transcriptReducer(ended, {
      type: "engine",
      message: { type: "agent_settled", outcome: "completed", checkpoint: checkpoint("c-2") },
    });
    expect(settled.turns.at(-1)?.checkpoint?.id).toBe("c-2");
  });

  test("a compaction's settlement and a malformed field leave the turn alone", () => {
    const compact = transcriptReducer(running(), {
      type: "engine",
      message: { type: "agent_settled", compact: true, checkpoint: checkpoint("c-3") },
    });
    expect(compact.turns.at(-1)?.checkpoint).toBeUndefined();
    const odd = transcriptReducer(running(), { type: "engine", message: { type: "agent_settled", checkpoint: { id: 3 } } });
    expect(odd.turns.at(-1)?.checkpoint).toBeUndefined();
  });

  test("a reopened chat's checkpoints land on their turns by prompt entry", () => {
    const state = {
      ...initialTranscript,
      turns: [
        { ...running().turns[0], id: "t1", entryId: "e1", status: "done" as const },
        { ...running().turns[0], id: "t2", entryId: "e2", status: "done" as const },
        { ...running().turns[0], id: "t3", status: "done" as const },
      ],
    };
    const next = withCheckpoints(state, [checkpoint("c-1", "e2"), checkpoint("c-9", "gone")]);
    expect(next.turns.map((turn) => turn.checkpoint?.id)).toEqual([undefined, "c-1", undefined]);
    expect(withCheckpoints(state, [])).toBe(state);
  });

  test("the revert note is an event, text that only looks like it is not", () => {
    expect(noteKind('<turn-reverted checkpoint="c-17">The person reverted your changes: src/a.ts.</turn-reverted>')).toBe(
      "turn-reverted",
    );
    expect(noteKind('<turn-reverted checkpoint="c-17">\nmulti\nline\n</turn-reverted>')).toBe("turn-reverted");
    expect(noteKind('<turn-reverted checkpoint="c-17">left open')).toBeUndefined();
    expect(noteKind('please explain <turn-reverted checkpoint="x"></turn-reverted>')).toBeUndefined();
  });
});
