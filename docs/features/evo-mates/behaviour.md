# Evo Mates — Behaviour reference

| | |
| --- | --- |
| Spec | [spec.md](spec.md) (FR-39 – FR-58) |
| UX | [ux.md](ux.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-28 |

How mates behave: with connections, plugins and skills, with the person in
a direct conversation, and with each other in a channel. The owner asked
for the behaviour of the AI-teammate apps on the market; this page records
what those apps do, as patterns, and what Evo Mates does for each, built on
what Evo already has. "Today" names the Evo feature a behaviour reuses.

## 1. Connections, plugins and skills

| Pattern seen | Evo Mates | Today |
| --- | --- | --- |
| Apps are connected once for the whole app; each bot is then narrowed to some apps | Connectors and plugin accounts are set up once in Evo. Each mate has an **Apps** setting: **All connected apps**, **Only these** (a list), or **None**. A new mate from scratch gets All; a mate from a template or team file gets only the apps it names, each off until the person turns it on. | Connectors are app-wide with a switch per chat; plugins have accounts (several per service) |
| A bot that needs an app it cannot reach shows a **Connect** card in the chat; an expired sign-in shows **Reconnect** | The mate shows a card naming the app and why it needs it, with **Connect** (or **Reconnect**). The sign-in runs in Evo's usual flow (browser OAuth, device code, web login). The mate never signs in by itself and waits on the card; the conversation shows **Needs you**. | Connector OAuth and plugin sign-in exist; the card is new |
| Several accounts per app, each with a label | A mate's **Apps** list picks the account when a service has several ("work", "personal"). | Plugin accounts support several; connectors one server each |
| Per-tool rules: allow once, for this conversation, or always | Approval cards offer **Allow once**, **Allow for this conversation** and, for tools that only read, **Always allow for this mate**. Always-allow rules show in the mate panel and can be removed. Tools that send, post, pay, delete or control the computer never get Always allow. | Approval modes, "Allow for this chat", connector tool Allow / Ask / Block |
| A per-bot "Sending on your behalf" setting: ask every time, or send without asking | **Sending on your behalf**: **Ask every time** (default) or **Send without asking**, per mate. It applies to tools that send, post or publish to other people, whatever the approval mode. | New on top of approval modes |
| A bot asks for a password or key through a secure card; the value never shows in the chat and is used by reference | A mate may ask for a secret with a **secure card** (a masked field, the service and the reason). The value goes to the OS keychain, never to the transcript or the model; the mate gets a reference it can pass to a tool that needs it. The person sees and deletes saved secrets in the mate panel (spec FR-43). | Plugin secrets in the OS keychain (`evo-vault`) |
| Skills: switches per bot; imported or flagged skills start off | A mate's **Skills** tab has a switch per skill Evo knows. Skills from a team file arrive off; a skill flagged by Evo's skill check arrives off with the reason. | Skills, plugins and the Customize page |
| Plugins: built-in and added integration cards with per-bot access | A plugin's tools, views and accounts are available to a mate only when its **Apps** setting includes the plugin. | Plugin permissions per plugin |
| Pause a bot: it stops, refuses new messages, keeps everything | **Pause** in a mate's menu stops its work, holds new messages ("Writer is paused") and resumes where it was on **Resume**. **Pause all mates** in the tray menu and Evo Mates' menu. | New; Stop exists |

## 2. A direct conversation

| Pattern seen | Evo Mates | Today |
| --- | --- | --- |
| A new bot greets and asks at most a few setup questions | The greeting says what the mate does in one or two lines and asks at most **four** setup questions in one question card; skipped questions keep the template's defaults. | `ask_user` question cards |
| Bots ask before guessing | Before work that would take long or act outside its folder, a mate asks when the request is ambiguous, in one question card. | `ask_user` |
| Activity chips while it works; result with proof | While working: one plain-word chip per step group. At the end: one reply with the result first, then what it did, and the files, pages or pictures that prove it. | Turn steps, `send_file` |
| The person writes while the bot works | A message sent while the mate works is **steered** into the running turn (the mate reads it at its next step) and shows as "Seen while working"; **Send as next** queues it instead. | The engine's steering and follow-up queues |
| Stop ends the current work; a new message starts again | **Stop** ends the turn; nothing is lost; the next message starts a new turn. | Stop |
| One main conversation, recall of other places | Each turn, the mate sees a short brief of what it did elsewhere in the last two days (its channels, up to ten lines), so a direct conversation and its channels stay one story. It can search its own past conversations and channels, never another mate's direct conversation. | Chat recall search, memory topics |
| Memory saved on its own, visible | The mate saves durable facts the person states and the lessons of its own work to its memory, with the "Saved to memory" chip. It does not save what other mates claim as facts. | Memory tool, memory topics |
| Notifications only when they matter | A notification when the mate **needs you** (approval, question, sign-in, secret), when it **finished** with a result while its conversation is not in front, or when it **couldn't reply**. None for bot-started work, none while the conversation is in front. Per mate: **Notifications** on or off. | Shell notices, notifications |
| Proactive messages only through set paths | A mate speaks unprompted only while finishing a task it was given, or when a mate in a channel hands it work. Routines and triggers are a later feature. | — |

## 3. Mates together in a channel

| Pattern seen | Evo Mates | Today |
| --- | --- | --- |
| Reply rules: Auto (a small decision model), lead, everyone, only when mentioned | Four rules: **Auto** (default for new channels), **Lead answers**, **Everyone answers**, **Mentioned only**. `@name` and `@everyone` always win over the rule. | — |
| The decision model sees the room's name, recent lines, the message and each member's name, title and description; it never does the work | Auto asks the router model with: channel name and instructions, the last lines (as many as fit 2,000 tokens), the new message, and each member's name, role and description, plus a "several members" choice. It answers one mate when that choice reaches **0.6**, every member when "several" reaches 0.6, else the lead. No answer within **1.5 s**, an error or no router: the lead. | — |
| The person sees who was picked | A quiet line above the reply: "Writer is answering · 94%" ("Picked by rule: lead" when the rule chose). | — |
| Bots hand work to each other: ask, assign, inform | A mate hands off with one of three kinds, shown as a hand-off line: **asks** (needs an answer back), **assigns** (do the work and report back), **tells** (information only). The receiving mate gets the text with a label: from which mate, which kind, and "not the person's instruction". | Session mail: `send_message`, `ask`, `reply` |
| Only a person's line is an instruction | A mate acts on another mate's assignment only within the person's request it came from, under its own approval mode and Apps. If an assignment needs the person (a decision, an approval, a send), the mate asks the person, never the other mate. | Approval modes per engine |
| A lead coordinates | The lead may split the person's request into assignments to members and posts a **wrap-up** reply when they report back. Members never assign work to the lead. | Nested sessions |
| Many bots at once; lines queue per bot | Several mates may work at once, up to the running limit. Each mate takes channel messages in order; a message for a busy mate waits for it ("Next: your message") unless it comes from the person and addresses only that mate, which steers it. Each reply is tied to the message it answers. | Running limits, queues |
| Context: room lines and the bot's recent work, never other bots' private chats | A mate in a channel gets the channel's instructions, the lines since its last turn (authors named), and its recent-work brief. It cannot read other mates' direct conversations. | Mesh deliveries |
| Loop limits | At most **12** hand-offs per message of the person (setting) and at most **10** hand-offs between the same two mates per hour; reaching either stops the chain with a notice. | Mesh per-pair mail limit |
| Approvals never rise through hand-offs | Each mate works under its own approval mode, Apps and sending setting; an assignment from a mate with looser settings does not loosen them. | ADR 0010 |
| Bots speak in a room only when picked, mentioned or handed work | No unprompted interjections in a channel in this release. | — |
| Notifications for a room | One notification when the channel needs the person; one "finished" when every reply to the person's message is in, while the channel is not in front. | Shell notices |

## 4. Teams moving between machines

| Pattern seen | Evo Mates |
| --- | --- |
| Imports never carry chats, memory, keys, models, approval levels, folders or connections | Team files carry profiles, instructions, avatar definitions, model needs, Apps by service name, skills by name, the channel and its rule. Imported mates start with the app's approval mode, Apps off, skills off, Sending on your behalf = Ask every time. |
| Imports never raise approvals; routines arrive paused | An import never sets an approval mode looser than the app's, whatever the file asks. Routines: n/a in this release. |
| Templates are recipes, not clones | A gallery role creates a new mate from the role's text; later edits to a template never change existing mates. |

## 5. Not taken in this release

Uninvited contributions in channels, triggers from app events, teaching by
recording, a mate's own cloud computer, outside chat apps, phone and voice
— see spec 2.2. Check-ins and follow-ups (FR-63 – FR-71) and the round from
the 2026-11 market research — Today, starters, teams, message search,
Knowledge, Describe a mate, check-ins from the panel and a daily spending
limit (FR-73 – FR-80) — are taken; what that research left out is listed in
spec A-24 and [research-2026-11.md](research-2026-11.md).
