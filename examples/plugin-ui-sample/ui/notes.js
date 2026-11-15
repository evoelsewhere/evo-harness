// The side panel: notes pinned to the open chat, and the agent's reply as it
// streams (chat.onEvent without a chat follows the window's open chat).
let chat = null;
let reply = "";
const list = document.getElementById("list");
const live = document.getElementById("live");

async function render() {
  list.replaceChildren();
  if (!chat) {
    list.append(el("div", { className: "empty", textContent: "Open a chat to pin notes to it." }));
    return;
  }
  const notes = (await notesStore.all()).filter((note) => note.chat === chat);
  if (notes.length === 0) list.append(el("div", { className: "empty", textContent: "No notes for this chat." }));
  for (const note of notes) {
    const remove = el("button", {
      textContent: "✕",
      title: "Delete",
      onclick: async () => {
        await notesStore.remove(note.id);
        render();
      },
    });
    list.append(el("div", { className: "item" }, remove, note.text));
  }
}

async function follow() {
  const now = await evo.chat.active().catch(() => null);
  if (now === chat) return;
  chat = now;
  const title = chat ? (await evo.chat.list()).find((c) => c.id === chat)?.title : null;
  document.getElementById("chat").textContent = title ? `Notes for “${title}”.` : "Notes for the open chat.";
  render();
}

document.getElementById("add").onclick = async () => {
  const input = document.getElementById("text");
  if (!input.value.trim() || !chat) return;
  await notesStore.add(input.value.trim(), chat, "yellow");
  input.value = "";
  render();
};

document.getElementById("keep").onclick = async () => {
  if (!chat) return;
  try {
    const messages = await evo.chat.messages(chat, { limit: 20 });
    const last = [...messages].reverse().find((m) => m.role === "assistant" && m.text.trim());
    if (!last) return evo.ui.notify("This chat has no reply yet.");
    await notesStore.add(last.text.trim().slice(0, 2000), chat, "green");
    evo.ui.notify("Kept the last reply as a note.", { title: "Sticky Wall", level: "success" });
    render();
  } catch (error) {
    live.replaceChildren(el("span", { className: "error", textContent: `${error.code}: ${error.message}` }));
  }
};

evo.chat.onEvent((event) => {
  if (event.type === "turn_start") reply = "";
  if (event.type === "text_delta") reply += event.delta;
  if (event.type === "tool_start") live.replaceChildren("Using ", el("b", { textContent: String(event.name) }), "…");
  if (event.type === "text_delta" || event.type === "turn_end") {
    live.replaceChildren(el("b", { textContent: event.type === "turn_end" ? "Last reply: " : "Replying: " }), reply.slice(-400));
  }
});

follow();
setInterval(follow, 1500);
