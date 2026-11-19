## stats

Number tiles for a few headline values: totals, counts, rates.

Fields:
- `items` (required): 1 to 8 tiles. Each tile has:
  - `label` (required): what the value is.
  - `value` (required): a raw number, or a short string when the value is not a number ("7/7").
  - `format` (optional): how to write `value`.
  - `delta` (optional): the signed change, as a raw number (`-12.5`, not `"-12.5%"`). Evo shows it with its sign and a status dot.
  - `deltaFormat` (optional): how to write `delta`; `format` when left out.
  - `hint` (optional): a short note under the value.

Example:

```evo-ui
{"v":1,"type":"stats","title":"Usage this week","source":"Usage page, 10–16 Nov",
 "items":[{"label":"Cost","value":1172.98,"format":{"currency":"USD"},"delta":-12.5,"deltaFormat":"percent"},
          {"label":"Tokens","value":3180000000,"format":"compact"},
          {"label":"Chats","value":42,"format":"number","hint":"7 still open"}]}
```

Rules:
- At most 8 tiles. Pick the values that matter; put the rest in a `table`.
- One number alone belongs in a sentence, not in a tile.
- `delta` is the change against an earlier period; say which one in `title` or `hint`.
