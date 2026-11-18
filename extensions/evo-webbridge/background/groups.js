// Evo's tab groups: one per chat in this profile (spec FR-6 – FR-9, FR-20).
// A tab belongs to a chat while it sits in that chat's group. Tabs the agent
// opens and tabs opened from them join the group; the person hands a tab over
// by dragging it in and takes it back by dragging it out.

import { on, send, failure } from "./port.js";

const COLOR = "blue";
const PREFIX = "Evo · ";

/** Evo tabs this worker knows: tab id → chat. Group changes Evo makes itself are not reported back. */
const owners = new Map();
const joining = new Set();

/** chat id → group id, for this browser session. */
async function groups() {
  const { groups = {} } = await chrome.storage.session.get("groups");
  return groups;
}

async function saveGroups(map) {
  await chrome.storage.session.set({ groups: map });
}

/** The chat a group belongs to. */
export async function chatOfGroup(groupId) {
  if (groupId === undefined || groupId < 0) return null;
  const map = await groups();
  return Object.keys(map).find((chat) => map[chat] === groupId) ?? null;
}

/** Whether a tab sits in one of Evo's groups. */
export async function isEvoTab(tabId) {
  try {
    const tab = await chrome.tabs.get(tabId);
    return (await chatOfGroup(tab.groupId)) !== null;
  } catch {
    return false;
  }
}

function describe(tab) {
  return { tab: tab.id, url: tab.url || tab.pendingUrl || "", title: tab.title || "", loading: tab.status === "loading", group: tab.groupId, window: tab.windowId };
}

async function lastWindow() {
  try {
    const win = await chrome.windows.getLastFocused({ windowTypes: ["normal"] });
    if (win?.id !== undefined) return win.id;
  } catch {
    // No window.
  }
  return (await chrome.windows.create({ focused: false })).id;
}

/** Put tabs into the chat's group, making it (in the tabs' window) when it does not exist. */
async function joinGroup(chat, title, tabIds) {
  const map = await groups();
  let groupId = map[chat];
  if (groupId !== undefined) {
    try {
      await chrome.tabGroups.get(groupId);
    } catch {
      groupId = undefined;
    }
  }
  for (const id of tabIds) joining.add(id);
  try {
    groupId = await chrome.tabs.group(groupId === undefined ? { tabIds } : { tabIds, groupId });
    await chrome.tabGroups.update(groupId, { title: PREFIX + (title || "chat"), color: COLOR });
    map[chat] = groupId;
    await saveGroups(map);
    for (const id of tabIds) owners.set(id, chat);
  } finally {
    for (const id of tabIds) joining.delete(id);
  }
  return groupId;
}

on("open", async ({ group, url, background = true }) => {
  if (!group?.chat) throw failure("failed", "open needs a chat");
  const map = await groups();
  let windowId;
  if (map[group.chat] !== undefined) {
    try {
      windowId = (await chrome.tabGroups.get(map[group.chat])).windowId;
    } catch {
      windowId = undefined;
    }
  }
  windowId ??= await lastWindow();
  const tab = await chrome.tabs.create({ url, active: !background, windowId });
  await joinGroup(group.chat, group.title, [tab.id]);
  return describe(await chrome.tabs.get(tab.id));
});

on("adopt", async ({ tab, chat, title }) => {
  await joinGroup(chat, title, [tab]);
  return describe(await chrome.tabs.get(tab));
});

on("release", async ({ tab }) => {
  owners.delete(tab);
  try {
    await chrome.tabs.ungroup(tab);
  } catch {
    // Closed already.
  }
  return { released: true };
});

on("close", async ({ tab }) => {
  try {
    await chrome.tabs.remove(tab);
  } catch {
    // Closed already.
  }
  return { closed: true };
});

on("title", async ({ chat, title }) => {
  const map = await groups();
  if (map[chat] !== undefined) {
    await chrome.tabGroups.update(map[chat], { title: PREFIX + title }).catch(() => {});
  }
});

/** The person's other tabs, titles and sites only (FR-9). */
on("list", async () => {
  const map = await groups();
  const evoGroups = new Set(Object.values(map));
  const tabs = await chrome.tabs.query({ windowType: "normal" });
  return tabs
    .filter((tab) => !evoGroups.has(tab.groupId) && /^https?:/.test(tab.url || ""))
    .map((tab) => {
      let site = "";
      try {
        site = new URL(tab.url).host;
      } catch {
        site = "";
      }
      return { tab: tab.id, title: tab.title || "", site, active: tab.active };
    });
});

/** The chat groups' tabs, for Evo after it reconnects. */
on("tabs", async () => {
  const map = await groups();
  const out = [];
  for (const [chat, groupId] of Object.entries(map)) {
    try {
      const tabs = await chrome.tabs.query({ groupId });
      for (const tab of tabs) out.push({ chat, ...describe(tab) });
    } catch {
      // The group is gone.
    }
  }
  return out;
});

// ── Changes the person or the page makes ────────────────────────────────────

chrome.tabs.onCreated.addListener(async (tab) => {
  // A tab the page opens from a group's tab joins that group (FR-7).
  if (tab.openerTabId === undefined) return;
  let opener;
  try {
    opener = await chrome.tabs.get(tab.openerTabId);
  } catch {
    return;
  }
  const chat = await chatOfGroup(opener.groupId);
  if (!chat) return;
  joining.add(tab.id);
  try {
    await chrome.tabs.group({ tabIds: [tab.id], groupId: opener.groupId });
  } catch {
    return;
  } finally {
    joining.delete(tab.id);
  }
  owners.set(tab.id, chat);
  send({ t: "tab", change: "joined", chat, opener: opener.id, ...describe(tab) });
});

chrome.tabs.onUpdated.addListener(async (tabId, change, tab) => {
  if ("groupId" in change) {
    if (joining.has(tabId)) return;
    const chat = await chatOfGroup(change.groupId);
    const before = owners.get(tabId);
    if (chat === before) return;
    // Out of a chat's group (taken back), into one (handed over), or both.
    if (before) {
      owners.delete(tabId);
      send({ t: "tab", change: "left", chat: before, ...describe(tab) });
    }
    if (chat) {
      owners.set(tabId, chat);
      send({ t: "tab", change: "joined", chat, ...describe(tab) });
    }
    return;
  }
  if (!("status" in change || "url" in change || "title" in change)) return;
  const chat = await chatOfGroup(tab.groupId);
  if (chat) send({ t: "tab", change: "updated", chat, ...describe(tab) });
});

chrome.tabs.onRemoved.addListener((tabId) => {
  if (!owners.delete(tabId)) return;
  send({ t: "tab", change: "closed", tab: tabId });
});

/** After the worker restarts: who owns which tab, from the groups kept for this session. */
async function rebuild() {
  const map = await groups();
  for (const [chat, groupId] of Object.entries(map)) {
    try {
      for (const tab of await chrome.tabs.query({ groupId })) owners.set(tab.id, chat);
    } catch {
      // The group is gone.
    }
  }
}
rebuild();

chrome.tabGroups.onRemoved.addListener(async (group) => {
  const map = await groups();
  const chat = Object.keys(map).find((c) => map[c] === group.id);
  if (!chat) return;
  delete map[chat];
  await saveGroups(map);
  send({ t: "group", change: "closed", group: group.id, chat });
});
