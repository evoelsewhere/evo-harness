// Notes kept in the plugin's own storage (`storage` permission, FR-13).
// One key, "notes": [{ id, text, color, chat, at }].
window.notesStore = {
  async all() {
    return (await evo.storage.get("notes")) || [];
  },
  async save(notes) {
    await evo.storage.set("notes", notes);
  },
  async add(text, chat, color) {
    const notes = await this.all();
    const note = { id: Date.now().toString(36) + Math.random().toString(36).slice(2, 6), text, chat: chat || null, color: color || "yellow", at: Date.now() };
    notes.unshift(note);
    await this.save(notes);
    return note;
  },
  async remove(id) {
    await this.save((await this.all()).filter((note) => note.id !== id));
  },
};

/** Text into an element, never HTML: chat text is untrusted. */
window.el = (tag, props, ...children) => {
  const node = document.createElement(tag);
  Object.assign(node, props || {});
  for (const child of children) node.append(child);
  return node;
};
