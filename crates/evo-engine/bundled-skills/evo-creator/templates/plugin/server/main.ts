// A plugin backend: an MCP server over stdio that evo runs with Deno under
// the plugin's permissions (references/backend.md). It holds no secret: it
// asks evo's host API for an account's token when it needs one.
import { McpServer } from "npm:@modelcontextprotocol/sdk@1.17.0/server/mcp.js";
import { StdioServerTransport } from "npm:@modelcontextprotocol/sdk@1.17.0/server/stdio.js";
import { z } from "npm:zod@3.25.76";

const HOST = Deno.env.get("EVO_PLUGIN_HOST") ?? "";
const KEY = Deno.env.get("EVO_PLUGIN_KEY") ?? "";

/** A plain setting from `env`; an empty one arrives as its unfilled `${...}`. */
function setting(name: string, fallback = ""): string {
  const value = Deno.env.get(name) ?? "";
  return value && !value.startsWith("${") ? value : fallback;
}

/** Ask evo's host API with this backend's key. */
async function host(path: string, method = "GET", body?: unknown): Promise<any> {
  const response = await fetch(`${HOST}${path}`, {
    method,
    headers: { Authorization: `Bearer ${KEY}`, "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await response.text();
  const answer = text ? JSON.parse(text) : null;
  if (!response.ok) throw new Error(answer?.message ?? `evo answered ${response.status}`);
  return answer;
}

/** A line in Settings → Logs (secrets are hidden by evo). */
function log(level: "debug" | "info" | "warn" | "error", message: string) {
  return host("/v1/log", "POST", { level, message }).catch(() => undefined);
}

/** Call the service with the account's current token; report a refusal. */
async function service(path: string): Promise<any> {
  const { accessToken } = await host("/v1/accounts/service/token");
  const response = await fetch(`${setting("BASE_URL", "https://api.example.com")}${path}`, {
    headers: { Authorization: `Bearer ${accessToken}`, Accept: "application/json" },
  });
  if (response.status === 401) {
    await host("/v1/accounts/service/signed-out", "POST").catch(() => undefined);
    throw new Error("The service refused the sign-in; sign in again on the plugin's Accounts tab.");
  }
  if (!response.ok) throw new Error(`The service answered ${response.status}`);
  return response.json();
}

const ok = (value: unknown) => ({ content: [{ type: "text" as const, text: typeof value === "string" ? value : JSON.stringify(value) }] });
const fail = (error: unknown) => ({ ...ok(String((error as Error)?.message ?? error)), isError: true });

const server = new McpServer({ name: "my-plugin", version: "1.0.0" });

server.tool("list_items", "List the person's items in the service, newest first.", { limit: z.number().int().min(1).max(100).optional() }, async ({ limit }) => {
  try {
    const items = await service(`/items?limit=${limit ?? 20}`);
    return ok(items.map((item: any) => ({ id: item.id, title: item.title, when: item.updated_at })));
  } catch (error) {
    return fail(error);
  }
});

await log("info", "backend started");
await server.connect(new StdioServerTransport());
