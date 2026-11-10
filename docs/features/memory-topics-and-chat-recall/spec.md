# Memory topics and chat recall — Spec

| | |
| --- | --- |
| Status | In progress |
| Area | engine, protocol, `evo-store`, desktop shell, desktop UI |
| Created | 2026-11-10 |
| Updated | 2026-11-10 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [Memory and the Scheduler](../memory-and-scheduler/spec.md), [Agent browser](../agent-browser/spec.md) (FR-20, the past-chat tools), [Approval modes](../approval-modes/spec.md) |

## 1. Summary

Memory today is a set of small notes the agent saves and finds by name. This
feature makes it work the way people know from chat assistants: a short list
of **topics** the agent keeps up to date while the chat goes on, controls to
pause, reset and scope it (per project and per chat), incognito chats that
leave no history, the ability for the agent to look up past chats when
asked, and automatic indexing: a model the person chooses reads idle chats,
keeps the topics up to date and writes a summary of each chat. It is needed because notes pile up as near-duplicates, the person
cannot keep one chat out of memory or out of history, and the agent's search
of past chats matches exact text only, newest first, cannot answer "what did
we work on yesterday?", and can reach into other projects' chats.

This feature extends [Memory](../memory-and-scheduler/spec.md) and the
past-chat tools `session_search` and `session_read` of
[Agent browser FR-20](../agent-browser/spec.md); where a requirement here
changes one there, it says so. Everything in those specs that is not named
here stays as it is.

## 2. Scope

### 2.1 Goals

- Memory as topics: one short document per subject, updated in place as the
  agent learns more, instead of a new note per fact.
- The agent recalls past chats of the same project when the person asks
  about them, and shows which chats it used.
- A switch to keep one chat out of memory, a switch to keep a project apart
  from personal memory, and pause and reset for all of memory.
- Incognito chats: a chat that uses no memory and leaves no history.
- Automatic indexing by a model the person chooses: topics updated from
  what was said in chats, and a summary per chat that makes past chats
  easier to find, with every change listed and undoable.
- One place for every memory setting: **Settings → Memory**.
- Sensitive subjects stay out of memory unless the person allows them.

### 2.2 Non-goals

- Indexing while evo is closed: chats waiting to be indexed are done after
  the next start.
- Review before a change applies: indexing changes memory at once; the
  person undoes what they do not want.
- Scoring, fading or automatic forgetting of topics: a topic stays until the
  agent or the person changes it.
- Importing memory from, or exporting it to, other products: topics are
  files the person can open from the Memory panel.
- Searching across projects: a chat never sees another project's chats.
- Searching past chats by meaning: search matches words and phrases only.
- Undoing what an incognito chat did: files it changed and commands it ran
  stay; only the chat's own record is removed.

## 3. User scenarios

### US-1 Memory builds up by topic (priority P1)

A person wants what they tell the agent about one subject to end up in one
place so that memory stays short and readable.

Acceptance scenarios:

1. **Given** a topic about commit style, **when** the person gives a new
   rule about commits, **then** the agent changes that topic instead of
   adding one, and the transcript shows "Updated memory · name" with the
   lines added and removed.
2. **Given** no topic covers a lasting fact, **when** the agent learns it,
   **then** it adds a topic with a category, and the transcript shows
   "Saved memory · name".
3. **Given** a topic holds a fact, **when** the person says to forget it,
   **then** the agent removes that fact from the topic (or deletes the topic
   if nothing is left) and says so.

### US-2 The agent recalls past chats (priority P1)

A person wants to ask about earlier work so that they do not have to find
and reread old chats.

Acceptance scenarios:

1. **Given** past chats of this project mention a release freeze, **when**
   the person asks what was decided about it, **then** the agent searches
   past chats, the transcript shows "Searched past chats" with the chats
   found, and the answer names the chats it used; selecting one opens it.
2. **Given** a chat in project A, **when** the agent searches, **then** no
   chat of project B is returned.
3. **Given** the person asks what was worked on yesterday, **when** the
   agent looks, **then** it lists that day's chats by time.
4. **Given** **Search past chats** is off, **when** the person asks about an
   earlier chat, **then** the agent says it cannot look at past chats.

### US-3 Keep one chat out of memory (priority P1)

A person wants to work on something without it being remembered.

Acceptance scenarios:

