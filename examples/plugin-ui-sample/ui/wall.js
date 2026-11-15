// The wall: every note, in the order the person dragged them to.
const wall = document.getElementById("wall");
const text = document.getElementById("text");
let titles = new Map();
let dragged = null;

async function chatTitles() {
  try {
    titles = new Map((await evo.chat.list()).map((chat) => [chat.id, chat.title]));
  } catch {
    titles = new Map();
  }
}

async function move(id, before) {
  const all = await notesStore.all();
  const from = all.findIndex((n) => n.id === id);
  if (from === -1) return;
  const [moved] = all.splice(from, 1);
  all.splice(all.findIndex((n) => n.id === before), 0, moved);
  await notesStore.save(all);
}

async function render() {
  const notes = await notesStore.all();
  wall.replaceChildren();
  if (notes.length === 0) {
    wall.append(el("div", { className: "empty", textContent: "No notes yet." }));
    return;
  }
  for (const note of notes) {
    const remove = el("button", {
      title: "Delete",
      textContent: "✕",
      onclick: async () => {
        await notesStore.remove(note.id);
        render();
      },
    });
    const where = note.chat ? `· ${titles.get(note.chat) || "a chat"}` : "";
    const card = el(
      "div",
      { className: `note ${note.color}`, draggable: true },
      note.text,
      el("div", { className: "meta" }, el("span", { className: "grow", textContent: `${new Date(note.at).toLocaleDateString()} ${where}` }), remove),
    );
    card.addEventListener("dragstart", () => {
      dragged = note.id;
      card.classList.add("dragging");
    });
    card.addEventListener("dragend", () => card.classList.remove("dragging"));
    card.addEventListener("dragover", (event) => event.preventDefault());
    card.addEventListener("drop", async (event) => {
      event.preventDefault();
      if (!dragged || dragged === note.id) return;
      await move(dragged, note.id);
      dragged = null;
      render();
    });
    wall.append(card);
  }
}

async function add() {
  const value = text.value.trim();
  if (!value) return;
  const chat = document.getElementById("pin").checked ? await evo.chat.active() : null;
  await notesStore.add(value, chat, document.getElementById("color").value);
  text.value = "";
  render();
}

document.getElementById("add").onclick = add;
text.addEventListener("keydown", (event) => {
  if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) add();
});
chatTitles().then(render);
