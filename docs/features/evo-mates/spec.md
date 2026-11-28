# Evo Mates: AI teammates in a second app on the same core — Spec

| | |
| --- | --- |
| Status | In progress |
| Area | desktop shell, desktop UI, engine, protocol, `evo-store`, packaging |
| Created | 2026-11-25 |
| Updated | 2026-11-25 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| UX | [ux.md](ux.md): layout, messages, states, interactions, avatars |
| Behaviour | [behaviour.md](behaviour.md): connections, plugins, skills, direct conversations, channels, team files |
| Related | [ADR 0028](../../architecture/decisions/0028-two-front-ends-in-one-app-process.md), [Shell-hosted engines](../shell-hosted-engines/spec.md), [Nested sessions: roles, limits and mail](../session-roles-and-mail/spec.md), [Session mesh protocol](../session-mesh-protocol/spec.md), [Memory topics and chat recall](../memory-topics-and-chat-recall/spec.md), [Memory and the Scheduler](../memory-and-scheduler/spec.md), [Approval modes](../approval-modes/spec.md), [Workspace rail and "Needs you"](../workspace-rail/spec.md), [Quick entry](../quick-entry/spec.md), [Usage and cost](../usage-and-cost/spec.md), [Customize](../customize-skills-plugins-connectors/spec.md) |

## 1. Summary

Evo Harness is built for one person working with agents on a project: chats,
files, terminals, source control. Many people want something different from
agents: a small team of named helpers they talk to like colleagues, each with
its own role, personality and memory, who stay around for weeks instead of
one chat. AI-teammate products now on the market show this as a chat app
with a roster of bots, direct messages and group channels.

**Evo Mates** is that second app. It is installed together with Evo Harness
and runs on the same Evo: the same engine, providers, models, connectors,
skills, approvals and memory. Each **mate** has a profile (name, avatar,
role, instructions, model, tools) and one long-lived conversation. Mates
can be put together in **channels**, where Evo decides which mate answers
and mates can ask each other for help in front of the person. Teams can be
started from role templates and moved between machines as one file. A
person switches between Evo Harness and Evo Mates with one click. Evo
Mates looks and behaves like the AI-teammate apps people know: a messaging
app with a roster of faces, bubbles, status words, mates that set
themselves up in chat, and inline cards to approve and answer
([ux.md](ux.md)).

## 2. Scope

### 2.1 Goals

- One installation gives two apps, **Evo Harness** and **Evo Mates**, each
  with its own launcher entry, name and icon, running on one Evo.
- One click switches from either app to the other; both can be open at once.
- Mates as lasting identities: a profile, one conversation that never needs
  restarting, their own memory, their own default tools and approval mode.
- Direct conversations with a mate, and channels with several mates, a lead
  and a rule for who answers.
- Mates that ask other mates for help, visibly, with guards against loops.
- Role templates to start from, and a team file to export and import mates
  and channels.
- Mates keep working while their window is closed, as long as Evo runs.
- The look and feel of a messaging app with AI teammates: faces with moods,
  bubbles, plain status words, setup in chat, mentions, visible hand-offs
  ([ux.md](ux.md)).

### 2.2 Non-goals

- Mates shared by a team of people, or mates that live on a server. Mates
  belong to one person on one device. Team sharing is a later feature, for
  example through chat apps or the Evo Workspace server.
- Running with Evo quit, as a system service, or on a cloud machine. Mates
  work while Evo runs (window, hidden or tray), as
  [shell-hosted engines](../shell-hosted-engines/spec.md) define.
- Triggers from webhooks, files or connected apps' events (new mail, a
  calendar event soon, a mention elsewhere), morning digests and mates
  working a channel's goal among themselves. A later feature. Mates do
  speak first on their own follow-ups and on check-ins the person
  schedules (US-10, FR-63 – FR-71); other scheduled prompts stay with the
  existing Scheduler.
- A mate's own computer: a virtual machine or a desktop of its own, and
  teaching a mate by recording a demonstration. A later feature (phase 3).
  Mates use the browser, app control and Webbridge tools the person allows,
  as chats do today.
- Mates in outside chat apps (Slack, Teams, Telegram), a phone app, voice
  calls and spoken replies. A later feature (phase 4).
- Handing a chat from Evo Harness to a mate, or mentioning a mate in a
  Harness chat.
- Running other vendors' command-line agents as mates. Every mate runs on
  Evo's own engine.

## 3. User scenarios

### US-1 Install once, get both apps (priority P1)

A person installing Evo wants both apps from one installer, so that they
never set up providers, keys or connectors twice.

Acceptance scenarios:

1. **Given** a fresh install, **then** the system's app list (Start menu,
   Applications folder, desktop entries) shows **Evo Harness** and
   **Evo Mates**, each with its own icon.
2. **Given** Evo Harness is running, **when** the person opens Evo Mates
   from the app list, **then** the Mates window opens in the running Evo,
   with the providers, models and connectors already set up there; no second
   copy of Evo starts.
3. **Given** neither app runs, **when** the person opens Evo Mates, **then**
   Evo starts and shows only the Mates window.

### US-2 Switch between the apps (priority P1)

A person working in one app wants to reach the other at once, so that the
two feel like one product.

Acceptance scenarios:

1. **Given** Evo Harness is in front, **when** the person clicks
   **Switch to Evo Mates**, **then** the Mates window comes forward (opening
   if it was closed) and the Harness window stays as it was.
2. **Given** Evo Mates is in front, **when** the person clicks
   **Switch to Evo Harness**, **then** the Harness window comes forward.
3. **Given** both windows are open, **then** each shows as its own app in
   the taskbar where the platform allows it (NFR-4).

### US-3 Meet the first mate (priority P1)

A person opening Evo Mates for the first time wants a useful mate in a
minute, so that they see what mates are for before configuring anything.