1. **Given** a new chat, **when** the person turns memory off for it before
   the first message, **then** the chat gets no memory, saves none, cannot
   search past chats, and shows that memory is off.
2. **Given** the first message was sent, **when** the person looks at the
   switch, **then** it is locked.
3. **Given** a chat with memory off, **when** another chat of the project
   searches past chats, **then** this chat can still be found.

### US-4 Keep a project apart (priority P2)

A person wants one project's chats to use only that project's memory.

Acceptance scenarios:

1. **Given** **Use personal memory** is off for a project, **when** a chat
   runs there, **then** it sees only the project's topics and saves only to
   the project.

### US-5 Pause and reset (priority P2)

A person wants to stop memory for a while, or start again from nothing.

Acceptance scenarios:

1. **Given** memory is paused, **when** a chat runs, **then** no topic is
   used or added, and the Memory panel still shows and edits the topics.
2. **Given** topics exist, **when** the person resets memory and confirms,
   **then** every personal and project topic is deleted and idle chats have
   an empty memory from their next message.

### US-6 Sensitive subjects (priority P2)

A person wants private subjects to stay out of memory unless they choose
otherwise.

Acceptance scenarios:

1. **Given** **Include sensitive subjects** is off, **when** the person
   mentions a health condition, **then** the agent does not save it; if they
   ask it to, it names the setting.
2. **Given** the setting is on, **when** the agent saves such a fact,
   **then** the transcript row marks it as sensitive.
3. **Given** any setting, **when** the person asks the agent to remember an
   identity number, an account number or a password, **then** the agent
   declines and says why.

### US-7 An incognito chat (priority P2)

A person wants a chat that is gone when they are done with it.

Acceptance scenarios:

1. **Given** a new chat, **when** the person makes it incognito before the
   first message, **then** it is marked as incognito, gets no memory, saves
   none and cannot search past chats.
2. **Given** an incognito chat, **when** the person ends it or evo quits,
   **then** its transcript is deleted and it is not listed among chats.
3. **Given** an incognito chat is open, **when** any chat or the person
   searches chats, **then** it is not found.

### US-8 Memory is indexed automatically (priority P2)

A person wants memory to keep itself up to date so that they never have to
say "remember this", and wants to choose which model does that work.

Acceptance scenarios:

1. **Given** automatic indexing is on with an index model chosen, **when** a
   chat with new messages has been idle for the set time, **then** the model
   reads the new part, updates the topics and writes the chat's summary.
2. **Given** indexing changed a topic, **when** the person opens the Memory
   panel, **then** the change is listed with its time, the topic, what
   changed and the chat it came from, and can be undone.
3. **Given** a chat's summary exists, **when** the agent searches past chats
   for something the summary mentions, **then** that chat is found and its
   summary is shown.
4. **Given** the index model cannot be used, **when** a chat goes idle,
   **then** nothing is indexed, chats work as before, and
   **Settings → Memory** says why.

### Edge cases

- Automatic indexing switched on for the first time: only chats used from
  then on are indexed; earlier chats are indexed only when the person asks
  for it, after seeing how many there are.
- A chat gets a message while it is being indexed: the chat is not delayed;
  the new part is indexed on the next run.
- The agent saved a fact in the chat and indexing finds the same fact:
  the topic is updated, not duplicated.
- Deleting a chat deletes its summary; topics learned from it stay.
- Reset deletes topics and the list of indexing changes; chat summaries stay
  and chats already indexed are not read again.
- An incognito chat that was open when evo crashed: what is left of it is
  deleted at the next start, before any chat is shown.
- A scheduled task never runs as an incognito chat.
- Memories saved before this feature: each is a topic as it is; nothing is
  converted.
- Two chats change one topic at the same time: a partial change applies to
  the topic's current text; if the text to change is no longer there, the
  change fails and the agent reads the topic again.
- Deleting a chat does not delete what was learned from it.
- A search that finds nothing says so; the agent does not make up an answer.
- The chat the agent is in is never one of its own search results.
- A nested chat follows the memory switch of its top-level chat.
- Text found in past chats may hold tool output or web pages: it is
  untrusted data, as in the chat it came from.

## 4. Requirements

### 4.1 Functional

