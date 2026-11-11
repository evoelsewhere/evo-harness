// Subagents, background reports, notes and approval prompts in the transcript:
// the live route (reducer actions) and the rebuild route (saved messages).
import { describe, expect, test } from "bun:test";
import {
  LOST_AGENT,
  hasOpenPermission,
  initialTranscript,
  isBackgroundNote,
  noteKind,
  transcriptFromMessages,
  transcriptReducer,
  type TranscriptAction,
  type TranscriptState,
  type ToolPart,
} from "./transcript";
import { applyMeshEvent, childRunOf, emptyView } from "@/engine/mesh";

const run = (actions: TranscriptAction[], from: TranscriptState = initialTranscript) =>
  actions.reduce(transcriptReducer, from);
const engine = (message: Record<string, any>): TranscriptAction => ({ type: "engine", message: message as any });
const ask = (requestId: string) => engine({ type: "ui_dialog_request", requestId, ui: { title: requestId, options: [{ title: "Allow" }, { title: "Deny" }] } });
const report = (id: string, status: string, body: string, description = "Do it") =>
  `<background-agent id="${id}" description="${description}" status="${status}">\n${body}\n</background-agent>`;
const user = (text: string) => engine({ type: "message_start", message: { role: "user", content: text } });
const child = (id: string, extra: Record<string, any> = {}) => ({
  agentId: id,
  description: "Do it",
  background: true,
  status: "running",
  startedAt: 1,
  endedAt: null,
  tools: [{ callId: "x", name: "bash", phase: "running" }],
  text: "tail",
  ...extra,
});
const tools = (state: TranscriptState) =>
  state.turns.flatMap((turn) => turn.parts.filter((part): part is ToolPart => part.kind === "tool"));

describe("permissions", () => {
  test("two requests queue, the oldest shows, each answer removes its own", () => {
    let state = run([{ type: "prompt", text: "go" }, ask("a"), ask("b")]);
    expect(state.permissions?.map((p) => p.requestId)).toEqual(["a", "b"]);
    expect(state.turns.at(-1)?.permission?.requestId).toBe("a");
    expect(hasOpenPermission(state)).toBe(true);
    // A wake pushes a turn: the request moves with it.
    state = run([{ type: "prompt", text: "wake" }], state);
    expect(state.turns[0].permission).toBeUndefined();
    expect(state.turns[1].permission?.requestId).toBe("a");
    state = run([{ type: "permission-answered" }], state);
    expect(state.turns.at(-1)?.permission?.requestId).toBe("b");
    state = run([engine({ type: "ui_dialog_closed", requestId: "a" })], state);
    expect(state.permissions?.map((p) => p.requestId)).toEqual(["b"]);
    state = run([{ type: "permission-answered", requestId: "b" }], state);
    expect(hasOpenPermission(state)).toBe(false);
    expect(state.turns.at(-1)?.permission).toBeUndefined();
  });
  test("a run's end keeps a background agent's request", () => {
    const state = run([{ type: "prompt", text: "go" }, ask("a"), engine({ type: "agent_end", messages: [] })]);
    expect(hasOpenPermission(state)).toBe(true);
  });
});

describe("notes", () => {
  test("only the whole structure counts", () => {
    expect(noteKind(report("a", "done", "ok"))).toBe("agent-report");
    expect(noteKind(`${report("a", "error", "x")}\n\n${report("b", "error", "y")}`)).toBe("agent-report");
    expect(noteKind('<mail id="m" from="s_1/a_2" title="T" kind="ask">\nhi\n(A question)\n</mail>')).toBe("mail");
    expect(noteKind('<nested-session id="c" title="T" status="done">\nok\n</nested-session>')).toBe("session-note");
    expect(noteKind("<mail-arrived>\nx\n</mail-arrived>")).toBe("mail-arrived");
    expect(noteKind("<background-agents-done>\nx\n</background-agents-done>")).toBe("agents-done");
    expect(isBackgroundNote("<mail> see this xml </mail>")).toBe(false);
    expect(isBackgroundNote('<mail id="m">\nwhat is this?')).toBe(false);
    expect(isBackgroundNote(`${report("a", "done", "ok")}\nand what does this mean?`)).toBe(false);
    expect(isBackgroundNote("<nested-session-like text")).toBe(false);
  });
  test("a steered message of the person's that looks like a note still makes its turn", () => {
    const text = report("a", "done", "ok");
    const state = run([
      { type: "prompt", text: "go" },
      engine({ type: "message_update", messageId: "m1", assistantMessageEvent: { type: "text_delta", contentIndex: 0, delta: "hi" } }),
      { type: "queued", item: { entryId: "e", text, deliverAs: "steer" } },
      user(text),
    ]);
    expect(state.queued).toEqual([]);
    expect(state.turns.map((t) => t.prompt)).toEqual(["go", text]);
  });
});

