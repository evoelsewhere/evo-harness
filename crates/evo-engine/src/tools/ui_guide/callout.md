## callout

A short note set apart from the reply: something the person must not miss.

Fields:
- `tone` (required): `"info"`, `"success"`, `"warning"` or `"error"`.
- `text` (required): one or two sentences in inline Markdown: links, emphasis and `code`. No headings, lists or code blocks.

Example:

```evo-ui
{"v":1,"type":"callout","tone":"warning","title":"Missing data",
 "text":"The usage log has no entries for **14 Nov**, so that day's cost is not counted."}
```

Rules:
- At most one or two callouts in a reply. Ordinary prose stays prose.
- `error` for a failure, `warning` for a risk or a gap, `success` for a result the person was waiting for, `info` for anything else.