Topics:

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | A memory is a topic: a short document about one subject with a name, a one-line description, a category and an update time. Existing memories are topics unchanged. | Given a memory saved before this feature, when the panel opens, then it is listed as a topic with its text unchanged. |
| FR-2 | A topic's category is one of the four memory types of [Memory FR-3](../memory-and-scheduler/spec.md#41-functional): `user`, `feedback`, `project`, `reference`. | Given the `memory` tool's schema, then the category offers exactly these four values. |
| FR-3 | The `memory` tool gains `update`: replace a given piece of a topic's text with new text, or add text at its end, without sending the whole topic. It fails when the text to replace is not found. `save` with an existing name still replaces the topic (extends [Memory FR-5](../memory-and-scheduler/spec.md#41-functional)). | Given a topic, when `update` replaces one line, then only that line changes; when the old text is absent, then the tool fails and the topic is unchanged. |
| FR-4 | The Memory section of the system prompt tells the agent to look for a topic that covers the subject and update it, to add a topic only for a new subject, to keep topics short, and to apply "remember", "change" and "forget" requests at once (extends [Memory FR-4](../memory-and-scheduler/spec.md#41-functional)). | Given five rules about one subject told in five chats, then one topic holds all five. |
| FR-5 | The transcript shows "Updated memory · name" with the lines added and removed (extends [Memory FR-6](../memory-and-scheduler/spec.md#41-functional)). | Manual check in the running app. |

Controls:

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-6 | A new chat has a memory switch, on by default, that can be changed only before the first message. Off: the chat has no Memory section, the `memory` tool and the past-chat tools are not offered, and the chat shows that memory is off. The choice is kept with the chat. | Given a chat started with memory off, when evo restarts and the chat continues, then it still has no memory and no memory tools. |
| FR-7 | Each project has a **Use personal memory** switch, on by default. Off: chats in that project get no personal topics and the `memory` tool refuses the personal scope there. | Given the switch off, when a chat starts in the project, then its prompt lists only the project's topics. |
| FR-8 | The existing on/off switch is shown as **Pause memory**: topics are kept, no chat uses or adds any, and the Memory panel still lists and edits them (restates [Memory FR-7](../memory-and-scheduler/spec.md#41-functional)). | Given memory paused, when the panel opens, then topics are listed and can be edited. |
| FR-9 | **Reset memory** deletes every personal topic and every project's topics after a confirm that states how many topics will be deleted and that it cannot be undone; idle chats reload. | Given topics in two projects and personal ones, when reset is confirmed, then none are left in any scope. |
| FR-10 | **Settings → Memory** holds every memory setting: **Pause memory**; **Automatic indexing** with **Index model**, **Index after idle** (minutes), **Index now** and **Index earlier chats**; **Search past chats**; **Include sensitive subjects**; **Reset memory**; and **Manage**, which opens the Memory panel. Each change applies without a restart. | Manual check in the running app: change each setting and see it apply to the next message of an idle chat. |

Past chats:

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-11 | `session_search` matches the words of the query, not only the exact text, and returns the best matches first, weighing how well they match and how recent they are, each with the chat's title, date and the passages around the matches (extends [Agent browser FR-20](../agent-browser/spec.md#41-functional), which returns exact matches newest first). | Given a chat that holds both words of a query apart from each other and a newer chat that holds one, when the agent searches, then both are returned and the first ranks higher. |
| FR-12 | `session_search` without a query lists past chats by time: newest or oldest first, before or after a date, up to a count. | Given chats on two days, when the agent lists chats after the first day, then only the second day's chats are returned. |
| FR-13 | `session_search` and `session_read` see only chats of the same project folder as the current chat; a chat with no folder sees only chats with no folder. Archived chats are included; the current chat is not. The option to search every project is removed, and `session_read` refuses a chat of another project (changes the tools as built under Agent browser FR-20, which says "own workspace"). | Given chats in two folders, when a chat in one searches or reads, then nothing of the other is returned. |
| FR-14 | The transcript shows "Searched past chats" with the query and the chats found; selecting a chat opens it. The agent's answer names the chats it used. | Manual check in the running app. |
| FR-15 | **Search past chats** is a setting, on by default. Off: `session_search` and `session_read` are not offered. | Given the setting off, when a chat starts, then its tool list has neither tool. |
| FR-16 | The tools only read; they need no approval. What they return is marked as untrusted data, as today. | Given approval mode "ask", when the agent searches, then no approval is shown. |

Incognito chats:

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-19 | A new chat can be made incognito, only before the first message. It is marked as incognito in the chat and in the sidebar for as long as it is open. It has a fixed title and cannot be renamed, pinned, forked or archived. | Manual check in the running app. |
| FR-20 | An incognito chat has no Memory section, and the `memory` tool and the past-chat tools are not offered (as a chat with memory off, FR-6). | Given an incognito chat, when it starts, then its prompt has no Memory section and its tool list has none of the three tools. |
| FR-21 | An incognito chat is never returned by the past-chat tools or by the app's own chat search, while it is open or after. | Given an open incognito chat that mentions a phrase, when another chat or the person searches for it, then the chat is not found. |
| FR-22 | An incognito chat ends when the person chooses **End chat** or evo quits. Its transcript, and those of its nested chats, are then deleted and it is no longer listed. What is left of one after a crash is deleted at the next start, before any chat is shown. | Given an incognito chat, when it is closed, then no transcript of it is left on the device; given evo was killed while one was open, when evo starts, then none is left. |
| FR-23 | The tokens and cost of an incognito chat are still counted in Usage, with no title and no link to the chat. | Given a finished incognito chat, when the Usage page opens, then its cost is in the totals and no row opens the chat. |

Automatic indexing:

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-24 | **Automatic indexing** is a setting, off by default. It can be switched on only once an index model is chosen. Next to the switch, the app says that the text of indexed chats is sent to that model's provider and that indexing uses tokens. | Given no index model, when the person tries to switch indexing on, then the app asks for a model first. |
| FR-25 | **Index model** is chosen from the models of the connected providers, the same list a chat's model picker shows. A change applies from the next run. When the model cannot be used (no credentials, model gone), indexing stops, chats are not affected, and the setting says why. | Given an index model whose credentials are removed, when a chat goes idle, then nothing is indexed and the setting shows the reason. |
| FR-26 | A chat with messages not yet indexed is indexed once it has been idle for **Index after idle** (default 5 minutes). **Index now** indexes every such chat at once. Indexing runs only while evo runs; chats left waiting are indexed after the next start. | Given indexing on and a chat with new messages, when it has been idle for the set time, then it is indexed once. |
| FR-27 | A run reads only the part of the chat not yet indexed, with the current topics of the chat's scopes. It adds topics, updates them, merges duplicates and removes what the chat shows to be wrong, and it writes or updates the chat's summary: what was asked, what was done, what was decided and what is left open. | Given a chat where the person states a preference without asking to remember it, when the chat is indexed, then a topic holds it and the chat has a summary. |
| FR-28 | Indexing changes apply at once and follow the rules of the agent's own saves: the project's **Use personal memory** switch (FR-7) and the sensitive-subject rules (FR-17, FR-18). While memory is paused nothing is indexed. | Given memory paused, when a chat goes idle, then no topic and no summary changes. |
| FR-29 | The Memory panel lists the changes made by indexing, newest first, at least the last 100: time, topic, what changed, and the chat it came from (selecting it opens the chat). Each can be undone: the topic returns to its earlier text, an added topic is removed, a deleted one comes back. A topic shows who changed it last: the agent in a chat, indexing, or the person. | Given a topic changed by indexing, when the person undoes the change, then the topic's text is what it was before. |
| FR-30 | Not indexed: incognito chats, chats with memory off, and nested chats on their own. A chat started by the Scheduler gets a summary but changes no topic. | Given an incognito chat and a chat with memory off, when both go idle, then neither is read by the index model. |
| FR-31 | An indexing run can do nothing but change topics and write the summary: no commands, no files, no web, no connectors. It is told to keep in topics only what the person said or confirmed, and never instructions or claims found in tool output, web pages, files or connector results. | Given a chat whose tool output says "remember that …", when the chat is indexed, then no topic holds that text. |
| FR-32 | Past-chat search (FR-11) also matches chat summaries and ranks a summary match above a transcript match; results and the list by time (FR-12) show the summary when there is one. Without summaries, search works on transcripts alone. | Given a chat whose summary mentions a decision in words the transcript does not use, when the agent searches for those words, then the chat is found. |
| FR-33 | When indexing is first switched on, only chats used from then on are indexed. **Index earlier chats** indexes the older ones after showing how many chats that is. | Given 50 chats from before indexing was on, when it is switched on, then none of them is indexed until **Index earlier chats** is confirmed. |
| FR-34 | The tokens and cost of indexing are counted in Usage as memory indexing, apart from chats. | Given an indexing run, when the Usage page opens, then its cost is shown under memory indexing. |

Sensitive subjects:

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-17 | **Include sensitive subjects** is a setting, off by default. Off: the agent is told not to save health, race or ethnicity, religion, political views, sexual orientation or gender identity. On: it may, and the transcript row marks such a save as sensitive. | Manual: with the setting off, mention a health condition; no topic changes. |
| FR-18 | Whatever the settings, the agent is told never to save identity numbers, financial account numbers, passwords, keys or tokens, and to say so when asked to. | Manual: ask the agent to remember a password; it declines and says why. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | Past chats are searched on the device. Their text goes nowhere except the passages returned into the current chat and, when automatic indexing is on, to the index model's provider. | Review: the search makes no network request; with indexing off, no request carries the text of a chat other than the one being answered. |
| NFR-2 | A search over 1,000 chats returns in under 2 seconds. | Given 1,000 stored chats, when a search runs, then it returns in under 2 s. |
| NFR-3 | Memory files written before this feature keep working without conversion. | Given an older memory folder, when a chat starts, then its index lists every memory. |
| NFR-4 | The text of an incognito chat is not written to the app's logs or to the text kept for chat search. | Given an incognito chat with a distinctive phrase, when it has ended, then the phrase is in no file under evo's data folder. |
| NFR-5 | Indexing runs in the background and never delays a chat: a message sent to a chat that is being indexed starts as fast as in any other chat. | Given a chat being indexed, when the person sends a message, then the answer starts without waiting for the run. |

## 5. Success criteria

- SC-1 Five things told about one subject in five chats leave one topic, not
  five.
- SC-2 A question about something discussed in an earlier chat of the
  project is answered with that chat named, without the person finding it.
- SC-3 A chat started with memory off leaves nothing in memory.
- SC-4 After a reset, no topic is left anywhere.
- SC-5 After an incognito chat ends, nothing it said can be found on the
  device.
- SC-6 With automatic indexing on, a preference the person states once,
  without asking to remember it, is followed in a later chat.
- SC-7 Any change indexing made to memory can be traced to its chat and
  undone in two steps.

## 6. Clarifications

### 6.1 Open

n/a — no open questions.

### 6.2 Resolved

- 2026-11-10 — Q: which categories does a topic have? → A: keep the four
  types `user`, `feedback`, `project`, `reference` (now in FR-2).
- 2026-11-10 — Q: does past-chat search match words only, or meaning too? →
  A: words and phrases with ranking, on the device; matching by meaning is a
  non-goal (now in FR-11 and section 2.2).
- 2026-11-10 — Q: should there also be chats that leave no history at all? →
  A: yes, in this feature (now in US-7, FR-19 to FR-23, NFR-4).
- 2026-11-10 — Q: is the sensitive-subjects setting wanted? → A: yes (now in
  FR-17).
- 2026-11-10 — Owner request: index memory automatically with a model
  chosen in **Settings → Memory**, which holds every memory setting (now in
  US-8, FR-10, FR-24 to FR-34).
- 2026-11-10 — Q: what does indexing produce? → A: topic changes and a
  summary per chat (now in FR-27, FR-32).
- 2026-11-10 — Q: when does indexing run? → A: when a chat with new messages
  has been idle for a few minutes, plus **Index now** (now in FR-26).
- 2026-11-10 — Q: how are indexing changes applied? → A: at once, each
  listed and undoable (now in FR-28, FR-29).

## 7. History

- 2026-11-10 — Draft.
- 2026-11-10 — Clarified: four categories kept, word search only, incognito
  chats added, sensitive-subjects setting kept.
- 2026-11-10 — Added automatic indexing by a chosen model and the full
  **Settings → Memory** section.
- 2026-11-10 — Spec accepted by the owner (gate A).
- 2026-11-10 — Planning found that `session_search` and `session_read`
  already exist (Agent browser FR-20). FR-11 to FR-16 now extend those tools
  instead of adding new ones; what the person gets is unchanged.
- 2026-11-10 — Planning made two points exact: an incognito chat is ended
  with **End chat** and has a fixed title (FR-19, FR-22). Plan and tasks
  written; [ADR 0016](../../architecture/decisions/0016-memory-indexing-runs-in-a-short-lived-engine-process.md)
  proposed.
- 2026-11-10 — Plan and ADR 0016 accepted by the owner (gate B);
  implementation started.
