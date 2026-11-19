## formats

How Evo writes a value. Give the raw number and one of these; Evo applies the person's locale.

- `"number"`: a plain number with separators. `1234.5` → 1,234.5.
- `"compact"`: a short large number. `3180000000` → 3.18B.
- `"percent"`: a percentage, not a ratio. `25` → 25%; `0.25` → 0.25%.
- `"bytes"`: a size in bytes. `1536` → 1.5 KB.
- `"duration"`: a time in seconds. `74.2` → 1m 14s; `0.25` → 250ms.
- `"date"`: an ISO date (`"2026-11-16"`), an ISO time (`"2026-11-16T09:30:00Z"`) or milliseconds since 1970. Written as a medium date.
- `{"currency":"USD"}`: money in the currency of that ISO 4217 code (`"EUR"`, `"JPY"`). `1172.98` → $1,172.98. The currency comes from the code, never from the locale.

Fields that take a format: `stats` items (`format`, `deltaFormat`), `bars` (`format`), `chart` (`format`, for the y values), `table` columns (`format`).

Example:

```evo-ui
{"v":1,"type":"stats","title":"Last build","source":"CI run 1832",
 "items":[{"label":"Duration","value":412,"format":"duration","delta":-8.5,"deltaFormat":"percent"},
          {"label":"Artifact size","value":48234496,"format":"bytes"},
          {"label":"Finished","value":"2026-11-16","format":"date"}]}
```

Rules:
- One format applies to every value of its field, so a field never mixes "$1K" and "$439.01".
- Never pre-format a number as a string ("$1.2K", "25%"): a string is shown as written and does not sort as a number.
- Without a format, a number is written as `number`.
