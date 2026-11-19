## chart

A line, bar or area chart of one or more series over the same x labels.

Fields:
- `kind` (required): `"line"`, `"bar"` or `"area"`.
- `x` (required): the labels along the x axis, strings or numbers ("Mon", "10 Nov", 2024). They are shown as written.
- `series` (required): 1 to 6 series. Each series has:
  - `name` (required): the legend name.
  - `values` (required): one raw number per `x` label, in the same order; `null` for a missing point. It must have exactly as many values as `x` has labels.
- `format` (optional): how to write the y values.
- `stacked` (optional): `true` stacks a `bar` or `area` chart.

Example:

```evo-ui
{"v":1,"type":"chart","kind":"line","title":"Requests per day","source":"Gateway logs",
 "x":["Mon","Tue","Wed","Thu","Fri"],"format":"compact",
 "series":[{"name":"api","values":[12000,34000,29000,null,41000]},
           {"name":"web","values":[8000,9500,11000,10200,12500]}]}
```

Rules:
- At most 6 series and 1,000 points in all (labels times series).
- A series whose `values` is not as long as `x` is left out.
- The first series is drawn in the accent colour: put the one that matters most first.
- `line` for trends over time, `bar` for a few categories, `area` with `stacked` for a total made of parts.
- For one series of fewer than about 10 values, `bars` reads better.
