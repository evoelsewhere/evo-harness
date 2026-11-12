# 0019. Read-only agents' shell commands run under an operating-system write restriction

| | |
| --- | --- |
| Status | Superseded by [0020](0020-read-only-agents-stay-guarded-by-the-command-checker.md) |
| Date | 2026-11-11 |
| ADD sections | [9. Cross-cutting concepts](../README.md#9-cross-cutting-concepts) (permissions and sandbox) |
| Related | [Nested sessions: roles, limits and mail](../../features/session-roles-and-mail/spec.md) FR-12 – FR-14; [Subagents and nested sessions](../../features/subagents-and-nested-sessions/spec.md) FR-3; [0010](0010-engine-enforces-approval-mode.md), [0014](0014-agent-tools-by-layer.md) |

## Context

`access: read-only` (subagents FR-3) is enforced by a command checker. It
parses shell commands and allows only listed reading commands. It has been
hardened twice, but it is still a parser that must read a command the way
bash or PowerShell would. Each misreading found so far was a way to write.

evo has no operating-system sandbox. "Sandbox" in evo means one process-wide
policy that the engine applies itself: timeouts, output size, environment and
denied file patterns. Shell commands run with the person's full rights. The
policy is global, so the parent and its read-only children share it.

Every shell command, bash or PowerShell, starts in one place:
`OsExecutionEnv::exec`. On Windows it already starts processes suspended and
puts them in a job object. A read-only child that works in a worktree gets
its own `OsExecutionEnv`. One that does not shares its parent's.

Nested sessions are about to get read-only agent types
([session-roles-and-mail](../../features/session-roles-and-mail/spec.md)
FR-1, FR-4). A whole engine will then be read-only, and it can reach app
tools answered by the shell (terminal, browser, connectors) that the
in-engine checker never sees.

## Decision

Behind the command checker, the shell commands of a read-only agent run under
an operating-system write restriction. They can read as before, and can write
only to the agent's own scratch folder and the null device. The restriction
is attached to the agent's own execution environment, never to the process
as a whole.

- **Per-environment policy.** `OsExecutionEnv` carries a write policy. A
  read-only subagent always gets its own `bash` and `powershell` built on an
  environment with the restricted policy (the existing "replacement tools"
  path). A read-only session's engine builds all its shell tools that way.
  The process-wide policy is unchanged and never switched per call.
- **Mechanism per platform**, chosen to need no administrator rights:
  - **Windows:** the command starts with a low-integrity copy of the user's
    token. The default no-write-up policy then refuses writes to the person's
    files and registry. The scratch folder is labelled low integrity, and the
    temp variables point to it.
  - **macOS:** the shell starts under `sandbox-exec` with a profile that
    denies `file-write*` except the scratch folder and `/dev/null`.
  - **Linux:** a Landlock ruleset (read everywhere, write the scratch folder)
    is applied in the child before it executes the shell.
- **Fail closed.** Where the mechanism is not available (a kernel without
  Landlock, a failed token setup), the command is refused with the reason,
  never run unprotected. **Settings → Sandbox** reports whether protection is
  available.
- **The shell gate refuses changing app tools to read-only sessions**
  (terminal input, browser actions, computer, scheduled-task changes,
  connector tools not marked read-only). Read-only agents get no language
  servers.
- **Order of checks** (ADD §9): the read-only checker first, as today. The OS
  restriction is the last layer and does not replace any check.

## Consequences

- A misreading by the checker no longer means a write. "Read-only" becomes a
  property the operating system enforces for shell commands.
- New platform code in `evo-tools`: a raw process spawn with a token on
  Windows, a profile wrapper on macOS, Landlock on Linux. Each needs a CI test
  that a restricted command cannot write.
- Gaps the OS layer does not cover:
  - brokers that write on a command's behalf (container daemons, WSL, COM or
    XPC services, credential helpers);
  - file metadata on Linux before Landlock ABI 3/5;
  - in-process engine tools.

  The checker keeps refusing the known brokers. The gaps are listed in
  **Settings → Sandbox** and the docs.
- Some reading tools may degrade under the restriction (MSYS temp files, git's
  optional index lock), so `GIT_OPTIONAL_LOCKS=0` and the scratch temp folder
  are set. Prototype runs on each platform come before the plan is accepted.
- The person's own chats and `full` agents are unchanged. An OS sandbox for
  them would be a separate decision.

## Alternatives considered

| Option | Why not |
| --- | --- |
| Keep the checker only | Every audit found new misreadings; a parser cannot be proved to read every command like the shell. |
| Windows AppContainer | Cannot read the workspace or the person's folders without changing their ACLs. |
| Linux bubblewrap | Needs unprivileged user namespaces, which several distributions restrict by default; Landlock needs no namespace. |
| A process-wide restriction for the whole engine | The engine writes its transcript, caches and worktrees, and the parent and `full` children share the process. |
| Run read-only agents in a container or VM | Heavy, platform-specific installs, and slow to start for each command. |
