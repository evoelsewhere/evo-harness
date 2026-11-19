## table

A table the person can sort by clicking a header.

Fields:
- `columns` (required): 1 to 12 columns. Each column has:
  - `key` (required): the field of each row that holds this column's value.
  - `label` (optional): the header; `key` when left out.
  - `format` (optional): how to write the column's numbers.
  - `align` (optional): `"left"` or `"right"`. Put numbers on the right.
- `rows` (required): 1 to 500 rows. Each row is an object with a value per column `key`: a string, a raw number, or `null` for an empty cell. A row may also have:
  - `action` (optional): what a click on the row does: `{"send": text}` or `{"open": path or https link}`. Read the `actions` guide for the rules.

Example:

```evo-ui
{"v":1,"type":"table","title":"Slowest test packages","source":"cargo test, this run",
 "columns":[{"key":"name","label":"Package"},
            {"key":"tests","label":"Tests","format":"number","align":"right"},
            {"key":"time","label":"Duration","format":"duration","align":"right"}],
 "rows":[{"name":"evo-engine","tests":812,"time":74.2,"action":{"open":"crates/evo-engine/src/lib.rs"}},
         {"name":"evo-agent","tests":140,"time":9.5},
         {"name":"evo-store","tests":96,"time":3.1}]}
```

Rules:
- At most 500 rows and 12 columns. Leave out columns the person does not need.
- Give numbers as numbers so they sort as numbers.
- Do not name a column `action`: that field is the row's action.