Acceptance scenarios:

1. **Given** no mate exists, **when** Evo Mates opens, **then** it shows a
   gallery of role templates with a one-line description each, and
   **Start from scratch**.
2. **Given** the gallery, **when** the person picks a role, **then** a mate
   is created with that role's name, avatar, instructions and suggested
   tools, opened on its conversation with a greeting that says what it can
   do and asks its setup questions; the answers go into its profile.
3. **Given** no provider is connected, **when** the person picks a role,
   **then** Evo asks them to connect one first and returns to the mate
   afterwards.

### US-4 Talk to a mate over weeks (priority P1)

A person wants one place to talk to each mate, which remembers what was
said, so that they never re-explain and never manage "new chats".

Acceptance scenarios:

1. **Given** a mate, **when** the person opens it, **then** they see one
   conversation with all earlier messages, newest at the bottom.
2. **Given** a long conversation, **when** it grows past the model's
   context, **then** the mate keeps answering, older parts are summarised
   for the model, and the person can still scroll and search every message.
3. **Given** the person told the mate a fact last week, **when** they ask
   about it after Evo restarted, **then** the mate answers from memory.
4. **Given** a mate is working, **when** the person closes the Mates window,
   **then** the work goes on, and its reply is there when they come back.

### US-5 Shape a mate (priority P1)

A person wants to decide who a mate is and what it may do, so that each mate
fits one job.

Acceptance scenarios:

1. **Given** a mate's profile, **when** the person changes its name, avatar,
   role, instructions, model, thinking level, tools, connectors, skills or
   approval mode, **then** the change applies from the mate's next turn.
2. **Given** the roster, **when** the person right-clicks a mate, **then**
   they can pin, mark unread, edit the profile, duplicate, hide or delete
   it.
3. **Given** the person deletes a mate, **then** Evo asks first, says its
   conversation and memory go too, and removes it from every channel.

### US-6 A channel of mates (priority P1)

A person with a job that needs several roles wants them in one room, so that
they hand over work to each other without the person copying text.

Acceptance scenarios:

1. **Given** two or more mates, **when** the person creates a channel,
   **then** they name it, pick its members and lead, write shared
   instructions, choose a reply rule and, if they want, a project folder.
2. **Given** a channel with the reply rule **Mentioned only**, **when** the
   person writes `@Writer draft the intro`, **then** only the Writer
   answers.
3. **Given** the rule **Auto**, **when** the person writes without a
   mention, **then** Evo picks the mate whose role fits the message best,
   shows who was picked, and falls back to the lead when unsure.
4. **Given** the rule **Lead answers**, **when** the person writes without a
   mention, **then** the lead answers and may bring in other members.

### US-7 Mates helping each other (priority P2)

A person wants a mate that needs another role to ask that mate itself,
where they can see it, so that work moves on without them but never in
secret.

Acceptance scenarios:

1. **Given** a channel, **when** a mate needs another member's help,
   **then** it writes to that member in the channel; the person sees the
   request, the answer and who is working.
2. **Given** mates keep handing a message back and forth, **when** the
   hand-off limit for one message of the person is reached, **then** Evo
   stops the chain and tells the person.
3. **Given** mates are working in a channel, **when** the person presses
   **Stop**, **then** every mate's work for that channel stops.

### US-8 Answer a mate that needs you (priority P1)

A person wants to know when a mate is waiting for an approval or an answer,
in whichever app they are in, so that no mate sits blocked unseen.

Acceptance scenarios:

1. **Given** a mate asks to approve an action or asks a question, **then**
   it shows as a card in its conversation or channel, the mate's roster row
   shows it needs the person, and a notification is sent when the Mates
   window is not in front.
2. **Given** the person is in Evo Harness, **then** the waiting mate is
   listed in **Needs you**; **when** they click it, **then** Evo Mates opens
   on that card.

### US-9 Share a team (priority P2)

A person who built a useful team wants to move it to another machine or give
it to a colleague, so that they do not rebuild it by hand.

Acceptance scenarios:

1. **Given** a channel with its mates, **when** the person exports it,
   **then** one file holds the mates' profiles and the channel's settings,
   and never secrets, credentials, memories or conversations.
2. **Given** a team file, **when** the person imports it, **then** Evo shows
   what it adds (mates, channels, instructions, tools and connectors asked
   for) before anything is created.
3. **Given** an imported team asks for a connector that is not set up,
   **then** the mates are created with that connector off and say so.

### US-10 A mate that speaks first (priority P2)

A person wants a mate to act like a colleague who remembers: to come back
on what they talked about (an interview tomorrow, a draft it promised, a
question left open) and to check in when asked to, without being nagged.

Acceptance scenarios:

1. **Given** the person told a mate "I have an interview tomorrow at 10",
   **when** the next day after 10 comes and the person has not mentioned
   it, **then** the mate writes first in its conversation to ask how it
   went, once.
2. **Given** a mate with nothing open, **then** it never writes first; a
   mate whose first message got no answer does not write again until the
   person does.
3. **Given** the person schedules "every weekday at 9:00, tell me what is
   left on the launch" for a mate, **then** at each time the mate writes
   that, in its conversation.
4. **Given** Do not disturb is on, **then** no mate writes first until it
   is off; what is still worth saying goes out then.
5. **Given** a mate's **Following up** list in its panel, **when** the
   person removes an item or marks it "never", **then** the mate does not
   come back on it.

### Edge cases

- A mate's provider is disconnected or its model is gone from models.dev:
  the mate says so in its conversation and offers to pick another model; it
  is never switched silently.
- Evo quits while a mate works: on the next start the turn shows as
  interrupted, with **Retry**.
- A mate is deleted while it works in a channel: its work stops first.
- A channel loses its lead: Evo asks the person to pick a new one; until
  then the rule acts as **Mentioned only**.