/** The same conversation live and rebuilt: [P, assistant(agent call), result, N (steered), assistant]. */
describe("background agent report", () => {
  const note = report("a1", "done", "The full report.\n\nWith two paragraphs.");
  const live = () =>
    run([
      { type: "prompt", text: "go" },
      user("go"),
      engine({ type: "message_start", messageId: "m1", message: { role: "assistant" } }),
      engine({ type: "tool_execution_start", toolCallId: "c1", toolName: "agent", args: { description: "Do it", background: true } }),
      engine({ type: "tool_execution_end", toolCallId: "c1", result: { content: [{ type: "text", text: "Started background agent a1" }], details: { child: child("a1") } } }),
      user(note),
      engine({ type: "message_update", messageId: "m2", assistantMessageEvent: { type: "text_delta", contentIndex: 0, delta: "done" } }),
    ]);
  const saved = [
    { role: "user", content: "go", timestamp: 10 },
    { role: "assistant", content: [{ type: "toolCall", id: "c1", name: "agent", arguments: { description: "Do it", background: true } }], timestamp: 11 },
    { role: "toolResult", toolCallId: "c1", content: [{ type: "text", text: "Started background agent a1" }], details: { child: child("a1") }, timestamp: 12 },
    { role: "user", content: note, timestamp: 20 },
    { role: "assistant", content: [{ type: "text", text: "done" }], timestamp: 21 },
  ] as any[];

  test("live: settles the call with the report and starts an event turn", () => {
    const state = live();
    expect(state.turns.map((t) => t.prompt)).toEqual(["go", note]);
    const [call] = tools(state);
    expect(call.details?.child.status).toBe("done");
    expect(call.details?.child.report).toBe("The full report.\n\nWith two paragraphs.");
    expect(call.details?.child.error).toBeNull();
  });
  test("rebuilt: the same turns and the same call", () => {
    const state = transcriptFromMessages(saved, { engineStarted: true, lostAgents: [] });
    expect(state.turns.map((t) => t.prompt)).toEqual(["go", note]);
    const [call] = tools(state);
    expect(call.details?.child.status).toBe("done");
    expect(call.details?.child.report).toBe("The full report.\n\nWith two paragraphs.");
    expect(call.details?.child.endedAt).toBe(20);
  });
  test("a failed report is the error, never the call's output", () => {
    const messages = saved.slice(0, 3).concat([{ role: "user", content: report("a1", "error", "The subagent stopped: timed out."), timestamp: 20 }, { role: "user", content: "next", timestamp: 21 }]);
    const state = transcriptFromMessages(messages, {});
    expect(state.turns.map((t) => t.prompt)).toEqual(["go", "next"]);
    const [call] = tools(state);
    expect(call.details?.child.status).toBe("error");
    expect(call.details?.child.error).toBe("The subagent stopped: timed out.");
  });
  test("queued report + wake: no turn for the report, live and rebuilt", () => {
    const wake = "<background-agents-done>\nA background agent finished: \"Do it\". Its report is in the message above. Continue the work with it.\n</background-agents-done>";
    const before = run([
      { type: "prompt", text: "go" },
      engine({ type: "tool_execution_start", toolCallId: "c1", toolName: "agent", args: {} }),
      engine({ type: "tool_execution_end", toolCallId: "c1", result: { content: [], details: { child: child("a1") } } }),
      engine({ type: "agent_end", messages: [] }),
    ]);
    expect(tools(before)[0].details?.child.status).toBe("running");
    const state = run([{ type: "prompt", text: wake }, user(note), user(wake)], before);
    expect(state.turns.map((t) => t.prompt)).toEqual(["go", wake]);
    expect(tools(state)[0].details?.child.status).toBe("done");
    const rebuilt = transcriptFromMessages(saved.slice(0, 3).concat([{ role: "user", content: note, timestamp: 20 }, { role: "user", content: wake, timestamp: 21 }]));
    expect(rebuilt.turns.map((t) => t.prompt)).toEqual(["go", wake]);
  });
  test("a note right after the prompt makes no turn, live and rebuilt", () => {
    const state = run([{ type: "prompt", text: "p" }, user("p"), user(note)]);
    expect(state.turns.map((t) => t.prompt)).toEqual(["p"]);
    const rebuilt = transcriptFromMessages([
      { role: "user", content: "p", timestamp: 1 },
      { role: "user", content: note, timestamp: 2 },
      { role: "assistant", content: [{ type: "text", text: "x" }], timestamp: 3 },
    ] as any[]);
    expect(rebuilt.turns.map((t) => t.prompt)).toEqual(["p"]);
  });
  test("two notes steered together: the last one's turn, live and rebuilt", () => {
    const second = report("a2", "done", "Other.", "Other");
    const state = run([user(second)], live());
    // `live()` ended with assistant text after the first note: a new turn for the second.
    expect(state.turns.map((t) => t.prompt)).toEqual(["go", note, second]);
    const together = run([
      { type: "prompt", text: "go" },
      engine({ type: "message_update", messageId: "m1", assistantMessageEvent: { type: "text_delta", contentIndex: 0, delta: "hi" } }),
      user(note),
      user(second),
    ]);
    expect(together.turns.map((t) => t.prompt)).toEqual(["go", second]);
    const rebuilt = transcriptFromMessages([
      { role: "user", content: "go", timestamp: 1 },
      { role: "assistant", content: [{ type: "text", text: "hi" }], timestamp: 2 },
      { role: "user", content: note, timestamp: 3 },
      { role: "user", content: second, timestamp: 4 },
      { role: "assistant", content: [{ type: "text", text: "x" }], timestamp: 5 },
    ] as any[]);
    expect(rebuilt.turns.map((t) => t.prompt)).toEqual(["go", second]);
  });
});

