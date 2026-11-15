# Skills, commands and subagents

The plain parts of a plugin: text files evo reads, with no code to run and
no permission to ask for. A plugin of only these needs no consent.

## Skills

`skills/<name>/SKILL.md` (more folders with `"skills": ["extra/"]`). evo
shows each as `<plugin>:<name>` and offers it to the agent by its
description; the agent loads the body when the task matches.

```markdown
---
name: triage
description: Sorts a bug report into severity and area, and drafts the reply. Use when the person pastes a bug report or asks to triage an issue. Not for feature requests.
---

# Triage

1. Read the report; find the version, the steps and the expected result.
2. ...
```

| Front matter | Rule |
| --- | --- |
| `name` | the folder's name: lowercase `a-z`, `0-9`, single hyphens, at most 64 characters |
| `description` | at most 1024 characters: what it does, **when to use it** (the words a person would use), and when not to |
| `disable-model-invocation: true` | only the person starts it (`/plugin:name`), never the agent on its own |
| `requires-tools` | tools it needs (a list, or comma-separated); offered only when they are on |

Writing one well:

- The description decides whether it is ever used. Start with the verb,
  name the triggers, name the near-misses.
- The body is instructions to the agent in the imperative: steps, checks,
  what to say to the person. Keep it under about 500 lines.
- Put long material in files beside it (`references/<topic>.md`,
  `scripts/<name>.py`, `assets/`) and say in the body when to read or run each one: they cost
  nothing until then.
- Scripts the agent runs go through its shell tools: give the command in
  full, with the script's path relative to the skill's folder (the agent
  knows where the skill lives).

## Commands

`commands/<name>.md`; a subfolder makes `<folder>/<name>`. The person
types `/<plugin>:<name> words…`.

```markdown
---
description: Draft release notes from the commits since a tag
argument-hint: <tag>
allowed-tools: bash, read
---

Write release notes for the changes since $1. Group them by area...
```

| Placeholder | Becomes |
| --- | --- |
| `$ARGUMENTS` | everything after the command |
| `$1`, `$2`, … | one word each |
| `$@` | all words; `${@:2}` from the second on |

A command is a prompt the person reuses; a skill is know-how the agent
picks up by itself. When in doubt, make a skill.

## Subagents

`agents/<name>.md`: a type the agent can hand work to (the `agent` tool).

```markdown
---
name: reviewer
description: Reviews a diff for bugs and risky changes. Use after editing code, before saying it is done.
tools: [read, grep, find, ls, bash]
access: read-only
thinking: high
maxTurns: 30
---

You review changes. Read the diff, then ...
```

| Front matter | What |
| --- | --- |
| `name`, `description` | as for skills: the description says when to use it |
| `tools` | the tools it may use (omit for all of the parent's) |
| `access: read-only` | it may not change files or run commands that change things |
| `isolation: worktree` | it works in its own Git worktree |
| `model` | `inherit` (or omit) — never a fixed model id |
| `thinking` | a thinking level, or `inherit` |
| `maxTurns` | a limit on its steps |

The body is its system prompt: who it is, what it returns (a short report
the parent can act on), what it must not do.
