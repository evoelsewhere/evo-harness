// Evo Webbridge's service worker (docs/features/browser-bridge/plan.md 3.2).
// It keeps the port to Evo open (an open native port keeps the worker alive),
// answers the popup and the side panel, and wires the parts that relay
// DevTools commands, keep the tab groups and draw the indicator.

import { connect, status, onStatus } from "./port.js";
import "./groups.js";
import "./cdp.js";

// The browser just started: Evo may be started too (FR-19).
chrome.runtime.onStartup.addListener(() => connect({ launch: true }));
chrome.runtime.onInstalled.addListener(() => connect({ launch: false }));

// The toolbar button opens the popup; the side panel opens from it.
chrome.sidePanel?.setPanelBehavior?.({ openPanelOnActionClick: false }).catch(() => {});

// The popup and the side panel ask for the connection state and can ask to reconnect.
chrome.runtime.onMessage.addListener((request, _sender, reply) => {
  switch (request?.type) {
    case "status":
      if (status.state !== "connected") connect();
      reply({ ...status });
      return false;
    case "reconnect":
      connect({ launch: Boolean(request.launch) }).then(() => reply({ ...status }));
      return true;
    default:
      return false;
  }
});

onStatus(() => {
  chrome.runtime.sendMessage({ type: "status-changed", status: { ...status } }).catch(() => {});
  chrome.action.setBadgeText({ text: status.state === "connected" ? "" : "!" }).catch(() => {});
  chrome.action.setBadgeBackgroundColor({ color: "#666666" }).catch(() => {});
});

// A worker woken by anything else connects too.
connect();
