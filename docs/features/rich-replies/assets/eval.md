# Rich replies — block-use eval (SC-4, NFR-5)

Checks that the agent uses an `evo-ui` block when a reply compares numbers it
has from a tool result or from the person, and does not use one otherwise.

## How to run

1. In Evo, open a chat on this repository with the model under test (one
   run per provider; NFR-5 asks for two providers).
2. Send each question below in a **new chat** (so earlier blocks do not
   prime the next answer). Questions in set A that name a file or command
   let the agent use its tools; the pasted ones carry their data.
3. For each answer, record **block**: yes if the reply has at least one
   `evo-ui` block, no otherwise. Note any block with a number that is not in
   the tool result or the question.
4. Pass: set A has a block in at least 16 of 20 answers, set B in at most 2
   of 20, and no block shows a made-up number.

Without the app, each question can run in its own engine started as the app
starts it, saving no chat: `evo-engine --mode rpc --no-session --provider
<p> --model <m> --approval-mode auto --role-file <file with
{"access":"read-only"}>` from the repository root, then one line
`{"id":"q1","type":"prompt","message":"<question>"}` on its stdin; the answer
is the last assistant `message_end` before `agent_end`. (Print mode, `-p`,
overflows the main thread's stack in a debug build on Windows once a tool
runs.) Save each answer as `A01.txt` … `B20.txt` in one folder and score
them with `bun scripts/eval-blocks.ts <folder>` in `apps/desktop`: it counts
each answer's `evo-ui` blocks and checks the desktop draws them.

## Set A — numbers to compare (expect a block)

1. Count the lines of every `.rs` file in `crates/evo-store/src/` and show which are largest.
2. How many tests does each crate under `crates/` have? Use `grep -c "#\[test\]"`.
3. List the five largest files in `apps/desktop/src/features/chat/` by size.
4. Here are last week's costs: Mon 12.40, Tue 33.10, Wed 8.05, Thu 41.90, Fri 27.30, Sat 2.10, Sun 0.95 (USD). Which days stand out?
5. Count the commits per day in the last 200 commits (`git log -200 --format=%ad --date=short`).
6. How many entries do `dependencies`, `devDependencies` and `scripts` each have in `apps/desktop/package.json`?
7. Build times: debug 3m 12s, release 9m 40s, check 41s, clippy 2m 05s. Summarise them.
8. How many `FR-` requirements does each spec under `docs/features/` have? Top ten.
9. Response times per endpoint: /chat p50 120 ms p95 480 ms; /search p50 45 ms p95 210 ms; /files p50 18 ms p95 66 ms; /usage p50 300 ms p95 1.2 s. Which endpoint needs work?
10. Which ADRs under `docs/architecture/decisions/` are longest (lines)?
11. Monthly active users: Jan 1200, Feb 1350, Mar 1290, Apr 1610, May 1880, Jun 2050. How is it trending?
12. Count the `TODO` comments per crate.
13. Disk use: target/ 38.2 GB, node_modules 1.1 GB, dist 24 MB, docs 3.4 MB. What should I clean?
14. For each phase in `docs/features/rich-replies/tasks.md`, how many tasks are done and how many are open?
15. Error counts by type this week: timeout 42, 429 rate limit 118, 500 server 9, DNS 3, TLS 1. Break it down.
16. How many Markdown files does each folder under `docs/` hold?
17. Model prices per million tokens (input/output): A 3/15, B 0.8/4, C 15/75, D 0.25/1.25. Compare them.
18. List the five longest functions you can find in `crates/evo-engine/src/session.rs` by line count.
19. Test results: engine 849 passed 3 failed, agent 140 passed, web 61 passed 2 skipped, store 210 passed. Summarise.
20. Weekly downloads: v4.0 3,400; v4.1 5,100; v4.2 4,750; v4.3 6,900. Plot it.

## Set B — no numbers to compare (expect no block)

1. What does `crates/evo-mesh` do?
2. Explain the difference between `steer` and `followUp` queued messages.
3. Rename the function `send_with_retry` to `send_retrying` (just tell me where it is used).
4. Write a commit message for "fix the sidebar tooltip flicker".
5. What is the approval mode `auto-edits`?
6. Why does the desktop use bun only?
7. Summarise `DESIGN.md` in three sentences.
8. Translate "The build passed" into Vietnamese.
9. What is 17 × 23?
10. Is `evo-plugin-sdk` allowed to depend on `evo-engine`?
11. Draft a short reply to a user asking how to reset their API key.
12. What does the `Read` tool class mean for approvals?
13. Explain what a worktree is in two sentences.
14. Which file defines the tool table?
15. Suggest a name for a feature that lets people pin chats.
16. What is the current model's context window? (one number)
17. Fix the typo in "recieve".
18. How do I run the desktop app in dev mode?
19. What is the opposite of "verbose"?
20. Tell me a one-line joke about compilers.

## Changes

- 2026-11-19 — A5 (commits per author) and A6 (two package counts) replaced: in
  this repository their answers hold one and two values, below the
  three-value rule, so no block was the right answer.
- 2026-11-19 — A5 changed again, to commits per day: all 200 recent commits
  fall in one month, so "per month" also gave one value. The run below used
  the per-month wording.

## Results

| Date | Provider / model | Set A blocks (≥ 16) | Set B blocks (≤ 2) | Made-up numbers | Result |
| --- | --- | --- | --- | --- | --- |
| 2026-11-19 | copilot / claude-sonnet-5.5 (thinking low) | 15/20 | 0/20 | none (derived sums, ratios and unit changes only) | fail (A by 1) |
| 2026-11-19 | xiaomi / mimo-v2.6-pro (thinking low) | 18/20 (one block, A6, used `stats` for `items` and was shown as source) | 0/20 | none (derived values only) | pass |
| 2026-11-19 (index v3) | copilot / claude-sonnet-5.5 (thinking low) | 16/20 | 0/20 | none (ratios from the given figures only) | pass |
| 2026-11-19 (index v3) | xiaomi / mimo-v2.6-pro (thinking low) | 20/20 | 1/20 (B2: a text table comparing `steer` and `followUp`) | none (sums and shares of the given figures only) | pass |

Run through `--mode rpc` with a read-only role and the final index wording
(plan section 5). copilot's misses: A5 (one month, one value — a question
fault), A6 and A12 (three values written as a bullet list), A13 (advice
around four sizes), A18 (approximate counts as a Markdown table). Earlier
runs with weaker index wording: "You can put UI blocks" got prose and
Markdown tables with no `ui_guide` call; adding "not a Markdown table"
raised copilot's set A to 12/20, and "from a command you ran" to 15/20.

Index v3 (the current wording, plan section 3.3) says a Markdown table or a
`- name: number` list of such values is wrong "even in advice", and that a
rough count still goes in a block with its method in `source`. Against the
previous copilot run it gained A5 (now per day, so it has many values) and
A12, and lost A1 (eleven files as a Markdown table): 16/20, the threshold
exactly. copilot still misses A1, A6 (three counts as a bullet list), A13
(advice around four sizes) and A18 (a Markdown table of line ranges). The
first xiaomi pass hit the 480 s limit on 17 answers with six engines
running at once; those were rerun three at a time.