- More mates are asked to work than may run at once (NFR-6): the rest wait,
  shown as **Queued**.
- The same approval shows in Evo Mates and in Evo Harness's Needs you:
  answering it in one removes it from both.
- A team file is damaged, from a newer Evo, or asks for an unknown tool:
  the import says what it cannot read and creates nothing until the person
  agrees to import the rest.
- Text from another mate, a web page, a connector or a team file asks a
  mate to act: it is data, never the person's instruction (NFR-5).
- The person opens the Mates launcher while only the Harness window is
  hidden in the tray: the Mates window opens; the Harness window stays
  hidden.

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | One installer must install both apps, each with its own entry, name and icon in the system's app list, on Windows, macOS and Linux. | Given a fresh install on each platform, then both entries exist and each opens its own app. |
| FR-2 | Opening either app while Evo runs must use the running Evo: one app process, one tray icon, shared settings, providers, credentials, connectors, skills and usage. | Given Harness running, when Mates is opened, then no second Evo process starts and a provider added in Harness is usable by a mate without restart. |
| FR-3 | Each app must have a visible switch to the other that opens or brings forward the other app's window, and both windows may be open at once. | Given each app, when the switch is clicked, then the other window is in front and the first is unchanged. |
| FR-4 | Closing a window must not stop work: closing Mates leaves mates working, closing Harness leaves its chats working, as long as Evo runs in the tray or another window is open. | Given a mate working, when the Mates window closes, then the turn completes and its reply is in the conversation on reopening. |
| FR-5 | The tray menu must open either app. | Given the tray menu, then it has **Open Evo Harness** and **Open Evo Mates**, and each brings its window forward. |
| FR-6 | Evo Mates must show a roster of mates and channels with avatar, name, role, last message and state: idle, working, queued, needs you or unread. | Given mates in each state, then each row shows the right state and updates without reopening. |
| FR-7 | The person must be able to create a mate from a role template or from scratch. A new mate starts with the app's approval mode, or with the mode its role card suggests when the person picks that role; every field (name, avatar, role, instructions, model, thinking level, Apps, skills, approval mode, sending setting) can be changed later in the mate panel. | Given the gallery, when a role is picked, then a mate exists with that role's settings and opens on its conversation; given **Start from scratch**, then the conversation opens with no form first. |
| FR-8 | Evo must ship a gallery of role templates, each with face, name, one-line description, instructions, suggested apps and, where the role needs it, a suggested approval mode shown on its card. | Given a fresh install, then the gallery lists the templates, each creates a working mate, and a role suggesting a looser mode shows it on the card. |
| FR-9 | Each mate must have one conversation that lasts across restarts, is never split into separate chats, is summarised for the model when it grows too long, and stays fully scrollable and searchable for the person. | Given a conversation past the model's context window, then the mate still answers and a search finds the first message. |
| FR-10 | A mate must answer as itself: its name and avatar on its messages, its instructions and model on every turn; a profile change applies from the next turn. | Given a changed model, when the person writes, then the next reply uses the new model, shown in its details. |
| FR-11 | Each mate must keep its own private memory across its conversation and channels, and also read the person's general memories, never Evo Harness's project memories. | Given a fact told to mate A in a direct message, when asked in a channel A is in, then A answers from memory and mate B does not know it; given a Harness project memory, then no mate can read it. |
| FR-12 | A mate's approvals and questions must show as cards where it works, mark its roster row, send a notification when the Mates window is not in front, and appear in Harness's **Needs you**, which opens them in Evo Mates. | Given a mate waiting with the Mates window closed, then a notification arrives and Harness's Needs you lists it; answering in one place clears both. |
| FR-13 | The person must be able to pin, mark unread, edit, duplicate, hide and delete a mate; delete asks first and removes the mate from every channel. | Given each action from the row's menu, then the roster and channels reflect it. |
| FR-14 | The person must be able to create a channel with a name, members, a lead, shared instructions, a reply rule and an optional project folder, and change them later. | Given a new channel, then its settings are kept and apply to the next message. |
| FR-15 | A channel must support four reply rules: **Auto** (default), **Lead answers**, **Everyone answers** and **Mentioned only**. Auto works as FR-53; with **Mentioned only** and no mention, nobody answers and the channel suggests a mention. | Given each rule and an unmentioned message, then the mates the rule names answer; with Mentioned only, none answers and the hint shows. |
| FR-16 | Mentioning one or more members, or `@everyone`, must make each of them answer, whatever the reply rule. | Given `@A @B`, then A and B both answer; given `@everyone`, then every member answers. |
| FR-17 | A mate must be able to ask another member of the same channel for help; the request and its answer must be shown in the channel. | Given a mate that needs another role, then the hand-off and the answer appear in the channel with both names. |
| FR-18 | A chain of hand-offs must stop, with a notice, at the per-message limit the person can set (default 12) and at 10 hand-offs between the same two mates in one hour. A report back to the member that asked or assigned the work counts toward the pair's hour only, not the message's limit. | Given two mates set to hand back and forth, then the chain stops at the limit with a notice; an eleventh hand-off between them within the hour is refused. A lead that assigns three mates and gets three reports stays under the default limit. |
| FR-19 | **Stop** must stop one mate's work, or every mate's work in a channel. | Given three mates working in a channel, when Stop is pressed, then all three stop within 2 s. |
| FR-20 | The person must be able to export a mate or a channel with its mates to one file, never containing secrets, credentials, memories or conversations. | Given an export, then the file has profiles and channel settings, and a search of it finds no key, token or message text. |
| FR-21 | Importing a team file must preview what it adds and create nothing until the person confirms. Imported mates start with the app's approval mode (never looser, whatever the file asks), Apps off, skills off and Sending on your behalf = Ask every time. | Given a team file asking for the loosest mode, then the preview shows the app's mode; Cancel creates nothing; after import every app and skill is off. |
| FR-22 | The Usage page must show tokens and cost per mate. | Given a mate's replies, then Usage lists the mate with its cost. |
| FR-23 | A project folder given to a mate or channel must go through the same project trust check as in Evo Harness. | Given an untrusted folder, then its project hooks and servers ask before they run, as in Harness. |
| FR-24 | Mates' conversations and channels must not appear in Evo Harness's sidebar or chat search; Harness shows mates only in **Needs you** and Usage. | Given mates with conversations, then Harness's sidebar and search list none of them; a waiting mate is listed in Needs you. |
| FR-25 | Each mate must have its own working folder that Evo creates and the person can open from the profile; a mate or a channel may be given a project folder instead, and a channel's folder is used by its members while they work in that channel. | Given a new mate, then its folder exists and opens from the profile; given a channel with a project folder, then a member's file tools work in that folder in the channel and in its own folder in its direct conversation. |
| FR-26 | A mate's profile must show its memories, and let the person edit, forget one, or clear all of them. | Given a mate with memories, when the person forgets one, then the mate no longer recalls it in its next turn. |
| FR-27 | The team file must be one versioned Markdown file: front matter listing the channel and its mates (name, role, avatar, model need such as "fast" or "capable" instead of a model ID, tools, connectors, skills, approval mode) and a body with each mate's instructions under its own heading. A file from a newer format version is refused with a message. | Given an exported file, then it parses against the format and holds no model ID; given a file with a higher version, then import says it needs a newer Evo and creates nothing. |
| FR-28 | The model that picks a mate under **Auto** must be one the person chooses in settings, by default the cheapest connected model that models.dev marks as fit for tool use; its cost must show in Usage under Evo Mates. When the provider refuses that model (the account cannot use it), the next one routes: the choice, then by price. A seat never runs on the channel's routing model; it runs on its mate's model, else the saved default. | Given no choice made, then the default is used and its picks show as Evo Mates in Usage; given no connected fit model, then Auto acts as Lead answers. Given a cheapest model the account cannot use, then the next model routes; a new seat runs on the default model. |
| FR-29 | Evo Mates must use the messaging layout of [ux.md](ux.md) section 2: a sidebar with search, **New**, pinned mate tiles, **Mates** and **Channels** lists and a footer with Settings and the switch; a conversation with a floating header pill and composer; a mate panel with **About**, **Tools & apps**, **Memory** and **Files**; folding rules for narrow windows. | Design review against ux.md 2 at 1440×900 and 900×700, light and dark. |
| FR-30 | Messages must show as bubbles: the person's on the right, a mate's on the left tinted with the mate's colour, runs joined with the avatar once at the run's end and a sender line (name, role, time) at its start. | Given a run of three mate messages, then one avatar and one sender line show, and corners join. |
| FR-31 | A mate's tool steps must show as activity chips in plain words ("Reading 3 files"), one per step group, opening their details in place; raw tool names never show in Evo Mates. | Given a turn that reads files and searches the web, then two chips in plain words show and each opens its details. |
| FR-32 | A mate's state must show the same everywhere (roster row, header pill, avatar) with the words of ux.md section 4: Thinking, Working, Queued, Needs you, Couldn't reply, and unread. | Given a mate in each state, then row, header and avatar agree. |
| FR-33 | Every mate must have a face drawn by the open-bot-avatar engine from Evo's own avatar definitions, on a round plate in the mate's colour, with the engine's state shown for the mate's state (ux.md 6); faces hold still at rest, move only while the mate works or under the pointer, and never move with reduced motion on. | Given idle, working and needs-you mates, then only the working one moves; with reduced motion, none moves; the app's notices list the engine's licence. |
| FR-34 | A mate created from the gallery or from scratch must greet the person and ask its setup questions in its conversation (what to call it, what to focus on, which apps it needs), and save the answers to its profile. | Given a new mate, when the person answers its questions, then the profile shows the answers. |
| FR-35 | When a mate saves something to memory, a "Saved to memory" chip must show under the message it came from and open that entry in the Memory tab. | Given the person tells a mate a fact it saves, then the chip shows and opens the entry. |
| FR-36 | In a channel, `@` must open a list of members with faces and roles; hand-offs must show as a line with both faces and a one-line summary that expands; under **Auto** a quiet line must name who answers and why. | Given a channel, when the person types `@`, then the members list shows; a hand-off and an Auto pick each show their line. |
| FR-37 | The gallery must show roles by category tabs with search; each card shows face, role name, one line and **Add**. | Given the gallery, when the person searches "sales", then only fitting roles show. |
| FR-38 | Keyboard: Ctrl+K search, Ctrl+N new mate, Ctrl+1 – 9 pinned mates, Esc closes the mate panel (⌘ on macOS). | Manual check of each shortcut. |
| FR-39 | Each mate must have an **Apps** setting: **All connected apps**, **Only these** (a list of connectors and plugins, with the account where a service has several) or **None**. A mate made from scratch gets All; a mate from a template or team file gets only the apps it names, each off. A mate reaches only the apps its setting allows. | Given a mate on Only these with one app, then tools of other apps are absent from its turns. |
| FR-40 | When a mate needs an app that is not connected, or whose sign-in expired, it must show a card naming the app and the reason, with **Connect** or **Reconnect**, wait on it and show **Needs you**; it never signs in by itself. | Given a mate asked to read mail with no mail connector, then the Connect card shows and the mate waits; after connecting, it goes on. |
| FR-41 | Approval cards in Evo Mates must offer **Allow once**, **Allow for this conversation** and, for tools that only read, **Always allow for this mate**. Always-allow rules are listed in the mate panel and removable. Tools that send, post, pay, delete or control the computer never offer Always allow. | Given a read-only connector tool, then Always allow shows and its rule appears in the panel; given a send tool, then it does not show. |
| FR-42 | Each mate must have **Sending on your behalf**: **Ask every time** (default) or **Send without asking**, applying to tools that send, post or publish to other people whatever the approval mode. | Given a mate on Ask every time under the loosest approval mode, when it sends an email, then it asks first. |
| FR-43 | A mate must be able to ask for a secret through a secure card (masked field, service, reason); the value is kept in the OS keychain, never in the transcript or sent to the model, and the mate uses it by reference. The mate panel lists saved secrets by name and deletes them. | Given a secret entered on the card, then a search of the transcript and the provider requests finds no value, and the tool that needs it works. |
| FR-44 | A mate's **Skills** tab must show a switch per skill; skills from a team file and skills flagged by Evo's skill check arrive off, the flagged ones with the reason. | Given an imported team with two skills, then both are off; a flagged one shows why. |
| FR-45 | **Pause** must stop a mate's work and hold new messages with a "paused" line, and **Resume** must continue where it was; **Pause all mates** does this for every mate, from the tray menu and Evo Mates' menu. | Given a paused mate, when the person writes, then the paused line shows and nothing runs until Resume. |
| FR-46 | A new mate's setup questions must fit in one question card of at most four questions; skipped questions keep the template's defaults. | Given a new mate, then one card with at most four questions shows; skipping keeps the defaults. |
| FR-47 | Before work that would take long or act outside its folder, a mate must ask one question card when the request is ambiguous, instead of guessing. | Given ambiguous long tasks from a test set, then the mate asks before acting in at least 8 of 10 runs. |
| FR-48 | A mate's final reply to a task must put the result first, then what it did, then the files, pages or pictures that show it. | Review of 10 task replies against the shape. |
| FR-49 | A message sent while the mate works must be steered into the running turn and shown as "Seen while working"; **Send as next** queues it instead. | Given a running mate, when the person sends a correction, then the next step reflects it; with Send as next, it runs after the turn. |
| FR-50 | Each turn must give the mate a brief of what it did in its other places in the last two days (up to ten lines); a mate may search its own past conversations and channels, never another mate's direct conversation. | Given work done in a channel, when asked in the direct conversation, then the mate knows of it; a search for another mate's direct message finds nothing. |
| FR-51 | A mate must save to its memory facts the person states and lessons from its own work, and must not save other mates' claims as facts. | Given a channel where another mate states a fact, then it is not in this mate's memory; a fact the person states is saved, with the chip. |
| FR-52 | Mates must notify only when they need the person (approval, question, sign-in, secret), when they finished with a result while their conversation or channel is not in front, or when they could not reply; never for work a mate started for another mate. Each mate has a **Notifications** switch. | Given each case with the window in front and not, then notifications follow the rule; with the switch off, none. |
| FR-53 | Under **Auto**, the router must get the channel's name and instructions, the recent lines (up to 2,000 tokens), the new message and each member's name, role and description, plus a "several members" choice; it picks one mate at a confidence of 0.6 or more, every member when "several" reaches 0.6, else the lead; no answer within 6 s, an error or no router means the lead. A line above the reply names the pick and its confidence. | `cargo test -p evo-engine channel` with a faux router for each branch; manual check of the line. |
| FR-54 | A hand-off must be one of **asks**, **assigns** or **tells**, shown as a line; the receiving mate gets it labelled with its sender, its kind and that it is not the person's instruction. A mate acts on an assignment only within the person's request it came from, under its own approval mode, Apps and sending setting; when the work needs the person, it asks the person. In a mate's reply an @mention asks that member only inside a question (a sentence ending with "?"); a mention anywhere else (thanks, a greeting, a reference) only names the member and wakes nobody. Work is assigned by mail starting with "[assign]"; the channel shows what the mate wrote, never the mail's envelope. | Engine test of the labels; manual: an assignment that needs a send asks the person, not the assigning mate. Engine test that only a mention in a question asks; hub test that a hand-off copy has no envelope. |
| FR-55 | A channel's lead may split a request into assignments to members and must post a wrap-up reply when they report back; members never assign work to the lead. | Given a lead-answered request that needs two members, then two assignment lines and one wrap-up show. |
| FR-56 | Each mate must take a channel's messages in order: a message for a busy mate waits ("Next: …") unless it comes from the person and addresses only that mate, which steers it; each reply is tied to the message it answers. | Given two messages for a busy mate, then the second waits; each reply shows which message it answers. |
| FR-57 | In a channel a mate must get only the channel's instructions, the lines since its last turn with their authors, and its recent-work brief; it never reads other mates' direct conversations and never speaks unless picked, mentioned or handed work. | Engine test of a seat's delivery contents; manual: no unprompted reply over a day of use. |
| FR-58 | A mate's conversation must read like a phone messaging app: the person's messages right-aligned in accent-coloured bubbles with light text, the mate's left in neutral bubbles; consecutive bubbles from one side grouped with a tail only on the last; a centred time line when more than 15 minutes pass between messages ("Today 3:02 PM"); "Delivered" under the person's last message until the mate answers; a reply's model, time and cost behind its info button, not under every bubble. | Visual review against ux.md 3 in light and dark; a gap of 16 minutes shows a time line. |
| FR-59 | While a mate works with no text yet, a typing bubble with three moving dots shows on its side, with its current step in one plain line under it ("Reading 3 files…"); reduced motion holds the dots still. | Given a running turn before its first text, then the typing bubble and step line show; they go when text streams. |
| FR-60 | The person can react to any bubble with one of six reactions (heart, thumbs up, thumbs down, laugh, emphasis, question) from a hover bar or right-click; a reaction shows as a badge on the bubble's corner, a second pick changes it, the same pick removes it. Reactions are kept across restarts, and the mate is told of new reactions to its messages at the start of its next turn, as information. | Given a reaction, then the badge shows after a restart; the next turn's input carries the note "The person reacted … to your message: …". |
| FR-61 | The person can reply to a specific bubble: Reply shows the quoted text above the message box (Esc removes it); the sent bubble shows the quote above it, and the mate receives the quoted text with the message. | Given a reply to a mate's bubble, then the quote shows above the sent bubble and the mate's answer refers to it. |
| FR-62 | The message box is a rounded pill: an attach button on the left, a field that grows to six lines, and a round send button in the accent colour that appears when there is text (a stop button while the mate works); Enter sends, Shift+Enter starts a new line. | Manual check of each control and key. |
| FR-63 | Each mate must keep an **agenda** of open threads taken from its conversations: what it promised to do, events the person mentioned with their time, and questions left open, each with when to come back to it. The agenda is the mate's own, kept with its profile on this device, and never holds secrets. | Given a conversation that mentions an event tomorrow and a promised draft, then both items are on the mate's agenda with sensible times. |
| FR-64 | A **heartbeat** must wake each mate that has a due agenda item or a check-in at most once an hour while Evo runs; the mate decides whether something is worth saying and either writes one message or stays silent (nothing shown). A mate with nothing due is not woken. | Given a due item, then within the hour the mate writes or records "nothing to say"; given none, then no model call is made for it. |
| FR-65 | A mate may write first at most its **initiative** allows per day (default **Medium**: 5 messages; **Low**: 2; **Off**: none), only after the person has talked with it, and stops after two first messages in a row get no answer, until the person writes again. Initiative is set per mate in its panel. | Given Medium and two unanswered first messages, then a third due item produces no message until the person writes. |
| FR-66 | A first message must read as the mate following up, put the point first, and say why it is writing ("You had the interview this morning: how did it go?"); it appears in the mate's conversation like a reply, marks it unread, and is announced under the mate's **Notifications** switch. | Review of 10 first messages from the conduct set; manual check of unread and the notice. |
| FR-67 | While Evo's **Do not disturb** is on, no mate writes first; due items wait and, once it is off, each mate sends at most one message for what is still worth saying, dropping items whose moment passed. | Given Do not disturb on over a due item, then nothing is sent until it is off, then one message at most per mate. |
| FR-68 | Between conversations a mate may do **read-only** work for an agenda item (read its folder, search the web, read connected apps it is allowed, draft text in its own folder) and bring the result in its message; anything that changes the person's files, sends, posts or spends must ask the person, whatever the mate's approval mode. | Given a mate on "Approve for me" with a due item that needs an email sent, then it asks before sending. |
| FR-69 | The person must be able to schedule **check-ins** per mate ("every weekday at 9:00", "every Monday", "once on 3 December at 15:00") with what the mate should look at; a check-in can be paused, changed or removed from the mate panel and from the card the mate shows when it sets one up. | Given a weekday check-in, then the mate writes at each time; pausing stops it. |
| FR-70 | A mate's panel must show its **Following up** list: each agenda item and check-in with its time; the person can remove an item, mark it "never follow up on this", or change a check-in. A removed or "never" item is not followed up. | Manual check of each action; a "never" item does not come back after a new mention of it. |
| FR-71 | Proactive work must be visible and bounded: each heartbeat decision, first message and read-only run is recorded in the mate's usage (source `mates`) and in the app log; **Pause** (FR-45) stops a mate's initiative too. | Usage shows proactive cost per mate; a paused mate never writes first. |
| FR-72 | In a channel a mate must ask the person in its reply, as plain text (numbered when there are several questions, each with what it would assume), never with a question card, which the channel cannot show; the person answers in the channel. A message of the person with no mention that follows such a question goes to the mates who asked, whatever the reply rule. | Given a channel with **Everyone** and a mate that asked a question, when the person replies without a mention, then only that mate gets it and the route line reads "{name}, who asked you"; a mate's seat has no `ask_user` tool. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | Switching to the other app while Evo runs shows its window within 500 ms; cold-starting Evo Mates shows the roster within the time the Harness window takes today. | Measured on the reference Windows machine, 10 runs, median. |
| NFR-2 | A conversation of 10,000 messages opens with its newest messages visible in under 1 s. | Measured with a generated conversation. |
| NFR-3 | Mates add no new network listener and no cloud service: everything stays on the device except the model providers, connectors and pages mates reach. | Review of open ports before and after; no new outbound host. |
| NFR-4 | Windows, macOS and Linux are supported. On Windows each window shows as its own app in the taskbar. Where the platform shows one app per process (macOS Dock), both windows show under Evo and the Mates entry in Applications opens the Mates window. | Manual check on each platform. |
| NFR-5 | Mates follow every existing trust rule: approval modes enforced by the engine, the sandbox, project trust, connector permissions. Messages from other mates, imported team files, tool output and pages are data, never the person's instruction. | A mate asked by another mate to run a command still needs the person's approval under "ask". |
| NFR-6 | At most a settable number of mates run turns at once (default in 6.3); others queue in order. | Given more working mates than the limit, then the extra ones show Queued and start as others finish. |
| NFR-7 | Evo Mates follows DESIGN.md (fonts, tokens, menus, dialogs, approval words, motion) plus the Mates layer of ux.md: bubbles and one colour per mate on its face, bubble tint and mention chip only. UI text says "Evo Mates" and "mate", never "session", "engine" or "agent type"; code says `mates`. No other product's names, logos, mascots or artwork are used. | Design review against DESIGN.md and ux.md; text search of Mates UI strings for the banned words. |
| NFR-8 | An older Evo database upgrades in place; Evo Harness works unchanged for a person who never opens Evo Mates. | Upgrade test from the last released schema; Harness checks pass. |

