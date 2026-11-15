# UI patterns for views

Ready pieces in evo's style, built on the classes of
`templates/plugin/ui/style.css`. Rules behind them: `view-style.md`; the
full design language: `design.md`.

## Page header

```html
<header class="row">
  <div class="grow">
    <h1>Issues</h1>
    <p class="hint">Assigned to you, newest first.</p>
  </div>
  <button class="ghost" id="refresh" title="Refresh">Refresh</button>
</header>
```

One title (15 px, 600), one line of hint (12 px, muted). Actions at the
right, the primary one last.

## A list of items

```html
<div class="card">
  <div class="list" id="list">
    <div class="item">
      <span class="dot ok"></span>
      <span class="grow truncate">Fix the login redirect</span>
      <span class="meta">2 h</span>
    </div>
  </div>
</div>
```

Rows separated by a 1 px shadow, not borders; status as a dot; time and
counts as 11 px muted text at the right. Build rows in JS with
`textContent`.

## Empty, loading, error

```html
<div class="empty">Nothing assigned to you.</div>
<div class="empty">Loading…</div>
<div class="error">GitHub answered 503. Try again in a minute.</div>
```

Every list has all three. An empty state says what would appear and, when
it helps, the one action that fills it.

## A form

```html
<form id="form" class="card">
  <label class="field">
    <span>Project</span>
    <input name="project" placeholder="acme/web" required>
    <span class="hint">owner/name</span>
  </label>
  <div class="row">
    <span class="grow"></span>
    <button type="button" class="ghost" id="cancel">Cancel</button>
    <button type="submit" class="primary">Save</button>
  </div>
</form>
```

```css
.field { display: flex; flex-direction: column; gap: 4px; font-size: 12px; font-weight: 500; }
.field .hint { font-weight: 400; }
.field input:invalid:not(:placeholder-shown) { box-shadow: 0 0 0 1px var(--danger); }
```

Labels above fields, 12 px, weight 500. One primary button per form, at
the right. Handle `submit` in JS with `preventDefault()` (forms may not
post anywhere).

## Buttons

| Class | For |
| --- | --- |
| `primary` | the one main action of a view or form (brand blue) |
| (none) | other actions: a card surface with a shadow border |
| `ghost` | toolbar and row actions, cancel |

28 px tall, 12 px text at weight 500. Destructive actions say what they do
("Delete note") and ask once.

## Tabs

```html
<div class="tabs" role="tablist">
  <button role="tab" aria-selected="true">Open</button>
  <button role="tab" aria-selected="false">Closed</button>
</div>
```

```css
.tabs { display: inline-flex; gap: 2px; padding: 2px; border-radius: 8px; background: var(--recessed); }
.tabs button { height: 24px; box-shadow: none; background: transparent; color: var(--muted); }
.tabs button[aria-selected="true"] { background: var(--card); color: var(--fg); box-shadow: 0 0 0 1px var(--line); }
```

## A table

```html
<table class="table">
  <thead><tr><th>Repo</th><th>Title</th><th class="num">Age</th></tr></thead>
  <tbody id="rows"></tbody>
</table>
```

```css
.table { width: 100%; border-collapse: collapse; font-size: 12px; }
.table th { text-align: left; font-weight: 500; color: var(--muted); padding: 6px 8px; }
.table td { padding: 6px 8px; box-shadow: 0 -1px 0 0 var(--line); }
.table .num { text-align: right; font-variant-numeric: tabular-nums; }
```

In a panel, prefer the list: tables need width.

## Badges and status

`<span class="badge">draft</span>` for a state word; `<span class="dot
warn"></span>` for health. Colour only at that size.

## Feedback

- Short confirmations: `evo.ui.notify("Saved", { level: "info" })`.
- Errors that need reading stay on the page (`.error`), next to what
  failed.
- A long call: disable its button and change its label ("Loading…") — no
  spinners larger than the text.

## A card in a chat

A card shows `evo.view.data()` (what the agent passed) compactly, at most
576 px wide, then sizes itself:

```js
const data = await evo.view.data();
render(data);
await evo.view.resize(document.body.scrollHeight);
```

No scroll inside a card; summarise and offer "Open" (`evo.ui.open("page")`)
for the full view.

## Light and dark

Never write a colour for text or surfaces: use the variables. Test both:

```js
evo.theme.onChange(() => render());
```
