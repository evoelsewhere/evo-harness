// DevTools commands for tabs in Evo's groups, through the browser's debugging
// API (spec FR-5, FR-20, FR-22; plan 3.3). Evo attaches when it needs a tab
// and detaches after the chat's turn; the browser shows its own notice while
// a tab is attached.

import { on, send, failure } from "./port.js";
import { isEvoTab } from "./groups.js";

const VERSION = "1.3";
/** Tabs this extension is attached to. */
const attached = new Set();

/** The wire code for a browser error message. */
function codeOf(message) {
  if (/another debugger|already attached/i.test(message)) return "busy";
  if (/no tab with|no target with|tab was closed|detached while/i.test(message)) return "gone";
  if (/cannot access|cannot attach|chrome:\/\/|edge:\/\/|extensions gallery|not allowed|webstore/i.test(message)) return "restricted";
  if (/policy|disabled by/i.test(message)) return "refused";
  return "failed";
}

function wrap(error) {
  const message = String(error?.message || error);
  return failure(codeOf(message), message);
}

export async function attach(tabId) {
  if (attached.has(tabId)) return;
  if (!(await isEvoTab(tabId))) throw failure("refused", "The tab is not in one of Evo's tab groups.");
  try {
    await chrome.debugger.attach({ tabId }, VERSION);
  } catch (error) {
    if (!/already attached/i.test(String(error?.message))) throw wrap(error);
  }
  attached.add(tabId);
}

export async function detach(tabId) {
  if (!attached.delete(tabId)) return;
  try {
    await chrome.debugger.detach({ tabId });
  } catch {
    // Already gone.
  }
}

export async function detachAll() {
  await Promise.all([...attached].map((tabId) => detach(tabId)));
}

on("attach", async ({ tab }) => {
  await attach(tab);
  return { attached: true };
});

// Stop all: Evo lets go of a tab at once (a notice, no answer).
on("detach-now", async ({ tab }) => {
  await detach(tab);
});

on("detach", async ({ tab }) => {
  await detach(tab);
  return { attached: false };
});

on("cdp", async ({ tab, session, method, params }) => {
  await attach(tab);
  const target = session ? { tabId: tab, sessionId: session } : { tabId: tab };
  try {
    return (await chrome.debugger.sendCommand(target, method, params ?? {})) ?? {};
  } catch (error) {
    throw wrap(error);
  }
});

chrome.debugger.onEvent.addListener((source, method, params) => {
  if (source.tabId === undefined || !attached.has(source.tabId)) return;
  send({ t: "event", tab: source.tabId, session: source.sessionId, method, params });
});

chrome.debugger.onDetach.addListener((source, reason) => {
  if (source.tabId === undefined || !attached.delete(source.tabId)) return;
  // `canceled_by_user`: the person pressed Cancel on the browser's notice.
  send({ t: "detached", tab: source.tabId, reason });
});
