// A card the agent shows with plugin_show_card. Its data is either
//   { title, columns: [..], rows: [[..], ..] }   a table, or
//   { title, items: [{ label, value }, ..] }     bars.
const board = document.getElementById("board");

function table(data) {
  const head = el("tr", {}, ...data.columns.map((c) => el("th", { textContent: String(c) })));
  const rows = data.rows.map((row) => el("tr", {}, ...(Array.isArray(row) ? row : [row]).map((cell) => el("td", { textContent: String(cell ?? "") }))));
  return el("table", {}, el("thead", {}, head), el("tbody", {}, ...rows));
}

function bars(items) {
  const max = Math.max(...items.map((i) => Number(i.value) || 0), 1);
  const rows = items.map((item) => {
    const bar = el("div", { className: "bar" });
    bar.style.width = `${Math.max(2, ((Number(item.value) || 0) / max) * 100)}%`;
    const cell = el("td", {}, bar);
    cell.style.width = "55%";
    return el("tr", {}, el("td", { textContent: String(item.label) }), cell, el("td", { textContent: String(item.value) }));
  });
  return el("table", {}, el("tbody", {}, ...rows));
}

evo.view.data().then((data) => {
  // A model may have sent the data as a JSON string.
  if (typeof data === "string") {
    try {
      data = JSON.parse(data);
    } catch {
      data = {};
    }
  }
  data = data && typeof data === "object" ? data : {};
  if (data.title) board.append(el("h1", { textContent: String(data.title) }));
  if (Array.isArray(data.columns) && Array.isArray(data.rows)) board.append(table(data));
  else if (Array.isArray(data.items)) board.append(bars(data.items));
  else board.append(el("p", { className: "hint", textContent: "No data to show." }));
  // Fit the card to what it shows.
  requestAnimationFrame(() => evo.view.resize(Math.ceil(document.body.offsetHeight)));
});