describe("after a restart", () => {
  const saved = [
    { role: "user", content: "go", timestamp: 10 },
    { role: "assistant", content: [{ type: "toolCall", id: "c1", name: "agent", arguments: {} }], timestamp: 11 },
    { role: "toolResult", toolCallId: "c1", content: [{ type: "text", text: "Started" }], details: { child: child("a1") }, timestamp: 12 },
  ] as any[];
  test("listed lost agents stop; a queued report waits", () => {
    const lost = transcriptFromMessages(saved, { engineStarted: true, lostAgents: ["a1"] });
    expect(tools(lost)[0].details?.child.error).toBe(LOST_AGENT);
    const queued = transcriptFromMessages(saved, { engineStarted: true, lostAgents: [] });
    expect(tools(queued)[0].details?.child.status).toBe("done");
    expect(tools(queued)[0].details?.child.reportPending).toBe(true);
    const old = transcriptFromMessages(saved, { engineStarted: true });
    expect(tools(old)[0].details?.child.error).toBe(LOST_AGENT);
    const alive = transcriptFromMessages(saved, {});
    expect(tools(alive)[0].details?.child.status).toBe("running");
  });
  test("a report that arrives live settles a stopped or waiting call", () => {
    for (const lostAgents of [["a1"], []]) {
      const start = transcriptFromMessages(saved, { engineStarted: true, lostAgents });
      const state = run([{ type: "prompt", text: "next" }, user(report("a1", "done", "Late report.")), user("next")], start);
      const call = tools(state)[0].details?.child;
      expect(call.status).toBe("done");
      expect(call.report).toBe("Late report.");
      expect(call.error).toBeNull();
      expect(call.reportPending).toBe(false);
      expect(state.turns.map((t) => t.prompt)).toEqual(["go", "next"]);
    }
  });
  test("a continued agent: each report goes to its own call", () => {
    const messages = saved.concat([
      { role: "user", content: report("a1", "done", "First."), timestamp: 20 },
      { role: "assistant", content: [{ type: "toolCall", id: "c2", name: "message_agent", arguments: { agentId: "a1", message: "more" } }], timestamp: 21 },
      { role: "toolResult", toolCallId: "c2", content: [{ type: "text", text: "Started" }], details: { child: child("a1", { startedAt: 21 }) }, timestamp: 22 },
      { role: "user", content: report("a1", "done", "Second."), timestamp: 30 },
      { role: "assistant", content: [{ type: "text", text: "x" }], timestamp: 31 },
    ] as any[]);
    const state = transcriptFromMessages(messages, { engineStarted: true, lostAgents: [] });
    expect(tools(state).map((t) => t.details?.child.report)).toEqual(["First.", "Second."]);
  });
});