## 5. Success criteria

- SC-1 From a fresh install with a provider connected, a person creates a
  first mate from a template and reads its first reply in under 2 minutes.
- SC-2 Switching between Evo Harness and Evo Mates takes one click and under
  1 second.
- SC-3 A mate recalls a fact told a week earlier, after Evo restarted, with
  no new chat started by the person.
- SC-4 In a channel of three mates on **Auto**, the mate the person meant
  answers in at least 8 of 10 messages of a fixed test set.
- SC-5 A team exported on one machine imports on another with the same
  mates and channel, and with no secret in the file.
- SC-6 A person who never opens Evo Mates sees no change in Evo Harness
  other than the switch and the tray entry.
- SC-7 Over a scripted week of conversations, a mate on Medium follows up
  on at least 4 of 5 events the person mentioned, at a sensible time, and
  never writes first more than its initiative allows.

## 6. Clarifications

### 6.1 Open

None.

### 6.2 Resolved

- 2026-11-25 — Q: How should a mate's conversation look and behave? → A:
  the owner asked for a phone messaging app's chat: bubbles on two sides,
  reactions, replying to a message and a pill message box (FR-58 – FR-62).
  Taken with the agent's recommended details: six reactions, the mate is
  told of reactions as information at its next turn, a reply quotes the
  text to the mate.
