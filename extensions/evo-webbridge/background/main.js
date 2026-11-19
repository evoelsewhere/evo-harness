// Evo Webbridge's service worker (docs/features/browser-bridge/plan.md 3.2).
// It keeps the port to Evo open (an open native port keeps the worker alive),
// answers the side panel, and wires the parts that relay
// DevTools commands, keep the tab groups and draw the indicator.

import { connect, status, onStatus, on, send } from "./port.js";
import "./groups.js";
import { detachAll } from "./cdp.js";

/** What Evo told this profile: site permissions, and the chats working here now. */
const known = { sites: {}, working: [], settings: {} };
on("sites", ({ sites }) => {
  known.sites = sites || {};
});
on("working", ({ chats }) => {
  known.working = chats || [];
});

function siteOf(url) {
  try {
    const parsed = new URL(url);
    return /^https?:$/.test(parsed.protocol) ? parsed.origin : "";
  } catch {
    return "";
  }
}

// Side panels: each open panel is a session; its requests go to Evo and the
// answers and the chat's events come back (spec FR-10, FR-14).
const panels = new Map();
chrome.runtime.onConnect.addListener((port) => {
  if (port.name !== "panel") return;
  const session = crypto.randomUUID();
  panels.set(session, port);
  port.postMessage({ type: "status", status: { ...status } });
  port.onMessage.addListener((message) => {
    if (message?.type === "panel") send({ t: "panel", session, req: message.req, body: message.body });
    if (message?.type === "reconnect") connect({ launch: true });
  });
  port.onDisconnect.addListener(() => {
    panels.delete(session);
    send({ t: "panel", session, body: { op: "close" } });
  });
});
on("panel", ({ session, req, body }) => {
  panels.get(session)?.postMessage({ type: "panel", req, body });
});
on("settings", (settings) => {
  known.settings = settings;
  for (const port of panels.values()) port.postMessage({ type: "settings", settings });
});

// The browser just started: Evo may be started too (FR-19). Session storage
// is empty once per browser run, so the first start of this worker in a run
// asks; later starts (the worker woke up again) never reopen an Evo the person
// quit. `onStartup` alone is not enough: a browser that loads the extension
// from its command line does not send it. A worker woken by anything else
// connects here too; nothing may connect before it, or its `launch` is lost
// (one connection at a time).
async function connectAtStart() {
  const { started } = await chrome.storage.session.get("started");
  if (!started) await chrome.storage.session.set({ started: true });
  return connect({ launch: !started });
}
chrome.runtime.onStartup.addListener(connectAtStart);
connectAtStart();

// The toolbar button opens the side panel.
chrome.sidePanel?.setPanelBehavior?.({ openPanelOnActionClick: true }).catch(() => {});

// The side panel asks for the connection state and can ask to reconnect.
chrome.runtime.onMessage.addListener((request, _sender, reply) => {
  switch (request?.type) {
    case "status":
      if (status.state !== "connected") connectAtStart();
      reply({ ...status });
      return false;
    case "reconnect":
      connect({ launch: Boolean(request.launch) }).then(() => reply({ ...status }));
      return true;
    case "site-info":
      chrome.tabs.query({ active: true, lastFocusedWindow: true }).then(([tab]) => {
        const site = siteOf(tab?.url || "");
        reply({ site, permission: known.sites[site] || "ask", working: known.working });
      });
      return true;
    case "set-site":
      if (request.site) send({ t: "site", site: request.site, permission: request.permission });
      known.sites[request.site] = request.permission;
      reply(true);
      return false;
    case "stop-all":
      send({ t: "control", action: "stop_all" });
      detachAll().then(() => reply(true));
      return true;
    default:
      return false;
  }
});

onStatus(() => {
  for (const port of panels.values()) port.postMessage({ type: "status", status: { ...status } });
  chrome.runtime.sendMessage({ type: "status-changed", status: { ...status } }).catch(() => {});
  chrome.action.setBadgeText({ text: status.state === "connected" ? "" : "!" }).catch(() => {});
  chrome.action.setBadgeBackgroundColor({ color: "#666666" }).catch(() => {});
});
