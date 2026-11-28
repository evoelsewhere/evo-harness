// Evo Mates UI v2 screenshots only (mates_v2_shots.py): loaded after
// mock-tauri.js, it gives the fake shell every state the redesign shows —
// a mate working, one waiting on an approval, unread conversations, a paused
// mate, a three-day conversation with steps, a reply quote and a long
// message, files, Today's data and a few gallery roles. Not app code.
(() => {
  const h = window.__mockHandlers;
  if (!h) return;
  const ago = (minutes) => Date.now() - minutes * 60_000;
  const day = (days, hour, minute = 0) => {
    const d = new Date();
    d.setDate(d.getDate() - days);
    d.setHours(hour, minute, 0, 0);
    return d.getTime();
  };
  const HOME = "C:\\Users\\demo";

  // ── Mates: Mo is paused; Rio gets a sixth mate's company ──
  const mates = h.mates_list();
  const mo = mates.find((m) => m.id === "mo");
  if (mo) mo.paused = true;
  const lin = mates.find((m) => m.id === "lin");
  if (lin) lin.pinned = true;
  const kai = mates.find((m) => m.id === "kai");
  if (kai) kai.pinned = true;
  for (const m of mates) {
    m.instructions = m.instructions || `You are ${m.name}, the team's ${m.role.toLowerCase()}. Keep replies short; put the result first.`;
  }

  // ── Chat rows: unread, previews and times ──
  const baseList = h.chats_list;
  h.chats_list = (args) => {
    const rows = baseList(args);
    if (args?.kind !== "mates") return rows;
    return rows.map((row) => {
      const edits = {
        "chat-ava": { unread: true, preview: "**Plan for Friday**\n\n- Build: done\n- Notes: two links left", updatedAt: ago(2) },
        "chat-kai": { unread: false, preview: "Running the release build on all three platforms.", updatedAt: ago(1) },
        "chat-lin": { unread: false, preview: "May I edit docs/release/4.14.md?", updatedAt: ago(6) },
        "chat-mo": { unread: false, preview: "Screenshots are refreshed in assets/.", updatedAt: ago(200) },
        "chat-rio": { unread: true, preview: "```\n3 sources agree: 12% growth\n```", updatedAt: ago(40) },
        "chan-launch": { unread: true, preview: "Ava: Everything is in place for Friday.", updatedAt: ago(3) },
        "chan-research": { unread: false, preview: "Rio: Two papers worth reading.", updatedAt: ago(900) },
      }[row.id];
      return edits ? { ...row, ...edits, turnCount: 3 } : row;
    });
  };

  // ── Who works and who waits ──
  h.engine_working = () => [
    { chat: "chat-kai", title: "Kai", state: "working" },
    { chat: "chat-lin", title: "Lin", state: "waiting" },
    { chat: "seat-kai", title: "Kai", state: "working" },
    { chat: "seat-mo", title: "Mo", state: "working" },
  ];
  // The icon badge (evo-mates-messaging FR-16).
  h.mates_set_badge = () => null;
  h.engine_questions = ({ chat }) =>
    chat === "chat-lin"
      ? [{ requestId: "q1", source: "engine", kind: "permission", title: "Edit docs/release/4.14.md", options: ["Allow", "Deny"], at: ago(6) }]
      : [];
  h.mates_queued = () => [];

  // ── Today ──
  h.mates_today = () => ({
    coming: [
      { id: "g1", mateId: "ava", kind: "checkin", text: "Morning brief: what is left for the launch", dueAt: day(-1, 9), repeat: "weekdays 09:00", status: "open", chatId: "chat-ava", createdAt: ago(9000), firedAt: null },
      { id: "g2", mateId: "rio", kind: "event", text: "Ask how the investor call went", dueAt: day(0, 17, 30), repeat: null, status: "open", chatId: "chat-rio", createdAt: ago(900), firedAt: null },
    ],
    team: [
      { mateId: "ava", replies: 6, costUsd: 0.142 },
      { mateId: "kai", replies: 4, costUsd: 0.318 },
      { mateId: "lin", replies: 3, costUsd: 0.087 },
      { mateId: "rio", replies: 1, costUsd: 0.021 },
    ],
  });
  h.mates_agenda = ({ id }) => (id === "ava" ? h.mates_today().coming.filter((c) => c.mateId === "ava") : []);
  h.mates_grants = () => [];
  h.mates_secrets = () => [];
  h.mates_knowledge = () => [];
  h.mates_search = ({ query }) =>
    String(query).toLowerCase().startsWith("lau")
      ? [{ chatId: "chan-launch", kind: "channel", snippet: "…We ship 4.14 on Friday. @Ava can you split the launch work…", updatedAt: ago(18) }]
      : [];

  // ── Files ──
  const file = (mateId, name, size, minutesAgo) => ({ mateId, name, size, modified: ago(minutesAgo), path: `${HOME}\\.evo\\agent\\mates\\${mateId}\\${name}` });
  h.mates_files = () => [
    file("lin", "release-notes-4.14.md", 6120, 4),
    file("kai", "CHANGELOG.md", 14210, 9),
    file("ava", "launch-plan.md", 3304, 30),
    file("mo", "hero-dark.png", 182_000, 200),
    file("rio", "market-sizes.csv", 2210, 1500),
    file("rio", "sources.md", 980, 3000),
  ];
  h.fs_stat = () => ({ exists: true, isDir: false, size: 1000 });
  // What a file holds, for the Files page's thumbnails.
  const PICTURE =
    "data:image/svg+xml;utf8," +
    encodeURIComponent(
      '<svg xmlns="http://www.w3.org/2000/svg" width="320" height="200"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#1e3a8a"/><stop offset="0.55" stop-color="#7c3aed"/><stop offset="1" stop-color="#f97316"/></linearGradient></defs><rect width="320" height="200" fill="url(#g)"/><rect x="24" y="24" width="150" height="14" rx="7" fill="#fff" opacity="0.85"/><rect x="24" y="48" width="96" height="10" rx="5" fill="#fff" opacity="0.6"/><rect x="24" y="120" width="272" height="56" rx="10" fill="#0b1020" opacity="0.45"/></svg>',
    );
  const TEXTS = {
    "release-notes-4.14.md": "# Evo 4.14\n\n## Highlights\n- **Evo Mates** gets a sidebar rail and presence\n- Find in a conversation\n- Today with counters\n\n## Fixes\n- Long links wrap in bubbles\n- Files open from replies",
    "CHANGELOG.md": "# Changelog\n\n## 4.14.0\n- feat(mates): sidebar rail of faces\n- feat(mates): quick switcher\n- feat(mates): steps chips\n- fix(replies): file not found\n- fix(ci): launcher script",
    "launch-plan.md": "# Launch plan\n\nFriday 10:00 — release build\nFriday 11:00 — notes live\nFriday 14:00 — post the update\n\n## Owners\n- Kai: build\n- Lin: notes\n- Mo: screenshots",
    "market-sizes.csv": "market,size,growth\nEU,12.4,9%\nUS,30.1,12%\nAPAC,18.7,15%\nLATAM,4.2,7%",
    "sources.md": "# Sources\n\n1. Industry report, 2026\n2. Public filings\n3. Two analyst notes\n\nAll three agree on 12% growth.",
  };
  h.fs_read_file = ({ path }) => {
    const name = String(path).split(/[\\/]/).pop();
    if (/\.png$/i.test(name)) return { kind: "image", dataUrl: PICTURE, size: 182000, mtime: Date.now() };
    if (TEXTS[name]) return { kind: "text", text: TEXTS[name], size: TEXTS[name].length, mtime: Date.now(), lossy: false };
    return { kind: "binary", size: 1000, mtime: Date.now() };
  };

  // ── Gallery roles ──
  const role = (id, name, role, category, definition, hue, description) => ({
    id, name, role, category, description, avatar: { definition, hue, material: "flat" }, approval: null,
    starters: [`What can you do as ${name}?`, "Plan my week", "Draft a short update"],
  });
  h.mates_templates = () => [
    role("writer", "Writer", "Drafts copy", "Marketing", "quill", 28, "Drafts posts, emails and pages in your voice."),
    role("engineer", "Engineer", "Writes and fixes code", "Engineering", "gear", 160, "Reads your project, fixes bugs and writes tests."),
    role("researcher", "Researcher", "Finds and checks facts", "Research", "scout", 275, "Searches the web and sums up what holds."),
    role("chief", "Chief of Staff", "Runs your day", "Operations", "pebble", 215, "Briefs you each morning and keeps follow-ups."),
    role("designer", "Designer", "Shapes how things look", "Design", "prism", 330, "Reviews screens and suggests clearer layouts."),
    role("bookkeeper", "Bookkeeper", "Keeps the books", "Finance", "pebble", 95, "Files receipts and tracks monthly spending."),
  ];
  h.mates_teams = () => [];

  // ── Ava's conversation: three days, steps, a quote, a long message ──
  const usage = { input: 6000, output: 200, cacheRead: 0, cacheWrite: 0, totalTokens: 6200, cost: { input: 0.01, output: 0.004, cacheRead: 0, cacheWrite: 0, total: 0.014 } };
  const user = (at, text, entryId) => ({ role: "user", content: [{ type: "text", text }], timestamp: at, entryId });
  const reply = (at, content, extra = {}) => ({ role: "assistant", content, provider: "openai", model: "gpt-5.1", api: "openai-responses", stopReason: "stop", usage, timestamp: at, ...extra });
  const tool = (at, id, name, text) => ({ role: "toolResult", toolCallId: id, toolName: name, content: [{ type: "text", text }], isError: false, timestamp: at });
  const LONG = Array.from({ length: 22 }, (_, i) => `${i + 1}. ${["Check the build", "Read the notes", "Confirm the links", "Post the update"][i % 4]} for step ${i + 1}.`).join("\n");
  const AVA = [
    user(day(2, 9, 12), "Morning! Can you keep an eye on the 4.14 launch this week?", "a1"),
    reply(day(2, 9, 13), [{ type: "text", text: "Morning! Yes — I'll track the build, the notes and the screenshots, and tell you what's left each morning." }]),
    user(day(1, 16, 40), "What did the team finish today?", "a2"),
    reply(day(1, 16, 41), [
      { type: "toolCall", id: "t1", name: "read", arguments: { path: "docs/release/4.14.md" } },
      { type: "toolCall", id: "t2", name: "read", arguments: { path: "CHANGELOG.md" } },
    ], { stopReason: "toolUse" }),
    tool(day(1, 16, 41), "t1", "read", "# 4.14"),
    tool(day(1, 16, 41), "t2", "read", "## 4.14"),
    reply(day(1, 16, 42), [
      { type: "toolCall", id: "t3", name: "web_search", arguments: { query: "release checklist desktop app" } },
    ], { stopReason: "toolUse" }),
    tool(day(1, 16, 42), "t3", "web_search", "results"),
    reply(day(1, 16, 43), [{ type: "text", text: "Kai cut the release build and wrote **14 changelog entries**. Lin drafted the notes in `docs/release/4.14.md`. Mo refreshed the dark screenshots." }]),
    user(day(1, 16, 50), "Give me the full checklist for Friday, step by step.", "a3"),
    reply(day(1, 16, 51), [{ type: "text", text: `Here is the whole list:\n\n${LONG}` }]),
    user(day(0, 8, 58), '<evo-mates-reply from="Ava">Kai cut the release build and wrote 14 changelog entries. Lin drafted the notes in docs/release/4.14.md.</evo-mates-reply>\nAre the two beta links fixed yet?', "a4"),
    reply(day(0, 9, 0), [{ type: "text", text: "Not yet — Lin marked them. I'll ask her to swap them for the release docs." }]),
    reply(ago(2), [{ type: "text", text: "**Plan for Friday**\n\n- Build: done\n- Notes: two links left\n- Screenshots: done\n\nI'll check the links again at 3 pm." }]),
  ];

  // ── Messaging basics 1b (evo-mates-messaging): reactions, a quoted reply,
  // a mention of the person with a link, a hand-off with a file, pins ──
  const LAUNCH = [
    { kind: "message", id: "p1", author: "person", text: "We ship 4.14 on Friday. @Ava can you split the launch work between the team?", at: ago(18) },
    { kind: "route", message: "p1", picked: ["ava"], line: "Ava, as you mentioned", at: ago(18) },
    { kind: "message", id: "a1", author: "ava", name: "Ava", text: "On it. Kai takes the release build and the changelog, Lin writes the notes from it, Mo refreshes the screenshots.", at: ago(17) },
    { kind: "handoff", message: "p1", handoff: "assigns", from: "ava", fromName: "Ava", to: "kai", toName: "Kai", text: "Cut the 4.14 release build and update CHANGELOG.md", at: ago(17) },
    { kind: "board", text: "Ava added LAUNCH-1 Cut the 4.14 release build for Kai", card: "LAUNCH-1", cardId: "card-1", at: ago(17) },
    { kind: "message", id: "k1", author: "kai", name: "Kai", text: "Build is green on all three platforms. CHANGELOG.md has 14 entries under 4.14.", at: ago(9) },
    { kind: "board", text: "Kai moved LAUNCH-1 to Review", card: "LAUNCH-1", cardId: "card-1", at: ago(9) },
    { kind: "handoff", message: "p1", handoff: "tells", from: "mo", fromName: "Mo", to: "lin", toName: "Lin", text: "The new dark screenshot for the notes", files: [`${HOME}\\.evo\\agent\\mates\\mo\\hero-dark.png`], at: ago(7) },
    { kind: "message", id: "l1", author: "lin", name: "Lin", text: "Great, I will use those 14 entries for the highlights.", quote: "k1", at: ago(5) },
    { kind: "message", id: "k2", author: "kai", name: "Kai", text: "@you can you confirm the two beta links before Friday? The release docs are at https://example.com/docs/release", at: ago(3) },
    // A thread under Kai's build line (evo-mates-threads-board FR-1 – FR-6).
    { kind: "job", id: "j1", message: "p1", status: "done", handoffs: 2, at: ago(3) },
    { kind: "message", id: "t1", author: "person", text: "Can you add the arm64 build for Linux too?", thread: "k1", at: ago(8) },
    { kind: "route", message: "t1", picked: ["kai"], line: "Kai, in this thread", thread: "k1", at: ago(8) },
    { kind: "message", id: "t2", author: "kai", name: "Kai", text: "Added `linux-arm64` to the matrix. It builds in 6 min; the artifact is in the release draft.", thread: "k1", replyTo: "t1", at: ago(6) },
    { kind: "handoff", message: "t1", handoff: "asks", from: "kai", fromName: "Kai", to: "mo", toName: "Mo", text: "@Mo can you add arm64 to the download page screenshot?", thread: "k1", at: ago(6) },
    { kind: "message", id: "t3", author: "mo", name: "Mo", text: "Yes, I will retake it once the page lists arm64.", thread: "k1", at: ago(4) },
    { kind: "job", id: "j2", message: "t1", status: "done", handoffs: 1, thread: "k1", at: ago(4) },
    // Linking (evo-mates-threads-board FR-16 – FR-22): a reply also sent to the
    // channel, a thread under Ava's plan, and Lin's share from #Research notes.
    { kind: "message", id: "t4", author: "person", text: "Thanks Kai, arm64 is in the matrix now.", thread: "k1", alsoChannel: true, at: ago(3) },
    { kind: "message", id: "t5", author: "person", text: "Can Mo take the screenshots after Lin's notes?", thread: "a1", at: ago(15) },
    { kind: "message", id: "t6", author: "ava", name: "Ava", text: "Yes, I moved it after the notes on the board.", thread: "a1", at: ago(14) },
    { kind: "share", id: "s1", by: "lin", name: "Lin", from: "#Research notes", text: "Two papers measured arm64 builds 8% faster with LTO on. Worth a line in the notes?", at: ago(2) },
  ];
  h.channel_reactions = ({ channel }) =>
    channel === "launch"
      ? [
          { message: "a1", reactor: "kai", reaction: "like", atMs: ago(16) },
          { message: "a1", reactor: "lin", reaction: "like", atMs: ago(16) },
          { message: "k1", reactor: "person", reaction: "heart", atMs: ago(8) },
          { message: "k1", reactor: "ava", reaction: "heart", atMs: ago(8) },
          { message: "k1", reactor: "lin", reaction: "emphasis", atMs: ago(6) },
        ]
      : [];
  h.channel_react = () => null;
  h.mates_mentions = () => [{ chat: "chan-launch", line: "k2", author: "kai", name: "Kai", excerpt: "@you can you confirm the two beta links before Friday?", at: ago(3) }];
  h.mates_mentions_read = () => null;
  // ── Board of the Launch room (evo-mates-threads-board FR-8 – FR-15) ──
  const COLUMNS = [
    { id: "col-todo", name: "To do", position: 0 },
    { id: "col-doing", name: "In progress", position: 1 },
    { id: "col-review", name: "Review", position: 2 },
    { id: "col-done", name: "Done", position: 3 },
  ];
  const dayAt = (days) => {
    const d = new Date();
    d.setDate(d.getDate() + days);
    d.setHours(23, 59, 0, 0);
    return d.getTime();
  };
  const card = (n, column, title, more = {}) => ({
    id: `card-${n}`, channelId: "launch", key: `LAUNCH-${n}`, number: n, columnId: column, position: n, title,
    description: "", assignee: null, priority: "none", dueMs: null, labels: [], checklist: [], links: [], threadRoot: null,
    source: null, createdBy: "ava", createdMs: ago(20), updatedMs: ago(5), ...more,
  });
  const CARDS = [
    card(6, "col-todo", "Confirm the two beta links", { assignee: "person", priority: "high", dueMs: dayAt(0), labels: ["release"] }),
    card(4, "col-todo", "Retake the download page screenshot with arm64", { assignee: "mo", priority: "medium", dueMs: dayAt(1), labels: ["design"] }),
    card(7, "col-todo", "Post the launch thread on the forum", { labels: ["marketing"] }),
    card(2, "col-doing", "Write the 4.14 release notes from the changelog", {
      assignee: "lin", labels: ["docs"], description: "Highlights first, then **fixes**. Link every entry to its pull request.",
      checklist: [{ text: "Highlights", done: true }, { text: "Fixes", done: true }, { text: "Known issues", done: false }, { text: "Links", done: false }],
    }),
    card(5, "col-doing", "Add linux-arm64 to the build matrix", { assignee: "kai", priority: "urgent", threadRoot: "k1", labels: ["release"] }),
    card(1, "col-review", "Cut the 4.14 release build and update CHANGELOG.md", { assignee: "kai", priority: "high", dueMs: dayAt(-1), labels: ["release"], source: { from: "ava", to: "kai" } }),
    card(3, "col-done", "Refresh the README screenshots", { assignee: "mo", labels: ["design"] }),
  ];
  const big = (() => {
    try { return localStorage.getItem("mock.bigBoard") === "1"; } catch { return false; }
  })();
  const ALL = big
    ? Array.from({ length: 500 }, (_, i) => card(i + 1, COLUMNS[i % 4].id, `Task ${i + 1}: ${["Check", "Write", "Fix", "Review", "Ship"][i % 5]} the ${["build", "notes", "docs", "page", "tests"][i % 5]}`, {
        assignee: [null, "kai", "lin", "mo", "person"][i % 5], priority: ["none", "low", "medium", "high", "urgent"][i % 5], labels: i % 3 ? [] : ["release"],
      }))
    : CARDS;
  const BOARD = { channelId: "launch", key: "LAUNCH", enabled: true, nextNumber: ALL.length + 1, columns: COLUMNS };
  const placed = (c) => ({ card: c, channelName: "Launch room", chatId: "chan-launch", column: COLUMNS.find((x) => x.id === c.columnId)?.name ?? "" });
  h.board_get = ({ channel }) =>
    channel === "launch" ? { board: BOARD, cards: ALL, proposedKey: "LAUNCH" } : { board: null, cards: [], proposedKey: "RESEARCH" };
  h.board_keys = () => ["LAUNCH"];
  h.board_column_count = ({ column }) => ALL.filter((c) => c.columnId === column).length;
  h.cards_assigned = ({ assignee }) => ALL.filter((c) => c.assignee === assignee && c.columnId !== "col-done").slice(0, 8).map(placed);
  h.card_by_key = ({ key }) => {
    const c = ALL.find((x) => x.key === key);
    return c ? placed(c) : null;
  };
  h.card_events = ({ id }) =>
    id === "card-2"
      ? [
          { id: 1, cardId: id, actor: "ava", kind: "created", text: "created LAUNCH-2 in To do, assigned to Lin", atMs: ago(17) },
          { id: 2, cardId: id, actor: "lin", kind: "moved", text: "moved LAUNCH-2 to In progress", atMs: ago(12) },
          { id: 3, cardId: id, actor: "lin", kind: "updated", text: "updated the checklist (2 of 4) on LAUNCH-2", atMs: ago(6) },
          { id: 4, cardId: id, actor: "person", kind: "comment", text: "Keep the highlights to five.", atMs: ago(4) },
        ]
      : [{ id: 1, cardId: id, actor: "ava", kind: "created", text: "created the card", atMs: ago(17) }];
  // ── Threads and links (evo-mates-threads-board FR-16 – FR-22) ──
  const THREADS = [
    { chatId: "chan-launch", root: "k1", rootExcerpt: "Build is green on all three platforms. CHANGELOG.md has 14 entries under 4.14.", following: true, resolved: false, replies: 4, lastMs: ago(3), lastAuthor: "mo", lastReadMs: ago(6), unread: true },
    { chatId: "chan-launch", root: "a1", rootExcerpt: "On it. Kai takes the release build and the changelog, Lin writes the notes…", following: true, resolved: true, replies: 2, lastMs: ago(14), lastAuthor: "ava", lastReadMs: ago(14), unread: false },
    { chatId: "chan-research", root: "r9", rootExcerpt: "Two papers worth reading on build times.", following: true, resolved: false, replies: 3, lastMs: ago(60), lastAuthor: "rio", lastReadMs: ago(70), unread: true },
  ];
  h.threads_followed = () => THREADS;
  h.threads_of = ({ chat }) => THREADS.filter((t) => t.chatId === chat);
  h.linked_of = ({ chat }) =>
    chat === "chan-launch" || chat === "chat-ava"
      ? [
          { id: "sh1", fromChat: "chan-research", fromLines: ["r9"], excerpt: "Two papers measured arm64 builds 8% faster with LTO on.", toChat: "chan-launch", note: "", by: "lin", atMs: ago(2) },
          { id: "sh2", fromChat: "chan-launch", fromLines: ["k1", "l1"], excerpt: "Build is green… I will use those 14 entries", toChat: "chat-ava", note: "Plan the wrap-up from these", by: "person", atMs: ago(20) },
        ]
      : [];
  for (const name of ["share_record", "thread_set", "thread_read"]) h[name] = () => null;
  for (const name of ["board_set", "board_column", "card_create", "card_update", "card_move", "card_delete", "card_comment"]) h[name] = () => null;
  h.link_preview = ({ url }) => ({ url, title: "Release notes and docs for 4.14", site: "Example Docs", description: "Everything new in this release.", image: null });
  const baseSettings = h.settings_all;
  h.settings_all = () => ({
    ...baseSettings(),
    "mates.linkPreviews": true,
    "mates.pins": {
      "chan-launch": [
        { key: "a1", excerpt: "On it. Kai takes the release build and the changelog, Lin writes the notes…", at: ago(16) },
        { key: "k1", excerpt: "Build is green on all three platforms.", at: ago(8) },
      ],
    },
    "mates.mute": { "chan-research": { until: null } },
    "computer_apps.enabled": true,
    "mates.computer": { ava: { chrome: false } },
  });

  const baseSend = h.engine_send;
  h.engine_send = (args) => {
    const { chat, message } = args;
    if (message?.type === "get_session_branch" && chat === "chan-launch") {
      setTimeout(
        () =>
          window.__mockEmit("engine://message", {
            chat,
            generation: 1,
            message: { type: "response", id: message.id, command: message.type, success: true, data: { entries: LAUNCH.map((data) => ({ type: "custom", customType: "channel", data })) } },
          }),
        10,
      );
      return null;
    }
    if (message?.type === "get_messages" && chat === "chat-ava") {
      setTimeout(
        () =>
          window.__mockEmit("engine://message", {
            chat,
            generation: 1,
            message: { type: "response", id: message.id, command: message.type, success: true, data: { messages: AVA } },
          }),
        10,
      );
      return null;
    }
    return baseSend(args);
  };
})();