- 2026-11-25 — Q: May Evo Mates break DESIGN.md's "achromatic, one accent"
  rule with tinted bubbles and a colour per mate? → A: yes, as a written
  Evo Mates exception in DESIGN.md, limited to the face and its plate, the
  bubble tint and the mention chip; everything else stays achromatic
  (NFR-7).
- 2026-11-25 — Q: Does a new mate ask its setup questions in chat instead
  of a form? → A: yes; the mate panel holds every field for later changes,
  and **Start from scratch** opens the conversation, not a form (FR-34,
  FR-46).
- 2026-11-25 — Q: Is the secure secret card in this release? → A: yes;
  values go to the OS keychain as plugin secrets do, and a mate only holds
  a reference (FR-43).
- 2026-11-25 — Q: Which approval mode does a new mate start with? → A: the
  app's approval mode; a gallery role may suggest a looser one, shown on
  its card, which the person accepts by picking the role; an import never
  goes looser than the app's (FR-7, FR-8, FR-21).
- 2026-11-25 — Q: Where do mates' faces come from? → A: the open-bot-avatar
  engine (MIT), with Evo's own avatar definitions and palette; its licence
  notice ships with the app. Other products' avatars and mascots are not
  used (FR-33, NFR-7).
- 2026-11-25 — Q: How do mates behave with apps, plugins, the person and
  each other? → A: like the AI-teammate apps the owner cited, as recorded
  in [behaviour.md](behaviour.md): Apps per mate, Connect cards, the
  sending setting, pause, steering, notification rules, four reply rules
  with a router, labelled hand-offs, lead wrap-ups (FR-15, FR-16, FR-18,
  FR-21, FR-39 – FR-57).

