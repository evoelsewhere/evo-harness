# Plugin settings, secrets and runtime — Spec

| | |
| --- | --- |
| Status | In progress |
| Area | desktop shell · desktop UI · engine · crate `evo-plugins` · crate `evo-mcp` |
| Created | 2026-11-14 |
| Updated | 2026-11-14 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [Plugin UI](../plugin-ui/spec.md) · [Customize: skills, plugins, connectors](../customize-skills-plugins-connectors/spec.md) · [ADR 0021](../../architecture/decisions/0021-plugin-views-run-in-native-webviews.md) |

## 1. Summary

A plugin's views already run behind permissions the person approves. The
rest of a plugin does not: its connector servers and hooks run as soon as
it is on, with the person's full rights and free network access, and the
secrets it asks for are kept in plain text. This feature gives every plugin
one place to set it up and control it — its settings (personal or per
workspace), its secrets (kept in the operating system's keychain), its
permissions (for its running code as well as its views) and its runtime
(when it starts, whether it runs, its log). It also lets a plugin ship a
backend in TypeScript, JavaScript or Python that evo runs for it, starting
it only when needed, and for TypeScript and JavaScript blocks network access
to anything the person did not allow. Plugins that work with an online
service get sign-in and sessions handled by evo — OAuth in the browser,
device codes, API keys, or logging in to a web page — with tokens and
cookies kept safe, renewed, and handed only to the plugin's own code.

## 2. Scope

### 2.1 Goals

- One page per plugin with its overview, settings, permissions, runtime and
  data.
- Settings described by the plugin and drawn by evo: more field types,
  groups, checks, and values per workspace.
- Secrets in the operating system's keychain, given only to the plugin's own
  processes; none left in evo's files.
- Permissions for a plugin's running code: running a backend, running
  hooks, receiving secrets, starting with evo, and the network origins it
  may reach; optional permissions asked for when needed.
- Plugin backends in TypeScript/JavaScript (run by Deno) and Python (run by
  uv), started on demand, stopped when idle, restarted when they crash.
- Network rules that are enforced for TypeScript/JavaScript backends and
  plainly marked as not enforced for others.
- Plugins installed before this feature are reviewed once.
- Sign-in and sessions for online services: OAuth (browser, PKCE),
  device codes, API keys and tokens, and logging in to a web page (for
  sites with no OAuth); several accounts; renewal; expiry; sign-out;
  sign-in asked for when the agent or a view needs it.

### 2.2 Non-goals

- Blocking network or files for Python, native binaries, hooks and existing
  connector servers with an operating-system sandbox (AppContainer, bwrap,
  macOS profiles) — a later feature; here they are declared and consented
  to, and marked as not enforced.
- Isolating in-process (`cdylib`) plugins — not possible; unchanged.
- Signing plugins or vetting marketplaces.
- Creating plugins with the agent (Creator Mode, `/create-plugin`) — its own
  feature.
- Views: their permissions and bridge stay as in [Plugin UI](../plugin-ui/spec.md);
  this feature adds bridge calls for the backend and sign-in (FR-14, FR-27).
- Receiving webhooks or other calls from the internet into a plugin.
- Sharing one sign-in between plugins, or with evo's own connectors and
  model providers: each plugin signs in for itself.
- Driving a logged-in web page for the plugin (scraping in the agent's
  browser): a plugin gets the session's cookies for its own requests only.

## 3. User scenarios

### US-1 Set up a plugin that needs an account (priority P1)

A person installs a plugin that syncs with a web service. It needs a
workspace name and an API token.

Acceptance scenarios:

1. **Given** the plugin was just installed, **when** the person opens it,
   **then** it shows "Needs setup", lists the required settings, and does
   not run until they are filled in.
2. **Given** the token is entered, **when** it is saved, **then** it is
   kept in the keychain, evo's files hold no copy, and the field shows it
   as set (not its value).
3. **Given** the plugin offers "Sign in" instead of a token, **when** the
   person signs in, **then** the token is kept in the keychain and renewed
   by evo.

### US-2 Know and limit what a plugin can do (priority P1)

A person wants to see what a plugin's code may do and change it.

Acceptance scenarios:

1. **Given** a plugin with a backend that asks for network access to one
   site, **when** it is installed, **then** the consent dialog lists
   "Run its own code", "Connect to <site>" with the plugin's reason, and
   the other permissions, together with the views' permissions.
2. **Given** a TypeScript backend calls a site it was not allowed, **then**
   the call fails and the plugin's log says which site was refused.
3. **Given** a Python backend or hook declares network access, **then** the
   permission reads "Connect to <site> (not enforced)".
4. **Given** the person takes back "Run its own code", **then** the backend
   stops at once and does not start again.

### US-3 Control when a plugin runs (priority P2)

Acceptance scenarios:

1. **Given** a plugin with a backend set to start on demand, **when** evo
   opens, **then** it is not running; **when** its view opens or the agent
   calls one of its tools, **then** it starts, and after the idle time
   with no use it stops.
2. **Given** the person chooses "Start with evo" and the plugin has (or is
   granted) that permission, **then** it starts when evo opens.
3. **Given** the backend crashes, **then** it is restarted; after repeated
   crashes it stays stopped and the plugin page says so, with its log.
4. The plugin page shows whether it runs, with Start, Stop and Restart.

### US-4 Different settings per project (priority P2)

Acceptance scenarios:

1. **Given** a setting the plugin marks as per workspace, **when** the
   person sets a value for workspace A, **then** chats in A use it and
   chats elsewhere use the personal value.

### US-5 Plugins installed before (priority P1)

Acceptance scenarios:

1. **Given** a plugin with hooks or connector servers installed before this
   feature, **when** evo starts the first time with it, **then** the plugin
   shows "Review permissions"; its hooks and servers do not run until the
   person approves; its views keep their earlier approval.
2. **Given** a `sensitive` value saved before, **then** it moves to the
   keychain once and is removed from evo's file.

### US-6 Sign in with OAuth (priority P1)

A plugin works with a service that supports OAuth (for example a calendar,
an issue tracker or a chat service).

Acceptance scenarios:

1. **Given** the plugin needs that account, **when** the person presses
   "Sign in", **then** the service's page opens in their usual browser,
   they approve, and evo shows the account name and the access granted.
2. **Given** the access token expires, **when** the plugin next needs it,
   **then** evo renews it without asking; the plugin never sees the renewal
   token.
3. **Given** the service has no browser redirect (or the person is on
   another device), **when** the plugin uses a device code, **then** evo
   shows the code and the address to visit, and finishes when they confirm.
4. **Given** the plugin's update asks for more access (scopes), **then** the
   account shows "Sign in again to grant: <access>" and the old access keeps
   working until then.

### US-7 Log in to a web page (priority P2)

A plugin works with a site that has no OAuth — an internal portal behind
company single sign-on with a second factor.

Acceptance scenarios:

1. **Given** the plugin declares a web login, **when** the person presses
   "Log in", **then** evo opens a login window that shows the site's real
   address, the person logs in there (password, single sign-on, second
   factor, captcha), and the window closes by itself when the site shows
   it is logged in.
2. **Then** only the cookies of the domains the plugin declared are kept,
   and the plugin's code can use them; the login window kept nothing of the
   person's other sessions and shares nothing with them.
3. **Given** the site logs the session out, **when** the plugin notices
   (its requests fail as signed out), **then** the account shows "Log in
   again" and a notification says so.

### US-8 The agent needs an account that is not signed in (priority P1)

Acceptance scenarios:

1. **Given** a chat's agent calls a plugin tool that needs an account and
   none is signed in, **then** the chat shows "Sign in to <service> for
   <plugin>" with Sign in and Cancel, like an approval; after signing in
   the call goes on; Cancel ends it with an error that says so.
2. **Given** no window shows that chat, **then** the person gets a
   notification and the call waits (like other questions in a chat).

### US-9 Several accounts (priority P2)

Acceptance scenarios:

1. **Given** a personal and a work account of one service, **when** the
   person signs in to both, **then** one is the default and a workspace may
   use the other (per-workspace setting, FR-3).
2. **When** the person signs out of one, **then** its tokens or cookies are
   deleted, the service is told to revoke them when it offers that, and the
   other account keeps working.

### Edge cases

1. The keychain is not available (a Linux desktop without a secret
   service): secrets cannot be saved; the setting says why; nothing falls
   back to a plain file.
2. A secret is deleted from the keychain outside evo: the plugin shows
   "Needs setup" again.
3. The plugin is uninstalled: its secrets are removed from the keychain, its
   workspace values and data with it.
4. Deno or uv is not on the machine yet: evo gets it the first time a
   plugin needs it, with progress; offline, the plugin page says it waits
   for the runtime.
5. A backend's runtime permission is taken back while the agent waits on
   one of its tools: the call ends with an error that names the reason.
6. An update asks for new permissions: as in Plugin UI FR-3, the new
   version waits for consent; the running one keeps its permissions.
7. The person closes the browser tab or the login window, or takes more than
   5 minutes: sign-in ends as cancelled, nothing is kept, and the person can
   try again.
8. Two places need the same account at once (two chats, a view): one
   sign-in runs; the others wait for it.
9. Renewal fails because the service revoked access or the person changed
   their password: the account shows "Sign in again"; the plugin gets an
   error saying the account needs sign-in, never a stale token.
10. Renewal fails because evo is offline: the session is kept and renewal is
    tried again when the network is back; calls in between fail as offline.
11. The service needs a client secret (some do for every app): the plugin
    asks the person for their own app's client ID and secret as settings,
    and sign-in uses them.
12. The login window is sent to another site (a single sign-on provider):
    that is allowed and shown in its address bar; only declared domains'
    cookies are kept.
13. A cookie session or token is larger than the keychain allows for one
    entry: it is still protected by the keychain (the person sees no
    difference).

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | Each installed plugin has a page with five tabs: Overview, Settings, Permissions, Runtime, Data. | The tabs show for a plugin with settings, a backend and views. |
| FR-2 | A plugin describes its settings in its manifest: types text, multiline text, number, boolean, select (from a list), folder, file and secret; with a title, description, default, required flag, group and checks (minimum, maximum, pattern). Older `userConfig` entries are read as settings. evo draws the form and refuses values that fail the checks. | A manifest with each type draws each field; a value outside its range is refused with the reason. |
| FR-3 | A setting may be personal (the default) or per workspace. A workspace value overrides the personal one for chats in that workspace; the form shows both and which applies. | US-4. |
| FR-4 | A plugin with a required setting not filled in is "Needs setup": its backend, hooks and connector servers do not run, and its views show the setup message. | US-1 scenario 1. |
| FR-5 | Secret settings, tokens and session cookies are protected by the operating system's keychain (Windows Credential Manager, macOS Keychain, Linux Secret Service): kept in it, or encrypted with a key kept in it when they are too large for one entry; never in plain form in evo's files or logs. The form shows whether a secret is set, and lets the person replace or clear it, never read it back. | US-1 scenario 2; evo's plugin files contain no secret value. |
| FR-6 | A plugin's secrets reach only its own backend, hooks and connector servers, as environment variables, and only with the `secrets` permission. They are never put into command arguments, URLs or a view. | A backend reads its token from its environment; the process list shows no secret; a view cannot get it. |
| FR-7 | A plugin may offer "Sign in" for a service. The ways to sign in, the session's life and who may use it are FR-20 – FR-31. | US-1 scenario 3. |
| FR-8 | A plugin's running code needs permissions the person approves, in the same consent as its views' (Plugin UI FR-3): `runtime` (run its backend), `hooks` (run commands on agent events), `secrets`, `startup` (start with evo) and `network:<https origin>` for its processes, each with the plugin's reason. Without the permission, that part does not run. | US-2 scenario 1; taking back `hooks` stops its hooks at once. |
| FR-9 | A permission may be optional: the plugin works without it and asks for it when needed; evo asks the person once and remembers the answer. | A plugin asking for `startup` later gets one dialog; "Not now" keeps it working on demand. |
| FR-10 | A plugin may ship a backend written in TypeScript or JavaScript (run by Deno) or Python (run by uv). The backend is the plugin's connector server: its tools are offered to the agent like other connector tools. | A sample backend's tool is listed for the agent and answers a call. |
| FR-11 | A backend, and a connector server the plugin declares the old way, starts on demand by default: when one of its views opens, the agent calls one of its tools, or its view calls it (FR-14); it stops after an idle time (10 minutes by default, changeable per plugin). With `startup`, the person may choose "Start with evo". "Off" never starts it. | US-3 scenarios 1, 2. |
| FR-12 | A backend that exits unexpectedly is restarted; after 5 crashes in a minute it stays stopped until started by hand, and the plugin page shows the error. | US-3 scenario 3. |
| FR-13 | The Runtime tab shows the state (stopped, starting, running, crashed), Start, Stop, Restart, the start mode, the idle time, and the last lines of its log; the log also goes to Settings → Logs as `plugin:<name>`. | US-3 scenario 4. |
| FR-14 | A plugin's views may call their own plugin's backend tools through the bridge (`backend.call(tool, args)`), with no extra permission. | A view of the sample calls its backend and shows the answer. |
| FR-15 | Network access of a TypeScript/JavaScript backend is limited to the origins the person allowed; other connections fail and are logged. For Python backends, hooks and other connector servers the origins are declared and consented to, and each such permission is marked "not enforced". | US-2 scenarios 2, 3. |
| FR-16 | A TypeScript/JavaScript backend reads and writes only its plugin folder and data folder unless the plugin asks for `files:read:<folder>` or `files:write:<folder>` and the person approves; folders may be given as "the workspace". | A backend reading the home folder without permission fails. |
| FR-17 | Plugins installed before this feature are reviewed once (US-5): hooks and connector servers wait for approval of `hooks` and `runtime`; `sensitive` values move to the keychain. | US-5. |
| FR-18 | Deno and uv are obtained when a plugin first needs them, checked against published checksums, kept in evo's runtimes folder, and shared by all plugins. | A first backend on a clean machine gets Deno with progress; a second one does not download again. |
| FR-19 | The Data tab shows the size of the plugin's storage and data folder, opens the folder, and clears the data (after a confirmation). Uninstalling removes data, workspace values and keychain entries. | Clear data empties it; uninstall leaves no keychain entry. |

| FR-20 | A plugin declares the accounts it needs in its manifest: for each, the service's name, the sign-in kind (OAuth, device code, API key or token, web login), what it is needed for, and whether the plugin works without it. The plugin page lists them with their state: not signed in, signed in as <name>, needs sign-in again, offline. | A plugin with two accounts shows both and their states. |
| FR-21 | OAuth sign-in opens the service's page in the person's default browser (not inside evo), uses PKCE and a state check, and returns to evo on a one-time local address. Endpoints are declared, or discovered from the service's issuer. A plugin may use its own client ID, or ask the person for one (and a client secret) as settings. | US-6 scenario 1; edge case 11; a forged or replayed return is refused. |
| FR-22 | Device-code sign-in shows the code and the address in evo, opens the address on request, and finishes when the service confirms; the person may cancel. | US-6 scenario 3. |
| FR-23 | API-key and token accounts are secret settings with an optional check the plugin's backend runs ("Test"), showing the result. | A wrong key shows the service's error on Test. |
| FR-24 | Web-login accounts open a login window: its own storage (no cookies of the app or of other plugins), the site's address shown and read-only, no access to evo, navigation allowed (single sign-on). It closes when a declared address or cookie appears. Only cookies of the declared domains are kept. | US-7 scenarios 1, 2; edge case 12. |
| FR-25 | evo renews OAuth tokens before they expire, one renewal at a time per account; renewal tokens never leave evo. A failed renewal marks the account "needs sign-in again" (revoked) or keeps it and retries (offline). | US-6 scenario 2; edge cases 9, 10. |
| FR-26 | The plugin's backend and hooks get the current access token or cookie header of an account when they ask evo, with the `secrets` permission; a connector server declared the old way gets it in its environment and is restarted after a renewal. A backend may report an account as signed out (its requests were refused), which marks it "needs sign-in again" and notifies the person. | US-7 scenario 3; a backend asking after a renewal gets the new token. |
| FR-27 | Views see accounts' states and may start a sign-in (`auth.status`, `auth.signIn`), never a token or cookie. | A view's sign-in button opens the flow; no bridge call returns a secret. |
| FR-28 | When a plugin tool the agent calls needs an account that is not signed in, the chat asks "Sign in to <service> for <plugin>?" (Sign in · Cancel) as it asks approvals; the call waits and goes on after sign-in. With no window showing the chat, a notification is sent. | US-8. |
| FR-29 | A plugin may hold several accounts of one service; one is the default, and a workspace may choose another (FR-3). | US-9 scenario 1. |
| FR-30 | Signing out deletes the account's tokens or cookies, asks the service to revoke them when it declares how, and leaves other accounts as they are. Uninstalling the plugin signs out of all its accounts. | US-9 scenario 2. |
| FR-31 | An update that asks for more OAuth access marks the account "sign in again to grant <access>"; the account keeps its old access until then. | US-6 scenario 4. |
| FR-32 | evo ships a skill, `evo-creator`, that lets the agent make a plugin with the person: skills, commands, subagents, hooks, connector servers, views, a backend, settings and accounts, as this spec and [Plugin UI](../plugin-ui/spec.md) define them, with views styled after evo's design language (DESIGN.md, carried in the skill) through the theme's `--evo-*` variables. Its references stay in step with what evo reads. | Asked to make a plugin, the agent loads the skill; the plugin it writes installs from its folder with no problem; a test fails when the skill's manifest example or its copy of DESIGN.md drifts. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | Security: secrets, tokens and cookies never appear in evo's files, logs, the process list or a view; a backend cannot widen its own permissions or read another plugin's accounts; sign-in follows the native-app practice for OAuth (system browser, PKCE, loopback return); enforcement claims match reality ("not enforced" wherever evo does not block). | Review; a test plugin tries each leak, each widening and another plugin's account. |
| NFR-2 | A backend on demand answers its first tool call within 1.5 s of being started on the developer machine for TypeScript/JavaScript, and within 3 s for Python (runtime and dependencies already present); an idle backend uses no CPU. | Measured. |
| NFR-3 | Works on Windows and macOS; Linux needs a secret service for secrets (edge case 1). | Both platforms. |
| NFR-4 | Plugins without settings, backends or new permissions behave exactly as before. | Existing plugins with only skills and commands need no review. |

## 5. Success criteria

- SC-1 No plugin secret is stored in plain text anywhere evo writes.
- SC-2 A person can tell, from one page, what each plugin may do, whether it
  runs, and change both.
- SC-3 A TypeScript backend cannot reach a site the person did not allow.
- SC-4 With five plugins installed and none in use, no plugin process runs.
- SC-5 A plugin signed in to a service keeps working for weeks without the
  person signing in again, as long as the service allows it.

## 6. Clarifications

### 6.1 Open

n/a — all answered (6.2).

### 6.2 Resolved

- 2026-11-14 — Q: How is network access of plugin processes controlled? →
  A: declared and consented to for all; enforced for TypeScript/JavaScript
  by running them in Deno with allowed origins; marked "not enforced"
  elsewhere; operating-system sandboxes later (FR-8, FR-15, non-goals).
- 2026-11-14 — Q: Which runtime for TypeScript/JavaScript? → A: Deno
  (instead of Bun, for its enforceable permissions); uv for Python (FR-10).
- 2026-11-14 — Q: Where are secrets kept? → A: the operating system's
  keychain; existing plain values move there once (FR-5, FR-17).
- 2026-11-14 — Q: Default start mode? → A: on demand, stopped when idle;
  "Start with evo" needs `startup` (FR-11).
- 2026-11-14 — Q: Settings scope? → A: personal and per workspace (FR-3).
- 2026-11-14 — Q: Plugins installed before? → A: reviewed once; their hooks
  and servers wait for approval (FR-17).
- 2026-11-14 — Q: Deno and uv bundled or obtained when needed? → A:
  obtained the first time a plugin needs them, like the Office runtime
  (FR-18).
- 2026-11-14 — Q: Idle time before a backend stops? → A: 10 minutes by
  default, changeable per plugin (FR-11).
- 2026-11-14 — Q: May secrets be per workspace? → A: yes, each workspace
  value is its own keychain entry (FR-3, FR-5).
- 2026-11-14 — Q: Do connector servers declared the old way also start on
  demand? → A: yes, the same lifecycle as backends (FR-11 – FR-13).

## 7. History

- 2026-11-14 — Draft, from the research on plugin settings and the six
  decisions above.
- 2026-11-14 — Accepted (Gate A) with the proposed answers to the last four
  questions.
- 2026-11-14 — NFR-2 split by language after spike S4: Deno answered in
  about 0.25 s, a Python backend with the `mcp` package in about 2.1 s.
- 2026-11-14 — Sign-in and sessions added at the person's request (US-6 –
  US-9, edge cases 7 – 13, FR-20 – FR-31); FR-5 and FR-7 widened to tokens
  and cookies.
- 2026-11-14 — FR-32 added at the person's request: a bundled skill,
  `evo-creator`, for making plugins, carrying evo's DESIGN.md so plugin
  views match the app.
