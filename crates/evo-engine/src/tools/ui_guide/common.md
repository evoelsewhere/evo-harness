# Evo UI blocks

A block is a fenced code block with the info string `evo-ui` holding one JSON object. Every block has `"v":1` (the format version) and `"type"` (the component). Every block may also have `"title"` (a heading) and `"source"` (where the data comes from, shown under the block). Unknown fields are ignored. A block that is not valid JSON, or misses a required field, is shown as code.

- Use a block whenever the person compares or scans three or more related values or a sequence of steps. Not for a single number, a yes/no answer or plain prose.
- Take every number from a tool result or from the person. Never estimate or invent one.
- Give raw numbers (`1172.98`, not `"$1,172.98"`) and a `format`; Evo writes them in the person's locale.
- Name the `source`.
- Keep one or two sentences of interpretation outside the block. Do not repeat the block's data in prose.
- Formats: `number`, `compact`, `percent`, `bytes`, `duration`, `date`, `{"currency":"USD"}`. Read the `formats` guide for what each one expects.
