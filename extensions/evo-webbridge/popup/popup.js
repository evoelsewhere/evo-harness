// The toolbar popup: connection state, this site's permission, chats working
// in this browser, Open side panel, Stop all (spec FR-4, FR-12, FR-18).

const TEXT = {
  connecting: "Connecting to Evo…",
  connected: "Connected to Evo",
  "evo-not-running": "Evo is not running",
  "not-set-up": "Not set up yet",
  refused: "Evo refused the connection",
  error: "Something went wrong",
};

const HINT = {
  "evo-not-running": "Open Evo, then try again.",
  "not-set-up": "In Evo, open Settings → Webbridge and choose Set up for this browser.",
  refused: "The Evo found on this computer is not the one this extension was set up with.",
};

const $ = (id) => document.getElementById(id);

function render(status) {
  $("status").dataset.state = status.state;
  const label = status.state === "connected" && status.label ? ` · ${status.label}` : "";
  $("status-text").textContent = (TEXT[status.state] || status.state) + label;
  const hint = HINT[status.state];
  $("hint").hidden = !hint;
  $("hint").textContent = hint || "";
  $("retry").hidden = status.state === "connected" || status.state === "connecting";
  $("stop-all").hidden = status.state !== "connected";
}

async function refresh() {
  const status = await chrome.runtime.sendMessage({ type: "status" });
  if (status) render(status);
  const extra = await chrome.runtime.sendMessage({ type: "popup-info" }).catch(() => null);
  if (extra?.site) {
    $("site").hidden = false;
    $("site-name").textContent = extra.site;
    $("site-permission").value = extra.permission || "ask";
  }
  if (extra?.working?.length) {
    $("working").hidden = false;
    $("chats").replaceChildren(
      ...extra.working.map((chat) => {
        const li = document.createElement("li");
        const name = document.createElement("span");
        name.textContent = chat.title;
        const count = document.createElement("span");
        count.textContent = chat.tabs === 1 ? "1 tab" : `${chat.tabs} tabs`;
        li.append(name, count);
        return li;
      }),
    );
  }
}

$("retry").addEventListener("click", async () => {
  render(await chrome.runtime.sendMessage({ type: "reconnect", launch: true }));
});

$("open-panel").addEventListener("click", async () => {
  const win = await chrome.windows.getCurrent();
  await chrome.sidePanel.open({ windowId: win.id });
  window.close();
});

$("stop-all").addEventListener("click", async () => {
  await chrome.runtime.sendMessage({ type: "stop-all" });
  window.close();
});

$("site-permission").addEventListener("change", (event) => {
  chrome.runtime.sendMessage({ type: "set-site", site: $("site-name").textContent, permission: event.target.value });
});

chrome.runtime.onMessage.addListener((message) => {
  if (message?.type === "status-changed") render(message.status);
});

refresh();
