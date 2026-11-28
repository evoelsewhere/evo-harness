# Learning layers: memory upkeep, dream, retro and evals — Plan

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Tasks | [tasks.md](tasks.md) |
| Updated | 2026-11-28 |
| ADD sections | [4 Process view](../../architecture/README.md#4-process-view), [5.1 Crate layers](../../architecture/README.md#51-crate-layers), [7 Data view](../../architecture/README.md#7-data-view), [9 Cross-cutting concepts](../../architecture/README.md#9-cross-cutting-concepts) |
| ADRs | [0028 Two front ends in one app process](../../architecture/decisions/0028-two-front-ends-in-one-app-process.md) |

## 1. Context and constraints

What exists (facts, 2026-11-28):

- **Memory topics.** One `<name>.md` per topic under `<agent dir>/memory/`
  (personal), `<agent dir>/projects/<slug>/memory/` (project), or, for a
  mate, the role's `memoryDir` = `<agent dir>/mates/<id>/memory`
  (`crates/evo-engine/src/memory.rs:171-201`,
  `apps/desktop/src-tauri/src/mates/mod.rs:75-82`). Frontmatter `name`,
  `description`, `type`, `updated`, `by`, `sensitive` (`topic_file`
  :323-343); `MEMORY.md` is a regenerated index (:301-316). Only the index
  lines reach the prompt (`prompt_text_as` :559-623); a topic's body is
  read with the `memory` tool. Topics have **no per-entry structure**.
- **The change list** is table `memory_changes`
  (`crates/evo-store/src/migrations/0011_memory_topics.sql:35-46`,
  `crates/evo-store/src/memory_log.rs`): CHECKs allow scope
  `personal|project` and action `added|updated|deleted`; no author or
  owner column; `KEEP=200` across the whole table. Only the index run
  records rows (`tools/memory.rs:77-84`); an undo writes no row.
- **Indexing** runs as `evo-engine --mode index` (`memory_index.rs`
  :393-486) with the `memory` and `set_summary` tools, started by the
  shell's loop (`apps/desktop/src-tauri/src/memory_index.rs` :308-386, TICK
  30 s) on chats of `kind='chat'` only; keys `memory.autoIndex`,
  `memory.indexModel`, `memory.indexIdleMinutes`, `memory.indexSince`.
  Usage source `memory`.
- **One-shot calls.** `side_questions::ask_once` runs on the parent chat's
  model (`side_questions.rs:557-584`); `provider.stream_simple` is called
  directly by the index run (:286).
- **Usage sources** `chat|subagent|plugin|side|memory|mates`
  (`usage_log.rs:32-55`; DB CHECK in `0015_mates.sql:88-89`); a new source
  needs a table rebuild (as `0012_usage_memory_source.sql`).
- **Skills** are found in project `.evo/skills`, `.agents/skills`,
  `<agent dir>/skills`, `~/.agents/skills`, then bundled
  (`resource_dirs.rs:275-305`); a mate's skills are a filter on that list
  (`mates/mod.rs:88-91`). SKILL.md: `name` = folder name, `description`
  required (`crates/evo-harness/src/skills.rs:650-750`).
- **Background loops** in the shell: Scheduler (`agent_tools/scheduler/
  run.rs` :318), mates heartbeat (`mates/initiative.rs` :127), the index
  loop. No clock-time (nightly) runner. Idle: `EngineState::is_working`
  (`engine.rs:719`), `supervisor::working` (:842); `agent_settled` hook
  (`supervisor.rs:1450`).
- **Channels.** A job starts in `Coordinator::turn`
  (`channel/coordinator.rs:432`) and ends at nobody picked, cancelled,
  30 min silent, at the limit, or all replied (:472-662, :822-932). Lines
  are custom entries of type `channel` in the root's transcript (`post`
  :414-422); the UI adds a row kind in `lib/mates.ts:195-209`,
  `channel-rows.ts`, `channel-view.tsx`.
- **Scripted model:** `crates/evo-ai/src/providers/faux.rs`
  (`FauxScript`); an RPC engine driven by faux in
  `modes/turn_totals_tests.rs`. No end-to-end channel test with faux exists;
  hub channel tests are in `crates/evo-mesh/src/hub.rs` ~3017-3100.