describe("engine exit", () => {
  test("closes open calls, agents and requests", () => {
    const before = run([
      { type: "prompt", text: "one" },
      engine({ type: "tool_execution_start", toolCallId: "c1", toolName: "agent", args: {} }),
      engine({ type: "tool_execution_end", toolCallId: "c1", result: { content: [], details: { child: child("a1") } } }),
      engine({ type: "agent_end", messages: [] }),
      { type: "prompt", text: "two" },
      engine({ type: "tool_execution_start", toolCallId: "c2", toolName: "agent", args: {} }),
      engine({ type: "tool_execution_update", toolCallId: "c2", partialResult: { content: [], details: { child: child("a2", { background: false }) } } }),
      ask("q"),
    ]);
    for (const action of [{ type: "fail", error: "The engine stopped" }, { type: "engine-exited" }] as TranscriptAction[]) {
      const state = run([action], before);
      expect(state.busy).toBe(false);
      expect(hasOpenPermission(state)).toBe(false);
      const [first, second] = tools(state);
      expect(first.phase).toBe("done");
      expect(first.details?.child.status).toBe("error");
      expect(first.details?.child.endedAt).toBeGreaterThan(0);
      expect(first.details?.child.tools[0].phase).toBe("error");
      expect(second.phase).toBe("error");
      expect(second.endedAt).toBeGreaterThan(0);
      expect(second.details?.child.status).toBe("error");
      expect(state.turns.map((t) => t.status)).toEqual(["done", "error"]);
    }
    // A failed prompt is not an exit: the background agent goes on.
    const failed = run([{ type: "fail", error: "busy" }], before);
    expect(tools(failed)[0].details?.child.status).toBe("running");
    expect(tools(failed)[1].details?.child.status).toBe("error");
    expect(hasOpenPermission(failed)).toBe(true);
  });
});

describe("a failed foreground agent", () => {
  test("keeps its child view and usage, live and rebuilt", () => {
    const final = child("a1", { background: false, status: "error", endedAt: 50, error: "timed out" });
    const live = run([
      { type: "prompt", text: "go" },
      engine({ type: "tool_execution_start", toolCallId: "c1", toolName: "agent", args: {} }),
      engine({ type: "tool_execution_update", toolCallId: "c1", partialResult: { content: [], details: { child: child("a1", { background: false }) } } }),
      engine({ type: "tool_execution_end", toolCallId: "c1", isError: true, result: { content: [{ type: "text", text: "subagent a1 stopped: timed out" }], details: { child: final }, usage: { input: 5, output: 7, cost: { total: 0.5 } } } }),
    ]);
    expect(tools(live)[0].details?.child.status).toBe("error");
    expect(live.turns[0].meta.cost).toBe(0.5);
    // An older engine's error result has no details: the last snapshot stays.
    const old = run([
      { type: "prompt", text: "go" },
      engine({ type: "tool_execution_start", toolCallId: "c1", toolName: "agent", args: {} }),
      engine({ type: "tool_execution_update", toolCallId: "c1", partialResult: { content: [], details: { child: final } } }),
      engine({ type: "tool_execution_end", toolCallId: "c1", isError: true, result: { content: [{ type: "text", text: "x" }], details: {} } }),
    ]);
    expect(tools(old)[0].details?.child.error).toBe("timed out");
    const rebuilt = transcriptFromMessages([
      { role: "user", content: "go", timestamp: 1 },
      { role: "assistant", content: [{ type: "toolCall", id: "c1", name: "agent", arguments: {} }], timestamp: 2 },
      { role: "toolResult", toolCallId: "c1", isError: true, content: [{ type: "text", text: "x" }], details: { child: final }, usage: { input: 5, output: 7, cost: { total: 0.5 } }, timestamp: 3 },
    ] as any[]);
    expect(tools(rebuilt)[0].details?.child.error).toBe("timed out");
    expect(rebuilt.turns[0].meta.cost).toBe(0.5);
  });
});

