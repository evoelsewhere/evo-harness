// A composer action: the open chat's notes first, then the rest.
(async () => {
  const list = document.getElementById("list");
  const chat = await evo.chat.active().catch(() => null);
  const notes = await notesStore.all();
  notes.sort((a, b) => Number(b.chat === chat) - Number(a.chat === chat));
  if (notes.length === 0) list.append(el("div", { className: "empty", textContent: "No notes yet. Add some on the Sticky Wall." }));
  for (const note of notes.slice(0, 30)) {
    const label = note.text.length > 140 ? `${note.text.slice(0, 140)}…` : note.text;
    const item = el("button", { className: "item", textContent: label });
    item.style.textAlign = "left";
    item.onclick = () => evo.composer.insert(note.text);
    list.append(item);
  }
})();