- **No secret detector** by text shape exists; only header and prompt
  checks.

Research: OpenMausBot (Apache 2.0, read 2026-11-28 at e1cc6a2):
`docs/memory.md`, `server/memory-dream.ts`, `server/memory-upkeep.ts`,
`server/memory-fact-checks.ts`, `server/skill-review.ts` (its prompt adapted
from an MIT project, Nous Research's background review), `evals/`. The
design, limits and prompt structure come from there; the code is Evo's.
NOTICE credits both (A-4).

## 2. Architecture check

| Rule | Result | Notes |
| --- | --- | --- |
| The engine owns agent logic; the shell relays and provides native features | ok | Every model step (tidy, dream, review, retro) runs in the engine as `evo-engine --mode learn`, like `--mode index`. The shell only times them (clock, idle) and starts the process. |
| Layers point down; library crates stay free of engine/desktop concerns | ok | Entry parsing, redaction and the journal live in `evo-engine` and `evo-store`; `evo-evals` is a dev-only crate on top of the engine and `evo-mesh`. |
| Protocol changes update `evo-protocol`, the engine and the desktop together | ok | New channel entry kind `retro`, RPC `channel_post_retro`, `learn` mode args; UI types in the same commits. |
| Models come from models.dev; no hardcoded model facts | ok | The Background model is a `{provider, modelId}` setting resolved through the catalogue; no temperature. |
| Real engine in the app; settings and credentials reload live | ok | Settings are read at each tick; a switch applies at the next due step. |
| Untrusted input stays untrusted; permissions, sandbox and project trust stay in the path | ok | Steps are tool-free (NFR-1); read text is fenced as data; quote checks and redaction before disk (FR-8, FR-9). |
| Desktop UI structure and `DESIGN.md` | ok | New panels follow the store/selector rule and the existing Memory and mate panels. |

## 3. Design

### 3.1 Overview

```text
 shell (apps/desktop/src-tauri/src/learning/)
   clock loop (60 s) ── nightly per owner ─► evo-engine --mode learn --task tidy|dream --owner …
   agent_settled     ── per chat         ─► evo-engine --mode learn --task review --chat …
   channel job_end   ── 2 min quiet      ─► evo-engine --mode learn --task retro --chat <root>
                                              │ stdout: JSON result
                                              ▼
                         retro card ─► RPC channel_post_retro to the channel's root engine
 engine (crates/evo-engine/src/learning/)
   entries.rs   parse/write dated entries, until, superseded, archive
   redact.rs    secret shapes → «redacted N chars»
   quotes.rs    quote and number/path/URL checks
   tidy.rs  dream.rs  review.rs  retro.rs   prompt + parse + apply
   journal  → evo-store::journal (memory and skill rows, undo)
 evals (crates/evo-evals, dev-only)  scenarios/*.json → worlds → frozen evidence → scorers
```

Owners: `personal`, `project:<slug>`, `mate:<id>`. An owner has a memory
folder, a learned-skills folder, a journal and a learning state (last tidy,
last dream, review counters).

### 3.2 Components

| Layer | Path | Responsibility | Requirements |
| --- | --- | --- | --- |
| store | `crates/evo-store/src/migrations/0018_learning.sql` | `journal` (replaces `memory_changes`, rows copied), `learning_state`, `learning_suggestions`; usage sources `upkeep`, `dream`, `review`, `retro` (table rebuild) | FR-1, FR-2, FR-14, NFR-4, NFR-6 |
| store | `crates/evo-store/src/journal.rs` (replaces `memory_log.rs`) | record / list per owner and subject / get / mark undone, keep 500 per owner and subject | FR-1, FR-2, FR-18, NFR-6 |
| store | `crates/evo-store/src/learning.rs` | state per owner; suggestions (add, list pending, accept, decline) | FR-6, FR-10, FR-14 |
| engine | `crates/evo-engine/src/learning/entries.rs` | entry grammar (3.4); `visible(text, today)` hides expired and struck lines; archive moves | FR-4, FR-5, FR-6 |
| engine | `crates/evo-engine/src/learning/redact.rs` | secret shapes: `sk-…`, `ghp_…`, `xox…-`, `AKIA…`, JWTs, `-----BEGIN … PRIVATE KEY-----`, `password: …`, `token=…`, long base64/hex after `key`/`secret` | FR-9 |
| engine | `crates/evo-engine/src/learning/quotes.rs` | each cited chat holds its quote (whitespace- and case-folded); every number, path and URL of the fact is in the quotes | FR-8 |
| engine | `crates/evo-engine/src/memory.rs`, `tools/memory.rs` | every write goes through redaction and records a journal row with its author (`agent`, `person`, `index`, `tidy`, `dream`, `undo`); `read` returns `visible` text; `until` field on save/update; hash-checked save for the panel | FR-1 – FR-5, FR-9 |
| engine | `crates/evo-engine/src/memory_index.rs` | the index memory tool takes `evidence` (quotes) for `user` facts and refuses without a passing check | FR-8 |
| engine | `crates/evo-engine/src/learning/{tidy,dream,review,retro}.rs` | prompt, tool-free call, parse, checks, apply, journal; result JSON on stdout | FR-6 – FR-22 |
| engine | `crates/evo-engine/src/learning/mod.rs`, `modes.rs`, `args.rs` | `--mode learn --task … --owner … [--chat …]`, model resolution (Background model, else the owner's), usage source per task | FR-23 – FR-25, NFR-1, NFR-4, NFR-5 |
| engine | `crates/evo-engine/src/resource_dirs.rs`, `customize.rs` | learned-skills folder in the skill search order, after `<agent dir>/skills`, marked `learned` in Customize | FR-15 |
| engine | `crates/evo-engine/src/channel/coordinator.rs` | a `job` entry when a job ends: status, hand-off count, notices, the job's first and last line ids; RPC `channel_post_retro` posts a `retro` entry | FR-19 – FR-21 |
| shell | `apps/desktop/src-tauri/src/learning/{mod,clock,jobs}.rs` | the clock loop, nightly due per owner (after 03:00 local, owner idle), review on `agent_settled`, retro after 2 min quiet, one process per owner at a time, 3-minute limit, failure counts | FR-6, FR-10, FR-16, FR-19, NFR-2, NFR-5 |
| shell | `apps/desktop/src-tauri/src/memory_index.rs` | reads the Background model key; migrates `memory.indexModel` once | FR-24 |
| shell | `apps/desktop/src-tauri/src/learning/commands.rs` | `learning_journal`, `learning_undo`, `learning_tidy_now`, `learning_suggestions`, `learning_suggestion_answer`, `learned_skills`, `learned_skill_undo`, `learned_skill_delete`, `channel_run_retro`; notification for a retro after a limit, stop or failure | FR-2, FR-6, FR-14, FR-15, FR-18, FR-19 |
| shell | `apps/desktop/src-tauri/src/mates/mod.rs` | role gains `learnedSkillsDir` = `<agent dir>/mates/<id>/skills`; per-mate switches | FR-15, FR-25 |
| UI | `apps/desktop/src/features/memory/` | journal list with source and Undo, conflict-safe editor, **Tidy now**, Suggestions | FR-1 – FR-3, FR-6, FR-14 |
| UI | `apps/desktop/src/features/settings/sections/{memory,mates}-settings.tsx` | Background model (renamed), Memory upkeep, Learn skills automatically, review step count, Learn from channels | FR-22 – FR-24, NFR-3 |
| UI | `apps/desktop/src/features/customize/`, `features/mates/{skills-tab,mate-panel}.tsx` | learned skills with switch, Undo last change, Delete; mate switches | FR-15, FR-25 |
| UI | `apps/desktop/src/features/chat/`, `features/mates/conversation.tsx` | "Learned · <skill>" line under a reply; suggestion card in a mate's conversation | FR-14, FR-18 |
| UI | `apps/desktop/src/lib/mates.ts`, `features/mates/channel-rows.ts`, `channel-view.tsx` | `retro` row kind (card with lessons and Undo); **Run retro** in the channel menu | FR-19 – FR-21 |
| evals | `crates/evo-evals/` (`publish = false`), `evals/` at the root for data | runner, worlds, scorers, redaction, case sets, live tier, report | FR-26 – FR-32, NFR-7 |
| CI | `.github/workflows/ci.yml` | `cargo run -p evo-evals -- offline` as a job | FR-32 |
| docs | `NOTICE` | credit OpenMausBot (Apache 2.0) and the MIT review prompt | A-4 |

### 3.3 Interfaces

Engine mode (stdout is one JSON line):

```text
evo-engine --mode learn --task tidy   --owner personal|project:<slug>|mate:<id> [--provider p --model m]
evo-engine --mode learn --task dream  --owner …
evo-engine --mode learn --task review --owner … --chat <id> --steps <n>
evo-engine --mode learn --task retro  --chat <channel root> --job <job entry id>
→ {"task":"dream","changed":[{"subject":"memory","name":"preferences","row":42}],
   "suggestions":[17], "skipped":"no model"|null, "error":null}
→ retro: {"task":"retro","card":{"job":"…","well":["…"],"wrong":["…"],
   "lessons":[{"mate":"m1","skill":"handing-off-research","row":51,"text":"…"}]}}
```

Model answers (parsed in Rust, everything else dropped):

```json
{"learned":[{"text":"…","from":[1,3],"evidence":["…","…"],"aboutUser":false,"topic":"writing","confidence":0.8}],
 "outdated":[{"line":4,"now":"…","from":[2],"evidence":["…"]}],
 "ideas":[{"text":"…","from":[1,4],"kind":"schedule|skill"}]}
{"changes":[{"action":"create|update","skill_name":"…","skill_md":"---\nname: …","gist":"…"}],"note":"…"}
{"well":["…"],"wrong":["…"],"lessons":[{"mate":"<id>","text":"…"}]}
{"contradictions":[{"older":3,"newer":7,"keep":"part still true or empty"}]}
```

RPC (to a channel's root engine): `channel_post_retro {card}` → posts
`{"retro": card}`. Channel entry `job`: `{"job": {"id", "status":
"completed|stopped|limit|failed|silent", "handoffs": n, "first", "last"}}`.

Tauri commands (shell → UI) as listed in 3.2; events `learning://changed
{owner}`.

Role field: `learnedSkillsDir: string` (beside `memoryDir`); older engines
ignore it and see no learned skills.

### 3.4 Data and storage

Entry grammar inside a topic body (any other line is kept as written):

```text
- 2026-11-28 · from chat "Plans" · The person wants drafts under 150 words
- 2026-11-28 · from chat "Trip" · The person is in Hanoi · until 2026-12-15
- ~~2026-10-02 · lives in Hanoi~~ · superseded 2026-11-28
- 2026-11-28 · dream · The person usually asks for numbers on Mondays
```

`<memory dir>/archive.md` holds moved entries with `· moved <date>`; it is
not a topic (excluded from the index and recall).

Learned skills: `<agent dir>/learned-skills/<name>/SKILL.md` (Harness),
`<agent dir>/mates/<id>/skills/<name>/SKILL.md` (a mate). A project's
learned skills are not separate: Harness learns per person.

Migration `0018_learning.sql`:

```sql
CREATE TABLE journal (
  id INTEGER PRIMARY KEY, at_ms INTEGER NOT NULL,
  owner TEXT NOT NULL,                 -- personal | project:<slug> | mate:<id>
  subject TEXT NOT NULL CHECK (subject IN ('memory','skill')),
  name TEXT NOT NULL,
  action TEXT NOT NULL CHECK (action IN ('added','updated','deleted','archived','undone')),
  by TEXT NOT NULL CHECK (by IN ('agent','person','index','tidy','dream','review','retro','undo')),
  chat_id TEXT, reply_id TEXT, diff TEXT, before_text TEXT, after_text TEXT,
  undoable INTEGER NOT NULL DEFAULT 1, undone_at INTEGER);
INSERT INTO journal (…) SELECT … FROM memory_changes;   -- by='index'
DROP TABLE memory_changes;
CREATE TABLE learning_state (owner TEXT PRIMARY KEY, tidied_on TEXT, dreamed_at_ms INTEGER,
  failures INTEGER NOT NULL DEFAULT 0, review_steps TEXT NOT NULL DEFAULT '{}');
CREATE TABLE learning_suggestions (id INTEGER PRIMARY KEY, owner TEXT NOT NULL, kind TEXT NOT NULL,
  text TEXT NOT NULL, from_chats TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'pending', at_ms INTEGER NOT NULL);
-- usage: rebuild with source CHECK adding 'upkeep','dream','review','retro'
```

Settings (`app_settings`): `learning.backgroundModel` `{provider, modelId}`
(copied once from `memory.indexModel`), `learning.harness.upkeep` (bool,
false), `learning.harness.skills` (bool, false), `learning.mates.upkeep`
(bool, true), `learning.mates.skills` (bool, true), `learning.reviewSteps`
(3–100, 10), `mates.learnFromChannels` (bool, false). Per mate: the mate
row's existing JSON gains `learning: {upkeep?: bool, skills?: bool}`
(no migration: stored in the mate's settings JSON).

### 3.5 Behaviour

- **Writes.** Every topic write (tool, panel, index, tidy, dream, undo)
  goes `redact → write → journal row`. The panel save sends the hash it
  opened; a mismatch returns the current text (FR-3).
- **Tidy** (nightly, or Tidy now): for each topic, archive entries whose
  `until` is past; drop the older of exact duplicates (spacing, bullet and
  final stop ignored); then, with a model, ask for contradicting pairs among
  unprotected dated entries (FR-7 filter runs before the prompt), strike at
  most a fifth per topic (none in a topic of fewer than five entries),
  keep the still-true part as a new entry. Journal rows `by=tidy`.
- **Dream** (after the owner's tidy that night): collect chats (FR-10)
  newest first, each cut to its latest lines within 4,000 bytes, 40,000 in
  all; prompt with the owner's dated entries numbered and the topic list;
  parse; for each learned or outdated item run `quotes` and `redact`;
  apply; ideas → `learning_suggestions`. Mates with Learn from channels
  also get the stretches of their channels under the channel rules (FR-22).
- **Review** (after `agent_settled`): the shell counts tool steps per chat
  since the last review from the transcript; at ≥ the step count it starts
  the review with the conversation tail (40 messages, 40,000 chars) and the
  owner's skills (learned ones in full up to 30,000 chars). Changes are
  applied only when FR-16/FR-17 hold; each writes `SKILL.md` atomically and
  a journal row `subject=skill` with the chat id and the id of the reply
  that ended the reviewed stretch; the UI shows the "Learned" line under
  that reply from the journal (nothing is written into the chat's
  transcript from outside its engine).
- **Retro**: the coordinator writes a `job` entry when a job ends; the
  shell waits until 2 minutes pass with no new channel line, then starts the
  retro (or at once from Run retro). Lessons go through `review` apply rules
  into each mate's learned skills; the card goes back through
  `channel_post_retro`. A retro of a job that ended at a limit, stopped or
  failed also sends a notification.
- **Concurrency.** One learn process per owner; a running review for a chat
  makes the next one for that chat wait. A message to a chat never waits on
  a learn process (it reads files; writes are atomic renames).

### 3.6 Errors and edge cases

- No model, or its provider refuses: tidy runs its model-free part; dream,
  review and retro skip with `skipped: "no model"`; the settings show it.
- Timeout (3 min) or an unreadable answer: nothing applied, one log line,
  `failures += 1`; at 3 the batch is dropped (state moves on).
- A topic changed between read and apply: the apply re-reads and skips that
  topic (no lost writes).
- Memory paused, chat memory off, incognito: excluded from every read.
- Engine crash during apply: writes are per file and atomic; the journal row
  is written after the file, so a missing row means an unrecorded change at
  worst, never a lost one.

### 3.7 Security and trust

- Tool-free calls: the learn mode builds requests with no tools (NFR-1).
- All read text is wrapped as data with the instruction to ignore requests
  inside it; output is parsed as JSON with fixed fields and limits.
- Quote checks (FR-8) and redaction (FR-9) run in Rust after the model,
  before any write.
- The journal lives in `evo.db`, outside the folders the agent's file tools
  can reach.
- Learned skills can never shadow an installed skill of the same name (the
  installed folder wins; a review cannot create a taken name).

### 3.8 Configuration

As in 3.4. All keys are read when a step is due, so changes apply without a
restart. Settings › Memory (Harness) holds Background model, Memory
upkeep, Learn skills automatically and the step count; Settings › Evo Mates
holds the Mates switches and Learn from channels; the mate panel holds the
per-mate switches.

### 3.9 Evals

`crates/evo-evals` (dev-only binary `evo-evals`):

- `worlds/` — `chat` (one RPC engine on faux), `channel` (in-process
  `evo-mesh` hub, a coordinator, faux seats), `memory` (an agent dir in a
  temp folder).
- `scenarios/*.json` under `evals/scenarios/`: `{world, mates, channels,
  turns (scripted per mate), steps, assertions}`; assertions per FR-27
  (`toolsOffered`, `toolCalled`, `routedTo`, `handoff`, `notice`,
  `memoryHas`, `skillHas`, `promptContains`).
- `golden/` — `redact` (ids, names, free text → placeholders; leak scan for
  emails, phone numbers, paths with a user name, and the source's names;
  refuses on a hit), trace extraction (turn order, tool names, hand-offs,
  routes) and replay against `evals/golden/baselines/*.json`.
- `cases/` — `dream.json`, `router.json`, `tool-choice.json`; offline
  scoring of saved answers; live runs score the model's answers with time
  and cost.
- `live/` — gated by `EVO_EVAL_LIVE=1`, model from `EVO_EVAL_MODEL`
  (`provider/model`), judge prompts in `evals/live/judge/*.md` pinned by
  sha256 in `manifest.json`; baseline under `evals/live/baselines/`
  (git-ignored); drift threshold in `evals/live/config.json`.
- Report: `evals/reports/runs/<time>.{json,md}` (git-ignored).

## 4. Alternatives considered

| Option | Why not |
| --- | --- |
| Run the steps in the shell with `ask_once`-like calls | The engine owns agent logic; the index run already set the pattern of a short-lived engine mode. |
| Keep `memory_changes` and add a second table for skills | Two journals, two undo paths; one table with `subject` is simpler and the old rows migrate. |
| Store entries as JSON lines beside each topic | Topics are person-readable Markdown today; a bullet grammar keeps them readable and hand-editable, and unparsed lines are kept as written. |
| The retro as a message the lead answers | A message wakes mates and costs a turn each; the card is one call and wakes nobody (FR-21). |
| Evals in TypeScript as OpenMausBot does | The coordinator, hub and memory are Rust; the faux provider is Rust; one language keeps scenarios close to the code they test. |

## 5. As-built notes

- 2026-11-28 — **Dated entries.** Text the agent adds to a topic stays as
  written unless it ends on a known day (`until`); text indexing adds and
  everything the dream, the tidy-up and channel learning write is a dated
  entry (`from chat "<title>"`, `dream`, …). Plain lines are never touched
  by the tidy-up, so a line the person or the agent wrote without a date is
  as safe as a hand-written one (FR-7).
- 2026-11-28 — **Journal texts.** A row keeps the whole file before and
  after, as `memory_changes` did; when either held a secret or is longer
  than 200,000 characters, both are dropped and the row is not undoable
  (FR-2). The `diff` column holds the changed lines, redacted.
- 2026-11-28 — **Panel conflict.** `save_memory` takes the `hash` the panel
  got with the topic; on a mismatch it writes nothing and answers the usual
  memory state plus `conflict: {current}`.
- 2026-11-28 — **A mate's chats.** A mate has one conversation, so the dream
  counts each local day of it with the person's lines as one chat (FR-10);
  with Learn from channels, each day of a channel the mate is in counts the
  same way. Channel lines come from the channel root's `channel` entries,
  not from the seats: another mate's line is shown to the model marked as
  not the person's words and is never quotable (FR-22). There is no
  separate capture pass: channels are read by the nightly dream only.
- 2026-11-28 — **Per-mate switches** live in the app setting
  `learning.mate.<id>` = `{upkeep?, skills?}` (the mate row has no JSON
  column); absent means the app's switch decides (FR-25).
- 2026-11-28 — **Owners for the clock.** Harness owners are `personal` and
  every `projects/<slug>` folder with a memory folder; they count as working
  while any chat runs. The shell counts chats roughly and starts the dream;
  the engine counts exactly and answers `skipped: "too few chats"`, after
  which that owner is not tried again that day.
- 2026-11-28 — **Process limit.** Each model call stops after three minutes
  (NFR-5); the shell stops a learn process after ten, since a tidy-up makes
  one call per topic.

