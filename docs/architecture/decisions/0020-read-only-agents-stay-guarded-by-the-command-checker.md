# 0020. Read-only agents stay guarded by the command checker; no operating-system write restriction

| | |
| --- | --- |
| Status | Accepted |
| Date | 2026-11-11 |
| ADD sections | [9. Cross-cutting concepts](../README.md#9-cross-cutting-concepts) (permissions and sandbox) |
| Related | Supersedes [0019](0019-read-only-agents-run-under-an-os-write-restriction.md); [Nested sessions: roles, limits and mail](../../features/session-roles-and-mail/spec.md) FR-12 – FR-14 (dropped), FR-13 |

## Context

[0019](0019-read-only-agents-run-under-an-os-write-restriction.md) decided
to run read-only agents' shell commands under an operating-system write
restriction, behind the command checker. The Windows spike of 2026-11-11
(session-roles-and-mail, plan section 5) found that:

- a low-integrity token works for PowerShell, cmd and native programs;
- Git Bash (MSYS2) cannot start at low integrity at all;
- a write-restricted token breaks both shells;
- a private MSYS2 runtime would mean copying 93 MB per Git update.

The restriction would need three platform mechanisms, CI on each platform,
and a "fail closed" path that takes `bash` away from read-only agents on
Windows. Nothing had been built on top of 0019 yet.

## Decision

evo adds no operating-system write restriction. Read-only agents (subagents
and sessions of a read-only type) stay guarded as they are today:

- by the command checker (`read_only.rs`) before any approval prompt and
  again before the call runs;
- by the approval mode;
- for read-only sessions, by the shell gate refusing app tools that change
  something (session-roles-and-mail FR-13).

## Consequences

- No platform-specific sandbox code. `bash` stays available to read-only
  agents on every platform.
- "Read-only" remains as strong as the command checker. A command it
  misreads can still write. Hardening the checker is the way to close such
  gaps, as the audit of 2026-11-11 did.
- session-roles-and-mail drops FR-12, FR-14, NFR-5, NFR-6, US-7 and SC-6,
  and the tasks of Phases 0 and 4. Subagents T30 is dropped as well.
- ADD §9 is unchanged: the chain of checks stays as it is.

## Alternatives considered

| Option | Why not |
| --- | --- |
| Keep 0019 (OS restriction, fail closed) | Three mechanisms to build and test, `bash` lost to read-only agents on Windows, and gaps it cannot cover anyway (brokers, metadata); the owner decided the cost is not worth it now. |
| OS restriction on Windows only for PowerShell | A partial guard that differs by shell and platform, harder to explain than the checker alone. |