- 2026-11-25 — Q: Where do mates' conversations show in Evo Harness? → A:
  only where the person must act: Harness's **Needs you** lists mates
  waiting for an approval or answer and opens them in Evo Mates; Usage
  lists mates. The Harness sidebar and chat search do not list mates
  (FR-12, FR-22, FR-24).
- 2026-11-25 — Q: Where does a mate work on files? → A: each mate has its
  own folder that Evo creates and the person can open; a mate or a channel
  can be given a project folder instead, which goes through project trust.
  A channel's folder wins over its members' own folders while they work in
  that channel (FR-23, FR-25).
- 2026-11-25 — Q: What does a mate remember, and what does it see of the
  person's other memory? → A: each mate has private memory; every mate
  also reads the person's general memories but not Evo Harness's project
  memories. The person sees, edits and clears a mate's memory on its
  profile (FR-11, FR-26).
- 2026-11-25 — Q: What format is the team file? → A: one versioned
  Markdown file: front matter lists the channel and its mates (name, role,
  avatar, model need such as "fast" or "capable" instead of a model ID,
  tools, connectors, skills, approval mode); the body holds each mate's
  instructions under its own heading, close to the agent type format
  (FR-20, FR-21, FR-27).
- 2026-11-25 — Q: Which model picks the mate under **Auto**? → A: a fast
  model the person chooses in settings, by default the cheapest connected
  model that models.dev marks as fit for tool use; its cost shows in Usage
  under Evo Mates. With no fit model, Auto acts as **Lead answers**
  (FR-15, FR-28).

