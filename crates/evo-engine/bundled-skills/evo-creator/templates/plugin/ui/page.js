// A view: state drawn for loading, empty, error and done; untrusted text
// set with textContent only; calls through window.evo (references/views.md).
const list = document.getElementById("list");
const summary = document.getElementById("summary");
const error = document.getElementById("error");

function el(tag, props = {}, ...children) {
  const node = Object.assign(document.createElement(tag), props);
  node.append(...children);
  return node;
}

async function load() {
  error.textContent = "";
  summary.textContent = "Loading…";
  list.replaceChildren();
  try {
    // The plugin's own backend tool (started if it sleeps).
    const result = await evo.backend.call("list_items", {});
    const text = result.content?.[0]?.text ?? "[]";
    if (result.isError) throw new Error(text);
    const items = JSON.parse(text);
    summary.textContent = items.length ? `${items.length} items` : "";
    if (!items.length) list.append(el("div", { className: "empty", textContent: "Nothing here yet." }));
    for (const item of items) {
      list.append(el("div", { className: "item" }, el("span", { className: "dot ok" }), el("span", { className: "grow truncate", textContent: item.title }), el("span", { className: "meta", textContent: item.when ?? "" })));
    }
  } catch (e) {
    summary.textContent = "Could not load.";
    error.textContent = String(e.message ?? e);
  }
  // A card or composer view sizes itself to its content.
  if (evo.view?.resize) evo.view.resize(document.body.scrollHeight).catch(() => undefined);
}

document.getElementById("load").addEventListener("click", load);
