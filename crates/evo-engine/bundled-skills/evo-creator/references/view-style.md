# Making a view look like evo

evo's design language is in `design.md` (evo's DESIGN.md, carried here as
is): achromatic, Geist type at weights 400/500/600, borders drawn as
shadows, one blue accent for what is interactive, colour only in small
status dots, a 4 px spacing grid. A view gets the app's live theme as CSS
variables, so it follows light and dark and any change the person makes.
Use the variables, not the hex values in `design.md`: those are the light
theme only.

`templates/plugin/ui/style.css` does all of this; start from it.

## The theme variables

evo sets these on the view's `<html>` (and `data-evo-theme="light|dark"`,
`color-scheme`). Always give a fallback, for opening the file in a browser.

| Variable | Use it for | DESIGN.md role |
| --- | --- | --- |
| `--evo-background` | the page | background (primary) |
| `--evo-foreground` | text | text (primary) |
| `--evo-card`, `--evo-card-foreground` | raised surfaces: cards, rows, popovers | background (elevated) |
| `--evo-muted`, `--evo-secondary` | recessed surfaces: inputs, tracks, code | background (recessed) |
| `--evo-muted-foreground` | secondary text, hints, metadata | text (secondary / muted) |
| `--evo-accent`, `--evo-accent-foreground` | hover and selected rows | — |
| `--evo-border`, `--evo-input` | the 1 px shadow-border, input outlines | shadow-as-border |
| `--evo-ring` | focus rings | focus ring |
| `--evo-brand`, `--evo-brand-foreground` | the one accent: primary button, active state, links | interactive blue |
| `--evo-primary`, `--evo-primary-foreground` | a strong neutral button (near-black on light) | — |
| `--evo-link` | links in text | links |
| `--evo-destructive` | errors, delete | red status |
| `--evo-radius` | controls (6 px); cards use 12 px | radius scale |
| `--evo-font-sans`, `--evo-font-mono` | text, code | Geist Sans, Geist Mono |

The font variable names Geist first; a view does not have evo's font files,
so it falls back to the system UI font unless the plugin ships Geist
(OFL-licensed `.woff2` files in the plugin, loaded with `@font-face`). The
fallback is fine; ship the font only when the view is text-heavy.

## Rules, in the order they matter

1. **Achromatic.** Surfaces and text are the variables above. `--evo-brand`
   only for the primary action, the active item and focus; never a fill
   for decoration. Status colours (`design.md` §2) only as 8–10 px dots or
   small text, never as backgrounds.
2. **Borders are shadows.** `box-shadow: 0 0 0 1px var(--evo-border)` for
   containers; add depth with a second soft shadow only for floating things.
   No `<hr>`: separate with space or a surface change.
3. **Type.** 13 px body, 12 px labels and buttons, 11 px metadata; headings
   15 px at weight 600. Weights 400, 500, 600 only — never bold (700).
   Slight negative letter-spacing on headings (−0.01em to −0.02em).
4. **Space on a 4 px grid**: 4, 8, 12, 16, 24, 32. Pages get 16–24 px of
   padding; panels 12 px; cards 12–16 px.
5. **Radius**: `var(--evo-radius)` (6 px) for buttons and inputs, 12 px for
   cards, 9999 px only for pills and badges.
6. **Focus**: the double ring — `0 0 0 2px var(--evo-background), 0 0 0 4px
   var(--evo-ring)` — on `:focus-visible`.
7. **Motion**: hover changes only `background-color` and `color`, 150 ms.
   No scale, slide or fade on controls.
8. **Density**: show information, not decoration. No hero images, no
   gradients, no emoji as icons; SVG icons at 14–16 px in `currentColor`.

## Sizes of the places

- A **panel** is narrow: one column, controls that wrap, nothing wider
  than the panel (test at 320 px).
- A **page** has room: keep text lines under about 72 characters; a
  `max-width` of 960 px for forms and lists.
- A **card** sits in a chat: compact, at most 576 px wide; call
  `evo.view.resize(document.body.scrollHeight)` after rendering.
- A **composer** view is a 380 × 300 popover: one task, one primary button.

## Checklist

- Light and dark both (switch evo's theme; `evo.theme.onChange`).
- Long text truncates or wraps; no horizontal scroll.
- Empty, loading and error states drawn, in the muted colour.
- Every interactive element reachable by Tab, with the focus ring.