- 2026-11-25 — Q: Two separate apps, or one installation with two
  front ends on one Evo? → A: one installation, one app process, two apps
  (windows), each with its own launcher entry, and a one-click switch (FR-1
  – FR-5, NFR-4; [ADR 0028](../../architecture/decisions/0028-two-front-ends-in-one-app-process.md)).
- 2026-11-25 — Q: What is the second app called? → A: **Evo Mates**; each
  AI teammate is a **mate** (NFR-7).
- 2026-11-25 — Q: Are mates personal and local, or shared by a team? → A:
  personal and local in this feature; team sharing is a later feature
  (2.2).
- 2026-11-25 — Q: When do mates run? → A: while Evo runs (window, hidden
  or tray); no system service, no cloud machine (2.2, FR-4).
- 2026-11-25 — Q: What does the first release cover? → A: the two-app
  shell and switch, mates with profiles and one lasting conversation,
  channels with reply rules and visible hand-offs, role templates, team
  export and import (phases 0 and 1 of the research). Self-started work, a
  mate's own computer, outside chat apps, phone and voice are later
  features (2.2).

- 2026-11-26 — Q: What does the first version of mates that speak first
  cover? → A: follow-ups from an agenda of open threads, a heartbeat that
  lets each mate decide quietly, and check-ins the person schedules; no
  app-event triggers and no channel initiative yet (US-10, FR-63 – FR-71).
