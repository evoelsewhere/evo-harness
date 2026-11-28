# Evo Mates — Market research, 2026-11

| | |
| --- | --- |
| Spec | [spec.md](spec.md) (FR-8, FR-73 – FR-80) |
| Plan | [plan.md](plan.md) (3.10) |
| Updated | 2026-11-28 |

The owner asked for Evo Mates to be as complete as the best AI-teammate
apps. This page records a survey of about forty AI-teammate, AI-employee
and multi-agent workspace products on the market in November 2026: what
they do, as patterns, set against Evo Mates today. Products are not named
(repository rule); each pattern says how common it is: **very common** (ten
or more products), **common** (five to nine), **some** (two to four),
**rare** (one).

How far to trust it: most claims come from the products' own pages and
help centres; prices disagree between sources; review themes come from
public review sites and forums.

## 1. The market in one paragraph

Two shapes sell. Small-business products sell a fixed roster of named
"employees" with faces (a marketer, a sales rep, a support agent) that
share one business "brain" and are paid for in credits. Work and
enterprise products sell agents as members of a workspace: they are
@mentioned in channels, assigned tasks on a board, run on schedules and
triggers, and ask before anything with outside impact. In 2026 several
products merged or retired agent builders, moved reusable playbooks into
**skills** apart from the agent, and grouped work into **projects** (files,
instructions, memory and schedules in one place). The loudest complaint
everywhere is cost: credits drain in days and bills are hard to read.

## 2. Patterns and gaps

Value and cost are for one person running a few mates on their own
computer: **H** high, **M** medium, **L** low. "Built" names the
requirement that closes the gap in this round.

### 2.1 Onboarding and making a teammate

| Pattern seen | Evo Mates today | Gap | Value | Cost |
| --- | --- | --- | --- | --- |
| Gallery of ready-made agents by category (very common) | 8 roles in 6 categories with search (FR-8, FR-37) | A small gallery: no finance, people, product, social or study roles | M | L — built: FR-8 (16 roles) |
| Describe the job in your words; the product drafts the agent, then you refine by chat (very common) | **Start from scratch** opens a chat where the mate asks setup questions (FR-34) | No way to start from a description; the mate stays "New mate" until renamed by hand | H | L — built: FR-78 |
| Prebuilt **teams**: several agents wired together for one job (common in builder tools) | Team files import from disk (FR-20, FR-21) | No team to start from without a file | H | L — built: FR-75 |
| Conversation starters / suggested first requests under a new agent (very common) | A greeting, then an empty box | Nothing shows what to ask first | M | L — built: FR-74 |
| Onboarding that connects mail and calendar first, then proposes jobs (some) | Connect cards when a mate needs an app (FR-40) | Proposing jobs from the person's mail needs app events | M | H — not taken (2.2) |
| Custom name and avatar for agents you make (common) | Faces, hue, picture (FR-33) | — | — | — |
| Teach by recording a demonstration (rare) | — | — | L | H — not taken (2.2) |

### 2.2 Knowledge and memory

| Pattern seen | Evo Mates today | Gap | Value | Cost |
| --- | --- | --- | --- | --- |
| Files given to an agent as its knowledge, read when relevant (very common) | A mate's folder (FR-25); the person can attach files to one message | No lasting knowledge files a mate is told to consult | H | M — built: FR-77 |
| A shared business "brain" every agent reads, switchable per agent (common) | Personal memory read by every mate (FR-11) | Shared files for every mate | M | M — question for the owner |
| Editable memory view with history and restore (some, highly valued) | Memory tab, journal and undo (FR-26, learning layers) | — | — | — |
| Learning from corrections and thumbs (common) | Reactions told to the mate (FR-60), skill review and retro (learning layers) | — | — | — |
| Skills apart from the agent (common, growing) | Skills per mate, learned skills (FR-44, learning layers) | — | — | — |

### 2.3 Conversations and channels

