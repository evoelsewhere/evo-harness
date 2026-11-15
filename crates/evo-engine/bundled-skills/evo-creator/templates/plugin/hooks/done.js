// A Stop hook: runs when a turn ends. The event arrives as JSON on stdin;
// EVO_PLUGIN_HOST and EVO_PLUGIN_KEY reach evo's host API
// (references/manifest.md "Hooks", references/backend.md).
let input = "";
process.stdin.on("data", (chunk) => (input += chunk));
process.stdin.on("end", async () => {
  const event = JSON.parse(input || "{}");
  const host = process.env.EVO_PLUGIN_HOST;
  const key = process.env.EVO_PLUGIN_KEY;
  if (host && key) {
    await fetch(`${host}/v1/log`, {
      method: "POST",
      headers: { Authorization: `Bearer ${key}`, "Content-Type": "application/json" },
      body: JSON.stringify({ level: "info", message: `turn ended (${event.outcome ?? "?"})` }),
    }).catch(() => undefined);
  }
  process.exit(0);
});
