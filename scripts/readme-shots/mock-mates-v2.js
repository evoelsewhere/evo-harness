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
  ];
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

  const baseSend = h.engine_send;
  h.engine_send = (args) => {
    const { chat, message } = args;
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
