## steps

An ordered list of steps, each with a status.

Fields:
- `items` (required): the steps, in order. Each step has:
  - `title` (required): the step.
  - `detail` (optional): one short line under it.
  - `status` (optional): `"done"`, `"current"`, `"todo"`, `"failed"` or `"skipped"`.

Example:

```evo-ui
{"v":1,"type":"steps","title":"Release 0.9.0",
 "items":[{"title":"Build","status":"done"},
          {"title":"Test","detail":"3 suites left","status":"current"},
          {"title":"Sign the installer","status":"todo"},
          {"title":"Publish","status":"todo"}]}
```

Rules:
- Use it for a sequence: a plan, a pipeline, what is done and what is left.
- Give each step the status it really has; at most one `current`.
- Keep `detail` to one line. Longer explanations go in the reply's text.