describe("mesh", () => {
  const env = (op: string, body: any, seq: number) => ({ type: "mesh", v: 1, id: `e${seq}`, kind: "evt", op, from: "s_1", to: "w", seq, ts: 0, body }) as any;
  const node = (state: string, status?: string) => ({ id: "s_1/a_x", kind: "agent", root: "s_1", depth: 1, title: "Do it", origin: "agent", state, status, caps: [], usage: { totalTokens: 1, cost: 0 }, startedAt: 1, meta: { callId: "c1", background: true } });
  test("node_report is the report; a new run drops it", () => {
    const view = emptyView();
    applyMeshEvent(view, env("node_state", { node: node("running") }, 1));
    applyMeshEvent(view, env("node_activity", { node: "s_1/a_x", text: "…tail" }, 2));
    expect(applyMeshEvent(view, env("node_report", { node: "s_1/a_x", status: "done", text: "Whole report" }, 3))).toBe("s_1/a_x");
    applyMeshEvent(view, env("node_state", { node: node("settled", "done") }, 4));
    const run = childRunOf(view, "s_1/a_x");
    expect(run?.report).toBe("Whole report");
    expect(run?.text).toBe("…tail");
    expect(run?.status).toBe("done");
    applyMeshEvent(view, env("node_state", { node: node("running") }, 5));
    expect(childRunOf(view, "s_1/a_x")?.report).toBeUndefined();
  });
  test("agent-progress reaches a message_agent call and keeps the report it has", () => {
    const state = run([
      { type: "prompt", text: "go" },
      engine({ type: "tool_execution_start", toolCallId: "c9", toolName: "message_agent", args: { agentId: "a1", message: "more" } }),
      engine({ type: "tool_execution_end", toolCallId: "c9", result: { content: [], details: { child: child("a1") } } }),
      user(report("a1", "done", "Kept.")),
      { type: "agent-progress", callId: "c9", child: child("a1", { status: "done", endedAt: 9 }) },
    ]);
    expect(tools(state)[0].details?.child.report).toBe("Kept.");
  });
});

describe("notes about a proposed session", () => {
  test("the hub's started, dismissed and cut notes are automated messages", () => {
    const started =
      '<nested-session id="c1" title="Check docs" status="started">\nThe user started the session you proposed (proposal_1_1). Its id is c1: use message_session to follow up.\n</nested-session>';
    const dismissed =
      '<nested-session proposal="proposal_1_2" title="Check docs" status="dismissed">\nThe user dismissed this proposed session: it was not started.\n</nested-session>';
    const untitled =
      '<nested-session proposal="proposal_1_3" status="dismissed">\nThe user dismissed this proposed session: it was not started.\n</nested-session>';
    const cut =
      '<nested-session id="c1" title="Check docs" status="done">\nThe start of a long reply…\n[Cut: this is the start of a reply of 900 characters. Read all of it with session_read {"session": "c1"}, or ask the session with message_session.]\n</nested-session>';
    for (const note of [started, dismissed, untitled, cut]) expect(noteKind(note)).toBe("session-note");
    // Pasted text that only starts like one stays the person's.
    expect(noteKind('<nested-session proposal="x" status="dismissed"> and more')).toBeUndefined();
  });
});
