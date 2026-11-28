# Evo Mates — UX reference

| | |
| --- | --- |
| Spec | [spec.md](spec.md) (FR-29 – FR-38, NFR-7) |
| Behaviour | [behaviour.md](behaviour.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-28 |
| v2 | [Evo Mates UI v2](../evo-mates-ui-v2/spec.md) supersedes the layout details below where they differ: the sidebar folds to a rail of faces, faces carry presence marks, the profile opens with a hero and an Overview |

The owner asked for Evo Mates to look and behave like the AI-teammate apps
on the market: a messaging app with a roster of bots, bots with faces and
moods, and the same ways of talking to them, assigning work and approving.
This page records the patterns Evo Mates takes from that research, so the
plan and the UI follow one picture. It describes patterns, not any
product's artwork, names or trademarks: avatars, icons and words are Evo's
own (see 7).

## 1. Principles

1. **Calm by default.** Nothing moves, counts or blinks unless a mate is
   working or the person must act.
2. **Mates are people in a messaging app.** Faces, names, roles, bubbles
   and short status words; no developer vocabulary (no "session", "engine",
   "agent type", "tokens") in Evo Mates.
3. **Mates set themselves up.** A new mate asks what it needs in its
   conversation; the profile is for changing things later, not a form to
   fill before the first message.
4. **Every state is designed:** empty, loading, working, waiting, error,
   long text, narrow window, light and dark.
5. **One product with Evo Harness.** Same fonts, tokens, menus, dialogs and
   approval wording ([DESIGN.md](../../../DESIGN.md)); Mates adds bubbles
   and mate colours, nothing else.

## 2. Layout

Refined 2026-11-25 from the owner's reference screenshots: faces stand bare
(no plate) in lists, the conversation's bar sits in the window's title row
(face, name, state, Edit; the model pill and the profile toggle on the
right), a direct conversation shows replies in a neutral bubble with no
face beside each one (faces beside replies are for channels), and the
message box is one line ("Message <name>") with no model or approval
menus: the model is in the bar, approvals in the profile.


```text
┌──────────── sidebar 280 ────────────┬──────────── conversation ────────────┬── mate panel 320 ──┐
│ [search]                  [+ New]   │  ┌ header pill ──────────────────┐  │  (avatar 72)        │
│ ┌────┐┌────┐┌────┐  pinned tiles    │  │ ◉ Writer · Drafts copy  ● Working │  Writer             │
│ │ ◉  ││ ◉  ││ ◉  │  (60px avatar)   │  │              [model ▾] [⋯]  │  │  Drafts copy        │
│ └────┘└────┘└────┘                  │  └───────────────────────────────┘  │  ──────────────     │
│ Mates                               │        ◉ Writer  9:14                │  About · Tools ·    │
│  ◉• Planner   Plans work     9:02   │        ╭──────────────────╮         │  Memory · Files     │
│  ◉  Writer    Here's the …   9:14   │        │ Here's the intro …│         │                     │
│  ◉  Analyst   Queued          —     │        ╰──────────────────╯         │  instructions …     │
│ Channels                            │        [▸ Read 3 files ✓]           │  model, approvals … │
│  #  Launch room  Writer: …   9:20   │                   ╭──────────────╮  │                     │
│                                     │                   │ make it shorter│ │                     │
│                                     │                   ╰──────────────╯  │                     │
│ ⚙  Settings   ⇄ Evo Harness         │  ┌ composer ─────────────────────┐  │                     │
│                                     │  │ +  Message Writer…        ➤  │  │                     │
└─────────────────────────────────────┴──┴───────────────────────────────┴──┴─────────────────────┘
```

- **Sidebar** (280 px): search and **New** on top; pinned mates as tiles
  (60 px avatar, name under it); **Mates** and **Channels** lists; the
  footer holds **Settings** and **Switch to Evo Harness**.
- **Roster row** (36 px avatar, 8 px padding, 12 px radius): avatar with a
  state mark, name (13/18, 500), one line under it (last message, or the
  state word when not idle), time (12/16, tertiary). An unread or waiting
  dot on the right; no count badges, no summary banner.
- **Conversation**: a header pill floating over the transcript (avatar,
  name, role, state word, model pill, menu); the transcript fades in under
  it; the composer floats at the bottom (22 px radius, 18 px once it
  wraps).
- **Mate panel** (320 px, right): everything about one mate, opened from
  the header. Tabs: **About** (name, role, description, instructions),
  **Tools & apps** (tools, connectors, skills, approval mode), **Memory**,
  **Files** (its folder or project folder).
- **Narrow windows:** the sidebar folds to avatars when the conversation
  would drop under 440 px; under 920 px the mate panel floats over the
  conversation.

## 3. Messages

As built (2026-11-25), like a phone messaging app: the person's bubbles are
the brand blue with white text, the mate's grey; hovering a bubble shows
six reactions, Reply and Copy (right-click shows the same); a reaction is a
badge on the bubble's corner; a reply shows its quote above the bubble and
above the box; the box is a pill with + on its left and a round send button
that appears with text.


- **Bubbles**, 18 px radius. The person's on the right, neutral ink fill;
  a mate's on the left, tinted with the mate's colour at low strength. In a
  run of bubbles from one sender the corner on the sender's side tightens to
  6 px, and the avatar shows once, at the run's last bubble.
- **Sender line** above the first bubble of a run: name, role and time
  (12/16).
- **Activity chips** under a mate's bubble while it works and after:
  "Reading 3 files", "Searching the web", "Ran a command" — one chip per
  step group, opening the details in place. Never a raw tool name.
- **Cards** in the transcript for things to act on, using Evo's existing
  wording: approvals (**Allow** / **Deny**, **Always allow** where Harness
  offers it), questions (choices or a short answer), and suggestions
  (**Update** / **Not now**, later phase).
- **Memory chip**: "Saved to memory" under the message whose content a mate
  saved; clicking it opens the Memory tab on that entry.
- **Pictures** a mate took of its work (browser pages, app windows) fold
  into the transcript as thumbnails.
- **Streaming:** text fades in once per block; no typing dots.

## 4. States and status words

| State | Roster row | Header pill | Avatar |
| --- | --- | --- | --- |
| Idle | last message | role | still |
| Thinking | "Thinking…" | **Thinking** | eyes glance, slow breathe |
| Working | "Working…" or the current step | **Working** | breathes, glances, blinks; eyes follow the pointer on hover |
| Queued | "Queued" | **Queued** | still, dimmed |
| Needs you | "Needs you" + accent dot | **Needs you** | still, looks at the person |
| Unread | last message + dot | — | still |
| Error | "Couldn't reply" | **Couldn't reply** | still, eyes down |

Motion: avatars hold still at rest. Menus pop in 200 ms, panels 240 ms,
messages 180 ms, easing `cubic-bezier(0.22, 1, 0.36, 1)`, 300 ms ceiling
(600 ms for a streaming block's fade). Reduced motion turns every loop off.

## 5. Interactions

- **Meet your first mate** (empty Mates): a gallery of roles with category
  tabs (All, Engineering, Sales, Marketing, Operations, Personal, …) and
  search. A card: avatar, role name, one line, **Add**. **Start from
  scratch** under the gallery.
- **New mate**: **Add** creates the mate and opens its conversation; the
  mate greets the person, says what it can do and asks its setup questions
  (what to call it, what it should focus on, which apps it needs). Answers
  go into its profile; the person can change them later in the mate panel.
- **Assigning work**: write to the mate, or mention it in a channel
  (`@` opens a list of members with avatars and roles; arrow keys and Enter
  pick).
- **Hand-offs** in a channel show as a line: avatar → avatar, "Planner asked
  Analyst to check the numbers", expandable to the text.
- **Routing** under Auto shows one quiet line above the reply: "Writer is
  answering — fits: drafting copy".
- **Right-click a mate**: Pin, Mark unread, Edit profile, Duplicate, Copy
  conversation ID, Hide, Delete (Delete confirms; focus on Cancel).
- **Model picker** from the header's model pill: a provider rail on the
  left, the provider's models on the right, the default marked,
  unavailable providers dimmed with the reason.
- **Channel header**: stacked member avatars, the reply rule as a chip,
  **Stop** while anyone works.
- **Team import**: a review card before anything is created: mates (avatar,
  name, role, approval mode), the channel, instructions (expandable), a
  checklist of connectors, all off until switched on.
- **Keyboard**: Ctrl+K search, Ctrl+N new mate, Ctrl+1 – 9 pinned mates,
  Esc closes the mate panel (⌘ on macOS).

## 6. Mate avatars

- One face per mate, drawn by the **open-bot-avatar** engine (MIT, by
  Dominik Scholz): a small object character with eyes, rendered with
  implied 3D volume, idle motion, moods and gaze. Evo ships its own avatar
  definitions (silhouettes and eyes) and palette; it uses none of another
  product's characters or mascots. The engine's licence notice ships in
  the app's notices.
- **States** map to the engine's states: idle → `idle`, Thinking →
  `thinking`, Working → `working`, Needs you → `waiting`, Couldn't reply →
  `blocked`, a finished task → `done` for a moment, then `idle`. Mood is
  `happy` right after a finished task, otherwise `neutral`.
- **Gaze**: eyes follow the pointer while the mate is hovered or working
  (`followPointer`); a waiting mate looks at the composer (`lookAt`).
- **Motion**: the engine's idle breathing and blinking run only while the
  mate works or is hovered; at rest faces are drawn still (`still`).
  Reduced motion stops every loop.
- **Choice**: the gallery's roles each have a face; the person can pick
  another face, colour and material (`flat` or `fluffy`), or a picture of
  their own, in the mate panel.
- **Colour**: one hue per mate from a fixed palette, at one lightness and
  chroma per theme, as `agentHue` does for agents; used only on the face,
  its plate, the bubble tint and the mate's mention chip.
- **Rendering**: live faces (header, panel, working rows) mount the engine
  in the page; lists and small sizes use its static SVG so a long roster
  stays light.
- Sizes: 24 (mentions, hand-off lines), 36 (rows), 60 (tiles), 72 (panel),
  96 (gallery hero).
- Evo Harness's agents and subagents use the same faces, in each agent's
  own hue, with larger eyes at icon sizes
  ([subagents](../subagents-and-nested-sessions/spec.md) FR-25).

## 7. Borrowed patterns and what is not borrowed

Taken as patterns: the messaging layout, roster with faces and states,
bubbles with tinted mate replies, activity chips, inline approval and
question cards, mates that set themselves up in chat, role gallery with
categories, mention autocomplete, visible hand-offs and routing lines,
team review card, moods on avatars, calm motion. Behaviour patterns are in
[behaviour.md](behaviour.md).

Not taken: any product's names, logos, mascots, avatar artwork, icons,
illustrations, sounds or marketing text. Mascot characters and product
names of other apps are their owners' trademarks. The open-bot-avatar
engine is used under its MIT licence with Evo's own definitions.
