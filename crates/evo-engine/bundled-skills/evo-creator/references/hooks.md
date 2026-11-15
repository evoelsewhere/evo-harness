# Hooks

Commands (or web requests) evo runs on agent events. They run as the
person, unconfined, so they need the person's `hooks` permission and should
do one small, fast thing.

## Where

`hooks/hooks.json` (read anyway), more files or inline objects under
`"hooks"` in the manifest:

```json
{ "hooks": {
  "PreToolUse": [
    { "matcher": "bash|powershell",
      "hooks": [{ "type": "command", "command": "node", "args": ["${PLUGIN_ROOT}/hooks/guard.js"], "timeout": 10 }] }
  ],
  "Stop": [
    { "hooks": [{ "type": "http", "url": "https://hooks.example.com/done", "headers": { "Authorization": "Bearer ${secret.webhookToken}" } }] }
  ]
} }
```

| Handler | Fields |
| --- | --- |
| `command` | `command`; `args` (run directly, no shell — prefer this); without `args`, `command` runs in a shell (`sh`; on Windows Git Bash, else PowerShell; `shell: "powershell"` to choose); `timeout` in seconds (600 by default) |
| `http` | `url`, `headers` (`${secret.KEY}` allowed), `timeout` (60 by default); the event is POSTed as JSON, the answer read like a command's stdout |

`matcher`: tool names separated by `|` or `,`, or a regular expression,
matched against the tool's name (`bash` also matches `Bash`; `edit` matches
`Edit`); empty or `*` for all. For `SubagentStop` it matches the agent type.

## Events

Every event's JSON (on stdin, or the POST body) has `hook_event_name`,
`cwd` and `session_id`, plus:

| Event | Also has | It can |
| --- | --- | --- |
| `SessionStart` | `source`: `startup` or `resume` | add context to the first prompt (plain stdout) |
| `UserPromptSubmit` | `prompt` | block the prompt; add context (plain stdout or `additionalContext`). 30 s at most |
| `PreToolUse` | `tool_name`, `tool_input`, `tool_use_id` | deny, ask the person, allow, or rewrite the input |
| `PostToolUse`, `PostToolUseFailure` | the above, `tool_response` (text), `is_error` | add feedback or context to the result |
| `Stop` | `outcome`, `stop_hook_active` | runs after a turn, in the background |
| `SessionEnd` | `reason` | runs as the engine stops (1.5 s) |
| `SubagentStop` | `agent_id`, `agent_type`, `description`, `status` (`finished`, `failed`, `cancelled`, `stopped`), `report`, `worktree?` | nothing: it cannot block |

## Answering

- **Exit 0** with nothing: go on.
- **Exit 0** with JSON on stdout:

  ```json
  { "decision": "block", "reason": "…" }
  { "continue": false, "stopReason": "…" }
  { "systemMessage": "shown in the log" }
  { "hookSpecificOutput": {
      "permissionDecision": "deny" | "ask" | "allow",
      "permissionDecisionReason": "…",
      "updatedInput": { … },
      "additionalContext": "…" } }
  ```

- **Exit 2**: blocked; stderr is the reason the agent (or person) reads.
- **Any other exit code**: an error in the log, not a block.

## Writing one

- Use `args` with a runtime the person has (`node`, `python`) and a script
  in the plugin, not a long shell line: shells differ across systems.
- Read stdin to its end even when you do not need it.
- Be quick: a `PreToolUse` hook is in the path of every matching call.
- The plugin's hook commands get `EVO_PLUGIN_HOST` and `EVO_PLUGIN_KEY`
  for the host API (`references/backend.md`): settings, tokens, a log line.
- Customize → Hooks shows every hook and its recent runs with their
  results: check there after the person tries it.

## Example: a guard

```js
// hooks/guard.js — refuse `rm -rf /` and force pushes.
let input = "";
process.stdin.on("data", (c) => (input += c));
process.stdin.on("end", () => {
  const { tool_input } = JSON.parse(input);
  const command = String(tool_input?.command ?? "");
  if (/rm\s+-rf\s+\/(\s|$)|git\s+push\s+.*--force/.test(command)) {
    console.log(JSON.stringify({ hookSpecificOutput: { permissionDecision: "deny", permissionDecisionReason: "This plugin does not allow that command." } }));
  }
  process.exit(0);
});
```
