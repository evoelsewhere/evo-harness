# Evals

Evals of the Evo harness: what the harness does, not what models say
([spec](../docs/features/learning-layers/spec.md), FR-26 – FR-32;
[plan](../docs/features/learning-layers/plan.md), 3.9). The runner is the
`evo-evals` crate (`crates/evo-evals`).

```bash
cargo run -p evo-evals -- offline            # every scenario, golden replay and case set; no network
EVO_EVAL_LIVE=1 EVO_EVAL_MODEL=provider/model cargo run -p evo-evals -- live
cargo run -p evo-evals -- redact <channel.json> <golden.json>
cargo run -p evo-evals -- baseline evals/golden/channels/<name>.json
```

Each run writes a report, `reports/runs/<time>.json` and `.md` (not
committed). The offline tier is a CI gate; the live tier never runs in CI.

## Scenarios (`scenarios/*.json`)

A scenario names a **world**, a list of **steps** that drive the engine's
real code on a scripted model, and per step the **assertions** (`expect`)
on what happened. The worlds:

- `channel` — the coordinator's decisions: who answers a message
  (`message`), which mentions in a reply ask (`reply`), how a chain of
  hand-offs counts toward the limits (`handoffs`), what a mail shows
  (`mail`), a hand-off's kind (`handoffKind`), the router models' order
  (`routerOrder`), how a job ends (`jobEnd`).
- `memory` — an agent folder in a temporary directory: the agent's memory
  tool (`agent`), the panel's save (`person`), `undoLast`, `seed`, the
  tidy-up (`tidy`), the dream (`dream`) and the skill review (`review`),
  the model's answers scripted in `answers`.
- `chat` — the engine's model loops on a scripted model: the index run
  (`index`, the model's turns in `model`) and a learning step's call
  (`learnCall`).

Assertion kinds: `toolsOffered`, `toolCalled`, `routedTo`, `handoff`,
`notice`, `memoryHas`, `memoryLacks`, `skillHas`, `promptContains`,
`promptLacks`, `journalHas`, and `path` with `equals` or `contains`. A
failing one names its kind.

A scenario that keeps a fixed fault says so in `fault`.

## Golden channels (`golden/`)

`golden/channels/*.json` are real channels turned into data: `redact`
replaces names, ids and free text with placeholders (keeping mentions and
question marks, which routing reads) and refuses to write when its leak
scan finds an email, a phone number, a path with a user name or one of the
source's names. Replay runs the real routing and mention decisions over the
lines; `golden/baselines/<name>.json` is the trace it must match. After an
intended change, save a new baseline with `baseline` and review the diff.

## Case sets (`cases/`)

One decision each: `dream.json` (lines to keep and lines that must never be
kept — pasted instructions, a pasted key, disguised words), `router.json`
(who answers) and `tool-choice.json` (the tool a request should reach).
Offline they grade the saved `answer`; the live tier asks the model.

## Live tier (`live/`)

`EVO_EVAL_LIVE=1` and `EVO_EVAL_MODEL=provider/model` run the dream cases
on that model with time and cost per case, and a judge
(`live/judge/*.md`, each pinned by sha256 in `live/judge/manifest.json`; a
changed prompt is refused until its version and hash are updated). A run is
compared with the last one (`live/baselines/last.json`, not committed) and
fails when its pass rate drops by more than `live/config.json`'s
`maxDrift`.