- 2026-11-26 — Q: How talkative is a mate by default? → A: **Medium**: up to
  5 first messages a day, backing off after two unanswered ones (FR-65).
- 2026-11-26 — Q: When do mates stay quiet? → A: only while Evo's Do not
  disturb is on; no fixed quiet hours (FR-67).
- 2026-11-26 — Q: May a mate work on its own between conversations? → A:
  read-only work, then ask before anything that changes, sends, posts or
  spends (FR-68).
- 2026-11-26 — Q: How often does the heartbeat run? → A: hourly, only for
  mates with something due (FR-64).
- 2026-11-26 — Q: How does a first message reach the person? → A: like a
  reply: in the conversation, unread, notified under the mate's
  Notifications switch (FR-66).
- 2026-11-26 — Q: Is the agenda visible? → A: yes, as **Following up** in
  the mate panel, editable (FR-70).
- 2026-11-26 — Q: Change the Auto router's budget? → A: 6 s instead of
  1.5 s: in the app a real router never answered within 1.5 s (FR-53).

### 6.3 Assumptions

- A-1 The tray icon is shared by both apps and its menu opens either —
  because one app process has one tray icon (FR-5).
- A-2 A new mate's approval mode is the app's default approval mode —
  because mates must not be looser than chats by default (NFR-5).
  Confirmed by the owner (6.2).
- A-3 At most 4 mates run turns at once by default, settable — because it
  matches the existing default limit of 4 running subagents.
- A-4 The hand-off limit per message of the person is 12 by default,
  settable (FR-18) — because a lead with three mates spent 6 on real work
  (an ask, three assignments, two follow-up questions) and the limit stopped
  every mate mid-job; 12 leaves room for a round of follow-up questions.
- A-5 Opening Evo Mates with no mate shows the gallery and creates nothing
  until the person picks — because an unasked mate would spend tokens on a
  greeting.
- A-6 Avatars come from Evo's built-in icon set, or a picture the person
  picks — because generated avatars need a model call and add no function.
- A-7 A mate's model defaults to the app's default model and is chosen
  from connected providers' models listed by models.dev, never a
  hardcoded list.
- A-8 Each launcher opens only its own app; there is no setting yet for
  which app opens at sign-in.
- A-9 The heartbeat and check-ins run only while Evo runs, and a check-in
  missed while Evo was closed runs once when Evo starts, if it is less
  than 12 hours late — because mates do not run as a service (2.2).
- A-10 Agenda items are taken by the mate itself at the end of a turn (a
  tool it calls), not by a separate model pass — because it already has
  the conversation and the cost stays inside the turn.
- A-11 A mate's reply asks the person something when one of its
  questions mentions no member (FR-72) — a plain rule the router does not
  need; a draft that quotes a question also counts, so the next message
  without a mention goes back to that mate, which is usually who the
  person is talking to.
- A-12 A mention in a mate's reply asks only from a question (FR-54) — a
  plain rule, like A-11: mates thanked, greeted and named each other with
  @ and every such line woke the named mate, so greetings looped until the
  hand-off limit stopped everyone. An imperative request without "?" is
  given by mail with "[assign]", as the delivery tells the mate.

## 7. History

- 2026-11-25 — Draft, from framing: research of AI-teammate products on
  the market and an open-source chat app for local agent teams, mapped
  against Evo's existing features; the owner's five decisions in 6.2.
- 2026-11-25 — Clarified: the five open questions answered as proposed (6.2); FR-24 – FR-28 added.
- 2026-11-25 — Plan and tasks written; waiting for gate B.
- 2026-11-25 — Owner asked for the look, faces and interactions of AI-teammate apps on the market: ux.md written, FR-29 – FR-38 added, NFR-7 changed, three questions opened (6.1).
- 2026-11-25 — Owner chose open-bot-avatar for faces and asked for the reference apps' behaviour: behaviour.md written, FR-39 – FR-57 added, FR-15, FR-16, FR-18, FR-21, FR-33 changed; 6.1 now holds four questions.
- 2026-11-25 — The four open questions answered as proposed (6.2); FR-7 and FR-8 changed. No clarification left; spec ready for gate A.
- 2026-11-25 — Gates A and B: the owner accepted the spec and the plan; status Accepted, then In progress with phase 1.
- 2026-11-25 — Owner asked for a phone messaging app's chat UI: FR-58 – FR-62 added (6.2).
- 2026-11-26 — Owner asked for mates that act more like people: research of
  AI companion and teammate products; US-10 and FR-63 – FR-71 added, 2.2
  changed, FR-53's router budget 1.5 s → 6 s; eight answers in 6.2.
- 2026-11-26 — Owner found the "needs you" card of a mate in a channel could not be answered and asked that mates ask in the chat and take the answer there: FR-72 added, A-11 recorded.
- 2026-11-27 — Owner found channels full of errors: mates woke each other for thanks and greetings until the hand-off limit stopped every mate, hand-off lines showed the raw mail, new seats ran on the routing model and failed. FR-18, FR-28 and FR-54 changed, A-12 recorded.
- 2026-11-27 — Owner raised the default hand-off limit per message from 6 to 12 after a lead with three mates hit 6 with real requests. FR-18 and A-4 changed.
- 2026-11-28 — FR-39 and FR-40 completed for connected services by [Connected apps](../connected-apps/spec.md) (FR-10, FR-11): a mate's Apps setting lists the services connected through Composio with per-service tools and accounts, enforced by the shell's gate; a mate's Connect / Reconnect card starts that sign-in and the mate goes on once the account is active.
