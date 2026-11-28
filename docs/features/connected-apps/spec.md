# Connected apps — Spec

| | |
| --- | --- |
| Status | In progress |
| Area | engine, `evo-mcp`, `evo-vault`, desktop shell, desktop UI, both apps |
| Created | 2026-11-28 |
| Updated | 2026-11-28 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [Customize: skills, connectors, plugins, hooks](../customize-skills-plugins-connectors/spec.md), [Evo Mates](../evo-mates/spec.md) (FR-39 – FR-42, FR-68), [Approval modes](../approval-modes/spec.md), [Learning layers](../learning-layers/spec.md) |

## 1. Summary

Today the agent reaches outside services only through connectors the
person sets up one by one (an MCP server's command or URL), and Evo Mates'
**Apps** setting (FR-39) has little to offer a mate that should read mail,
post to Slack or update a Notion page. **Connected apps** gives both apps a
catalogue of hundreds of services (Gmail, Google Calendar and Drive, GitHub,
Slack, Notion, Linear, HubSpot…) that the person connects with a click and
a sign-in in their browser, several accounts per service, each with a
label. The agent and each mate then get those services' tools, within the
person's grants and approvals. It runs on **Composio** with the person's own
Composio project key: Composio holds the sign-ins and runs the tools; Evo
never sees a service's password or token.

## 2. Scope

### 2.1 Goals

- A Composio project key, checked and kept in the system's keychain.
- A **Connected apps** page in both apps: search the catalogue, connect,
  add accounts with labels, reconnect, disconnect.
- The connected services' tools in chats and mates, through Evo's
  connector layer, with its permissions, approvals and tool search.
- Explicit account choice when a service has several accounts.
- Per-mate grants: which services, which accounts and which tools a mate
  may use (completes [Evo Mates FR-39](../evo-mates/spec.md#41-functional)).
- A mate's **Connect** card for a service it needs (Evo Mates FR-40) works
  with connected apps.
- Local files handed to a service's tool, only from allowed places and
  shown before sending.

### 2.2 Non-goals

- A managed Composio service run by Evo: there is none; the person brings
  their own key.
- App events (run a mate when new mail arrives): they need a server that
  receives Composio's webhooks; a later feature (Evo Mates lists triggers
  as a non-goal).
- Replacing MCP connectors: both stay; connected apps is one more source of
  tools.
- Signing in to a service for the person: the sign-in is always the
  person's, in their browser.

## 3. User scenarios

### US-1 Set up connected apps (priority P1)

A person wants their mates to use Gmail and Slack.

1. **Given** no key, **when** the person opens Connected apps, **then** the
   page explains what Composio is, that tool calls go through it, and how
   to get a project key, with a field to paste it.
2. **Given** a pasted key, **when** it is saved, **then** Evo checks it with
   Composio and shows the catalogue, or says exactly what is wrong (not a
   project key, refused, no network).

### US-2 Connect an account (priority P1)

1. **Given** the catalogue, **when** the person picks Gmail and **Connect**,
   **then** Evo asks for a label (prefilled "personal" for the first),
   opens the sign-in in the person's browser and shows the account as
   connected once Composio reports it active.
2. **Given** Gmail connected as "personal", **when** the person adds
   "work", **then** both show as separate rows and neither replaces the
   other.

### US-3 A mate uses a connected app (priority P1)

1. **Given** Writer allowed Gmail "work" only, **when** the person asks
   Writer to find last week's invoices, **then** Writer searches that
   account's mail and cannot read "personal".
2. **Given** Writer asked to send an email under "Ask every time", **then**
   the approval card shows the account, the recipient and the text before
   anything is sent.
3. **Given** Notion not connected, **when** the person asks a mate to
   update a Notion page, **then** the mate shows a Connect card for Notion
   and waits.

### US-4 In Evo Harness (priority P2)

1. **Given** GitHub connected, **when** the person asks a Harness chat to
   list their open issues, **then** the agent uses the GitHub tools, and the
   chat's `+` menu can switch connected apps off for that chat.

### Edge cases

- An account whose sign-in expired shows **Reconnect**; the agent is told
  the service needs the person, and a mate shows its Reconnect card.
- A service that needs more permissions than the connection gave (Google
  refuses with insufficient scope): the agent is told what to change,
  instead of retrying.
- A service with no sign-in that Composio provides: connecting it says it
  needs the person's own OAuth app in their Composio project, with the
  steps.
- The key is removed or replaced with another project's: every service
  shows as not connected; nothing is an error.
- No network or Composio down: Connected apps says so; chats keep their
  other tools.
- A tool call whose arguments do not fit the service tool's schema is
  refused before it leaves Evo, naming the first problem.

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | Settings has a **Composio project key** field (in Harness under Connected apps, in Mates in Settings › Evo Mates). A saved key is checked with Composio first and kept in the system's keychain, never in a plain file, a log or a prompt. **Remove key** forgets it. | Given a key that Composio refuses, then it is not saved and the reason shows. |
| FR-2 | Next to the key field, Evo says that service tool calls and their results go through Composio, and that the services' sign-ins are kept by Composio. | Review of the page text. |
| FR-3 | Connected apps and MCP servers are one place, **Connectors**: in Evo Harness the Customize section Connectors (also reached from a row in the sidebar's More menu); in Evo Mates a page opened from the sidebar and the Apps tab of each mate. It lists the connected services (each with its accounts) and the MCP servers side by side, a catalogue of services with search by name and category, and for each service every account as its own row: label, status (Active, Needs sign-in, Expired, Failed), connected date. | Given two Gmail accounts, then Gmail shows two account rows. |
| FR-4 | **Connect** asks for a label, unique within the service and required once the service has an active account; opens Composio's sign-in in the person's default browser; and shows the account once Composio reports it active. A pending sign-in can be reopened without creating another account; after ten minutes a new link is asked for. | Given Connect then closing the browser tab, when the person chooses Open again, then the same pending account is reused. |
| FR-5 | A service holds at most five usable accounts. A label held only by an account that is not active can be reused: that account is removed first. | Given an expired "work", when the person connects "work" again, then one "work" row remains. |
| FR-6 | **Disconnect** revokes that account at Composio (and with it the service's grant) and removes only that row; other accounts of the service stay. **Reconnect** signs in again under the same label. | Given two accounts, when one is disconnected, then the other still works. |
| FR-7 | Connected services' tools reach chats and mates through Evo's connector layer: one connector, **Connected apps**, whose tools obey Allow / Ask / Block, the approval modes and the tool search used past 40 tools (Customize FR-28). Customize shows it with its status and tools. | Given 300 connected tools, then a chat gets the tool search, not 300 tools. |
| FR-8 | When a service has several active accounts, a call must name the account; a call that does not, or names one the chat or mate may not use, is refused with the accounts it may use. With one account, it is used. | Given two Gmail accounts and a call with none named, then it is refused, listing both labels. |
| FR-9 | A call's arguments are checked against the tool's schema before it is sent; a refusal names the tool and the first problem and says it is about the call's shape, not access. | Given a missing required argument, then the call is refused with its name. |
| FR-10 | Each mate's **Apps** setting ([Evo Mates FR-39](../evo-mates/spec.md#41-functional)) lists connected services among its choices; under **Only these**, for each service: **All tools**, **These tools** (a list) or **None**, and, for a service with two or more accounts, **Accounts this mate may use**. A mate from scratch gets all connected apps; from a template or team file, only the services it names, each off. A service connected later reaches every mate on **All connected apps** and no mate on **Only these**. | Given a mate on Only these with Gmail "work", then its calls to "personal" are refused and other services' tools are absent. |
| FR-11 | A mate that needs a service that is not connected, expired, or not allowed shows its Connect / Reconnect card (Evo Mates FR-40) for that service; **Connect** runs FR-4 and, once active, the mate goes on. A service connected meanwhile is offered to it without restarting. | Given the card and a finished sign-in, then the mate continues its task. |
| FR-12 | In Evo Harness, connected apps follow the chat's connector switches: the `+` menu switches them off for one chat (Customize FR-27), and a project can switch them off for its chats. | Given connected apps off in a chat, then its tool list has none of their tools. |
| FR-13 | A tool that sends, posts, publishes, pays or deletes is classed as such and follows **Sending on your behalf** (Evo Mates FR-42) and never offers Always allow (Evo Mates FR-41); a tool that only reads may. Approval cards name the service, the account and the arguments. | Given a send tool under Send without asking off, then a card shows before the call. |
| FR-14 | A tool that takes a file accepts a full local path, uploaded to Composio only when the file is in the chat's or mate's folder or attached to this conversation, never a hidden file or folder, never Evo's own data; at most 10 files a call; the approval card lists each file (name, path, size) before anything is uploaded. Anything else is sent unchanged. | Given a path to `~/.ssh/id_rsa`, then it is not uploaded. |
| FR-15 | A service's refusal for missing permissions (for example Google's insufficient scope) reaches the agent with what to change (the person's own auth config in Composio) and that reconnecting alone will not help. | Given a scope refusal, then the agent's answer names the step. |
| FR-16 | Connecting a service Composio has no managed sign-in for uses the person's enabled auth config for it in their project when there is one, otherwise says how to create one. | Given no auth config for such a service, then Connect shows the steps. |
| FR-17 | Removing or changing the key, or Composio being unreachable, shows every service as not connected or unavailable with the reason, and chats keep working with their other tools. | Given Composio unreachable, when a chat starts, then it starts and Connected apps shows "unavailable". |
| FR-18 | The same Composio user is kept for the installation across restarts and updates, so connected accounts come back; a reinstall with the same key finds the earlier user when there is exactly one with a live account. | Given a restart, then the accounts are still connected. |
| FR-19 | Connected-app tool calls are listed in the transcript as "Called <service> · <tool> (<account>)" and their cost (none from Evo's side) does not count as model usage. | Manual check in the running app. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | The key and any Composio session secret are kept only in the system keychain; the database holds only non-secret ids. | Review: no secret in `evo.db`, logs or prompts. |
| NFR-2 | Service output is treated as untrusted data, as connector output today. | Review: results are wrapped as tool output, never as instructions. |
| NFR-3 | Opening the Connected apps page shows the catalogue within 2 s on a normal connection, and from a cache when offline. | Timed manual check. |
| NFR-4 | No request goes to Composio while no key is saved. | Given no key, then a session makes no request to Composio. |

## 5. Success criteria

- SC-1 A person connects Gmail and asks a mate about their mail in under
  two minutes from pasting the key.
- SC-2 No mate ever uses an account or a service outside its grant.
- SC-3 Nothing is sent to anyone without the card the person's settings
  call for.

## 6. Clarifications

### 6.1 Open

None.

### 6.2 Resolved

- 2026-11-28 — Q: Which backend for connected apps? → A: Composio with
  the person's own project key, through Evo's connector layer (section 1,
  FR-1, FR-7).
- 2026-11-28 — Q: Harness, Mates or both? → A: both apps (section 2.1).
- 2026-11-28 — Q: Where does it live? → A: Harness: Customize next to
  Connectors and the More menu; Mates: Settings and each mate's Apps tab
  (FR-3).
- 2026-11-28 — Q: Which mates get a newly connected service? → A: those
  on All connected apps, not those on Only these (FR-10).
- 2026-11-28 — Q: Connectors and Connected apps as two Customize sections?
  → A: one section, Connectors, holding both, with its inside redesigned
  (owner, while building; FR-3 changed).

### 6.3 Assumptions

- A-1 Five usable accounts per service — Composio allows 2 to 10 per
  session; five keeps the list short, as OpenMausBot does. A sign-in link
  lives ten minutes — Composio's lifetime for a link.
- A-2 The catalogue is cached for a day — because it changes rarely and the
  page must open offline.
- A-3 (2026-11-28, taken while building, owner not asked) Evo Mates has no
  Settings window of its own, so its "page in Settings" is the Connected
  apps page as a dialog from the Mates sidebar footer and from a mate's
  Apps tab; Settings › Evo Mates (in Evo Harness) holds the key card.
- A-4 (2026-11-28, taken while building) A project switches connected apps
  off for its chats with `"connectedApps": false` in its
  `.evo/settings.json`; there is no switch for it in the UI yet.
- A-5 (2026-11-28, taken while building) In Evo Harness chats, which have no
  Sending on your behalf setting, a tool that sends asks every time
  (Allow once / Deny); a tool that only reads runs without asking unless
  the person set it to Ask.
- A-6 (2026-11-28) Services show their logo, from Composio's logo host
  through the shell and its cache; a letter tile while it loads or when
  there is none. (First taken as letter tiles only; the owner asked for
  logos.)

## 7. History

- 2026-11-28 — Draft. Owner asked to bring connected apps into both apps,
  on Composio with their own key.
- 2026-11-28 — Owner accepted the spec with the proposed answers (Gate A).
- 2026-11-28 — Owner accepted the plan (Gate B). Status Accepted.
- 2026-11-28 — Implementation started. Status In progress.
- 2026-11-28 — Owner asked to merge Connectors and Connected apps into one section and redesign it; FR-3 changed, 6.2 recorded. Owner asked for service logos; A-6 changed.