| Pattern seen | Evo Mates today | Gap | Value | Cost |
| --- | --- | --- | --- | --- |
| Search across every conversation (very common in chat apps) | The roster search matches names; a conversation is searchable inside (FR-9) | Words said in any mate's conversation or channel cannot be found from one place | H | M — built: FR-76 |
| Many chats per agent, continuity through memory (very common) | One lasting conversation per mate by design (FR-9) | — (a choice) | — | — |
| @mention in a channel, reply in a thread, follow-ups without a new mention (very common) | Mentions and replies to whoever asked (FR-16, FR-72); no threads | Threads inside a channel | M | H |
| Router that picks who answers (common) | Auto rule with a router (FR-53) | — | — | — |
| Visible hand-offs with a trace (common in builder tools) | Hand-off lines with kinds (FR-36, FR-54) | — | — | — |
| A shared task board people and agents assign on (common) | Agenda per mate (FR-63); no board | A board across mates | M | H |
| Project spaces with files, instructions, memory and schedules (some, trend) | A channel has a folder and instructions (FR-14) | Channel memory and schedules | M | H |

### 2.4 Proactivity

| Pattern seen | Evo Mates today | Gap | Value | Cost |
| --- | --- | --- | --- | --- |
| Schedules (daily, weekly) set from a form (very common) | Check-ins set by asking the mate in chat; changed in the panel (FR-69, FR-70) | No way to add one without chatting | H | L — built: FR-79 |
| Daily brief in the morning, sometimes end of day (common, highly valued) | A check-in can do it if asked | No one-click brief | H | L — built: FR-79 presets |
| Event triggers: new mail, meeting ended, webhook (very common) | — | Needs app events from a server | H | H — not taken (2.2, sends data to a new third party) |
| Proactive nudges with a threshold (common) | Agenda, heartbeat, initiative budget (FR-63 – FR-67) | — | — | — |
| Email the agent / agents in outside chat apps (some to common) | — | Outside channels | M | H — not taken (2.2) |

### 2.5 Approvals, cost and oversight

| Pattern seen | Evo Mates today | Gap | Value | Cost |
| --- | --- | --- | --- | --- |
| Approve before outside impact; reads run freely (very common) | Approval modes, Sending on your behalf (FR-41, FR-42) | — | — | — |
| Always allow / Ask / Never per tool (common) | Always allow for read-only tools (FR-41) | — | — | — |
| Spend caps per agent, a breaker that pauses and asks (common; cost is the top complaint) | Usage per mate on the Usage page (FR-22); Pause by hand (FR-45) | Nothing stops a mate that spends too much | H | M — built: FR-80 |
| A home feed of what the team did, waiting approvals, failures (some) | Roster states; Needs you in Evo Harness (FR-12) | No single place to see the whole team's day | H | M — built: FR-73 |
| Run logs per agent (very common) | Transcript steps, app log | — | — | — |
| Cost per agent (common) | Usage page (FR-22) | Shown in Evo Harness only | M | L — built with FR-73 |

### 2.6 Sharing and reach

| Pattern seen | Evo Mates today | Gap | Value | Cost |
| --- | --- | --- | --- | --- |
| Share an agent with others; public marketplace (very common) | Team files (FR-20) | A marketplace | L for one person | H — not taken (2.2) |
| Agent definitions as portable files (some, trend) | Markdown team files (FR-27) | — | — | — |
| Phone apps and messaging channels (common) | — | — | M | H — not taken (2.2) |

## 3. What people say

Complaints, most frequent first: credits drain fast and bills are opaque;
billing traps around plan changes and cancelling; named helpers that do not
hand work to each other, so the person coordinates them; shallow, generic
output; wrong recipients and long delays; a learning curve and missing
integrations; products that keep changing; worry about noisy proactive
agents; writable memory and persona files as an attack surface.

Praise: hours saved on mail, scheduling and follow-ups; the "hire a named
employee" framing; building without code; proactive catches nobody asked
for; schedules and triggers that make agents more than chat.

What this means for Evo Mates: Evo runs on the person's own keys, so cost
is visible but not capped; a spend limit per mate (FR-80) and a daily view
of cost (FR-73) answer the top complaint. Mates already hand work to each
other in channels; ready-made teams (FR-75) make that the first thing a
person sees. Initiative budgets and Do not disturb already answer the worry
about noise.

## 4. Chosen for this round

FR-73 Today · FR-74 starters · FR-75 teams · FR-76 message search · FR-77
Knowledge · FR-78 Describe a mate · FR-79 check-ins from the panel · FR-80
daily spending limit, and FR-8's larger gallery. Left out, with why, in
spec 2.2 and 6.3.
