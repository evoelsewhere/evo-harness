## bars

A horizontal bar list: one value per item, ranked or in time order.

Fields:
- `items` (required): 1 to 50 bars. Each bar has:
  - `label` (required): the item.
  - `value` (required): a raw number.
  - `action` (optional): what a click on the bar does: `{"send": text}` or `{"open": path or https link}`. Read the `actions` guide for the rules.
- `format` (optional): how to write every value.
- `max` (optional): the value of a full bar (`100` for percentages). The largest value when left out.

Example:

```evo-ui
{"v":1,"type":"bars","title":"Cost per day","source":"Usage page, 10–14 Nov",
 "format":{"currency":"USD"},
 "items":[{"label":"11 Nov","value":1004.12,"action":{"send":"Show details for 11 Nov"}},
          {"label":"10 Nov","value":168.89},
          {"label":"12 Nov","value":45.6},
          {"label":"14 Nov","value":2.35}]}
```

Rules:
- Bars are drawn in the order given: sort them yourself (largest first for a ranking, oldest first for days).
- At most 50 bars. For more, or for many points over time, use a `chart` or a `table`.
- All values share one `format`.
