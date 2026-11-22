// Code mode benchmark (docs/features/code-mode/spec.md, SC-1; tasks.md T42).
//
// Runs each task of `tasks.ts` in a fresh copy of the sample project, once
// with code mode off and once on, with the real engine in RPC mode and the
// approval mode `auto`, and reports model turns, input tokens, cost and
// whether the task was done. It uses the person's configured model and
// credentials (`~/.evo/agent`), so it spends real tokens: run it on purpose.
//
//   bun scripts/bench/code-mode/run.ts [--set a|b] [--provider p] [--model m] [--only id,id] [--out report.json]
//
// EVO_ENGINE_BIN overrides the engine (default: target/debug/evo-engine).
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { TASKS, writeProject, type Task } from "./tasks";
import { TASKS_B, writeProjectB } from "./tasks-b";

interface Run {
  task: string;
  codeMode: boolean;
  passed: boolean;
  turns: number;
  input: number;
  cacheRead: number;
  output: number;
  costUsd: number;
  ms: number;
  codeRuns: number;
  model?: string;
  /** Tool calls in order; a failed one ends with `!`. */
  tools: string[];
  /** First lines of each run_code result, to see why a program needed a retry. */
  programs: string[];
  error?: string;
}

const argv = process.argv.slice(2);
const flag = (name: string) => {
  const at = argv.indexOf(name);
  return at === -1 ? undefined : argv[at + 1];
};
const repo = resolve(import.meta.dir, "..", "..", "..");
const engine = process.env.EVO_ENGINE_BIN ?? join(repo, "target", "debug", process.platform === "win32" ? "evo-engine.exe" : "evo-engine");
const only = flag("--only")?.split(",");
/** `--set b`: the multi-step tasks of `tasks-b.ts`; set A (`tasks.ts`) by default. */
const set = flag("--set") === "b" ? "b" : "a";
const modelArgs = [...(flag("--provider") ? ["--provider", flag("--provider")!] : []), ...(flag("--model") ? ["--model", flag("--model")!] : [])];
const TIMEOUT_MS = 15 * 60 * 1000;

const textOf = (message: any): string =>
  (Array.isArray(message?.content) ? message.content : [])
    .filter((part: any) => part?.type === "text")
    .map((part: any) => part.text)
    .join("\n");

async function runTask(task: Task, codeMode: boolean): Promise<Run> {
  const dir = mkdtempSync(join(tmpdir(), `evo-bench-${task.id}-`));
  (set === "b" ? writeProjectB : writeProject)(dir);
  const started = Date.now();
  const run: Run = { task: task.id, codeMode, passed: false, turns: 0, input: 0, cacheRead: 0, output: 0, costUsd: 0, ms: 0, codeRuns: 0, tools: [], programs: [] };
  const child = Bun.spawn(
    [engine, "--mode", "rpc", "--no-session", "--approval-mode", "auto", ...(codeMode ? ["--code-mode"] : []), ...modelArgs],
    { cwd: dir, stdin: "pipe", stdout: "pipe", stderr: "ignore" },
  );
  let answer = "";
  try {
    const reader = child.stdout.getReader();
    const decoder = new TextDecoder();
    let buffer = "";
    let sent = false;
    const deadline = started + TIMEOUT_MS;
    settle: while (Date.now() < deadline) {
      const { value, done } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      let newline: number;
      while ((newline = buffer.indexOf("\n")) !== -1) {
        const line = buffer.slice(0, newline).trim();
        buffer = buffer.slice(newline + 1);
        if (!line.startsWith("{")) continue;
        const message = JSON.parse(line);
        if (message.type === "ready" && !sent) {
          sent = true;
          child.stdin.write(JSON.stringify({ type: "prompt", id: 1, message: task.prompt }) + "\n");
          child.stdin.flush();
        } else if (message.type === "message_end" && message.message?.role === "assistant") {
          run.turns += 1;
          const text = textOf(message.message);
          if (text) answer = text;
        } else if (message.type === "tool_execution_start" && message.toolName === "run_code") {
          run.codeRuns += 1;
        } else if (message.type === "tool_execution_end") {
          run.tools.push(`${message.toolName}${message.isError ? "!" : ""}`);
          if (message.toolName === "run_code") run.programs.push(textOf(message.result).split("\n").slice(0, 4).join(" | ").slice(0, 300));
        } else if (message.type === "agent_settled") {
          const turn = message.turn ?? {};
          run.input = Number(turn.input ?? 0);
          run.cacheRead = Number(turn.cacheRead ?? 0);
          run.output = Number(turn.output ?? 0);
          run.costUsd = Number(turn.costUsd ?? 0);
          if (turn.model) run.model = `${turn.provider ?? ""}/${turn.model}`;
          if (message.error) run.error = String(message.error);
          break settle;
        }
      }
    }
    run.passed = task.check(dir, answer);
  } catch (error) {
    run.error = String(error);
  } finally {
    child.kill();
    run.ms = Date.now() - started;
    // On Windows the engine holds its folder until it has exited.
    await child.exited.catch(() => undefined);
    for (let attempt = 0; attempt < 20; attempt++) {
      try {
        rmSync(dir, { recursive: true, force: true });
        break;
      } catch {
        await Bun.sleep(250);
      }
    }
  }
  return run;
}

const tasks = (set === "b" ? TASKS_B : TASKS).filter((task) => !only || only.includes(task.id));
const runs: Run[] = [];
for (const task of tasks) {
  for (const codeMode of [false, true]) {
    const run = await runTask(task, codeMode);
    runs.push(run);
    console.log(
      `${task.id.padEnd(20)} ${codeMode ? "on " : "off"}  ${run.passed ? "pass" : "FAIL"}  turns ${run.turns}  input ${run.input}  cached ${run.cacheRead}  $${run.costUsd.toFixed(4)}  ${(run.ms / 1000).toFixed(1)} s${run.codeRuns ? `  run_code ×${run.codeRuns}` : ""}${run.error ? `  error: ${run.error}` : ""}
    tools: ${run.tools.join(" ") || "-"}`,
    );
  }
}

const total = (on: boolean) => {
  const set = runs.filter((run) => run.codeMode === on);
  return {
    passed: set.filter((run) => run.passed).length,
    turns: set.reduce((sum, run) => sum + run.turns, 0),
    input: set.reduce((sum, run) => sum + run.input + run.cacheRead, 0),
    costUsd: set.reduce((sum, run) => sum + run.costUsd, 0),
  };
};
const off = total(false);
const on = total(true);
const less = (a: number, b: number) => (a > 0 ? `${Math.round((1 - b / a) * 100)}%` : "n/a");
console.log("");
console.log(`off: ${off.passed}/${tasks.length} passed, ${off.turns} turns, ${off.input} input tokens, $${off.costUsd.toFixed(4)}`);
console.log(`on:  ${on.passed}/${tasks.length} passed, ${on.turns} turns, ${on.input} input tokens, $${on.costUsd.toFixed(4)}`);
console.log(`code mode: ${less(off.turns, on.turns)} fewer turns, ${less(off.input, on.input)} fewer input tokens (target, set B: at least 10% fewer turns, no more input tokens, same or more passed)`);
const out = flag("--out");
const models = [...new Set(runs.map((run) => run.model).filter(Boolean))];
console.log(`model: ${models.join(", ") || "unknown"}`);
if (out) writeFileSync(out, JSON.stringify({ engine, model: modelArgs, models, runs, off, on }, null, 2));
