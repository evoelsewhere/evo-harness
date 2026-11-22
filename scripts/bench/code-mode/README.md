# Code mode benchmark

Measures what code mode changes ([spec](../../../docs/features/code-mode/spec.md),
SC-1): model turns, input tokens and cost for 12 multi-step tasks, each run
once with code mode off and once on, and whether each task was done.

| File | What it does |
| --- | --- |
| `tasks.ts` | The sample project (written into a temporary folder per run) and the 12 tasks, each with a check on the files or the final answer. |
| `tasks-b.ts` | Set B (`--set b`): 12 multi-step tasks (change, run a check, fix, write a report) on the same project plus check scripts. The target is measured here. |
| `run.ts` | Starts the real `evo-engine` in RPC mode for each task (`--approval-mode auto`, `--no-session`, and `--code-mode` for the second run), sends the prompt, reads the turn's tokens from `agent_settled` and counts assistant messages. |

## Run

It uses your configured model and credentials (`~/.evo/agent`) and spends
real tokens, so run it on purpose and with one fixed model:

```sh
cargo build -p evo-engine
bun scripts/bench/code-mode/run.ts --set b --provider <provider> --model <model> --out bench.json
bun scripts/bench/code-mode/run.ts --only rename-option,broken-links   # set A
```

`EVO_ENGINE_BIN` points it at another engine build.

## Targets

On set B, code mode on should need at least 10% fewer model turns and no
more input tokens (input plus cache reads) than off, and pass at least as
many tasks. Set A mostly takes one shell command per task, so with code mode
off it is already near the floor of two turns per task (one call, one
answer); it shows whether code mode costs extra on such work.
Record the result, with the model and date, in the feature's
[tasks.md](../../../docs/features/code-mode/tasks.md) verification table.
