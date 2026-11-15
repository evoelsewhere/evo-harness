// The issue-desk backend: an MCP server over stdio that evo runs with Deno
// under the plugin's permissions (docs/features/plugin-settings/spec.md,
// FR-10, FR-15, FR-26). It never holds a secret: it asks evo's host API
// for the GitHub token or the Hacker News cookies when it needs them, so a
// renewal never restarts it, and tells evo when a service refused them.
import { McpServer } from "npm:@modelcontextprotocol/sdk@1.17.0/server/mcp.js";
import { StdioServerTransport } from "npm:@modelcontextprotocol/sdk@1.17.0/server/stdio.js";
import { z } from "npm:zod@3.25.76";

const HOST = Deno.env.get("EVO_PLUGIN_HOST") ?? "";
const KEY = Deno.env.get("EVO_PLUGIN_KEY") ?? "";
// A setting left empty arrives as its unfilled `${...}`.
const setting = (name: string) => {
  const value = Deno.env.get(name) ?? "";
  return value.startsWith("${") ? "" : value;
};
const PAGE_SIZE = Number(setting("PAGE_SIZE")) || 20;
const STATE = setting("ISSUE_STATE") || "open";
const DEFAULT_REPO = setting("DEFAULT_REPO");

type Result = { content: { type: "text"; text: string }[]; isError?: boolean };
const reply = (value: unknown): Result => ({ content: [{ type: "text", text: typeof value === "string" ? value : JSON.stringify(value, null, 2) }] });
const fail = (message: string): Result => ({ ...reply(message), isError: true });

/** Ask evo's host API (`/v1/...`) with this backend's key. */
async function host(path: string, method = "GET"): Promise<any> {
  const response = await fetch(`${HOST}${path}`, { method, headers: { Authorization: `Bearer ${KEY}` } });
  const text = await response.text();
  const body = text ? JSON.parse(text) : null;
  if (!response.ok) throw new Error(body?.message ?? `evo answered ${response.status}`);
  return body;
}

/** A line in Settings → Logs (evo hides any secret in it). */
const log = (level: string, message: string) =>
  fetch(`${HOST}/v1/log`, { method: "POST", headers: { Authorization: `Bearer ${KEY}` }, body: JSON.stringify({ level, message }) }).catch(() => undefined);

async function github(path: string): Promise<any> {
  const { accessToken } = await host("/v1/accounts/github/token");
  const response = await fetch(`https://api.github.com${path}`, {
    headers: { Authorization: `Bearer ${accessToken}`, Accept: "application/vnd.github+json", "User-Agent": "evo-issue-desk" },
  });
  if (response.status === 401) {
    // The token was revoked at GitHub: evo marks the account and tells the person.
    await host("/v1/accounts/github/signed-out", "POST").catch(() => undefined);
    throw new Error("GitHub refused the sign-in; sign in again in evo");
  }
  if (!response.ok) throw new Error(`GitHub answered ${response.status}`);
  return response.json();
}

const shown = (issue: any) => ({ repo: issue.repository?.full_name ?? issue.repository_url?.split("/").slice(-2).join("/"), number: issue.number, title: issue.title, state: issue.state, url: issue.html_url });

const server = new McpServer({ name: "issue-desk", version: "1.0.0" });

server.tool("my_issues", "The GitHub issues assigned to you.", {}, async () => {
  try {
    const issues = await github(`/issues?filter=assigned&state=${STATE}&per_page=${PAGE_SIZE}`);
    return reply(issues.map(shown));
  } catch (error) {
    return fail(String((error as Error).message ?? error));
  }
});

server.tool(
  "repo_issues",
  "Issues of a GitHub repository (owner/name); the plugin's default repository when none is given.",
  { repo: z.string().optional() },
  async ({ repo }) => {
    const name = repo || DEFAULT_REPO;
    if (!/^[\w.-]+\/[\w.-]+$/.test(name)) return fail("Name a repository as owner/name, or set a default in the plugin's settings.");
    try {
      const issues = await github(`/repos/${name}/issues?state=${STATE}&per_page=${PAGE_SIZE}`);
      return reply(issues.filter((i: any) => !i.pull_request).map(shown));
    } catch (error) {
      return fail(String((error as Error).message ?? error));
    }
  },
);

/** GitHub with your sign-in when there is one; public repositories without. */
async function githubOrPublic(path: string): Promise<any> {
  try {
    return await github(path);
  } catch (error) {
    const response = await fetch(`https://api.github.com${path}`, { headers: { Accept: "application/vnd.github+json", "User-Agent": "evo-issue-desk" } });
    if (!response.ok) throw new Error(`GitHub answered ${response.status} (${(error as Error).message})`);
    return response.json();
  }
}

// The AI summary's tool (docs/features/plugin-ai/spec.md, FR-11): the model
// reads the issue through it when it needs to (`backendTools`).
server.tool(
  "issue_detail",
  "One GitHub issue: its title, state, labels, body and its last comments.",
  { repo: z.string(), number: z.number().int().positive() },
  async ({ repo, number }) => {
    if (!/^[\w.-]+\/[\w.-]+$/.test(repo)) return fail("Name the repository as owner/name.");
    try {
      const issue = await githubOrPublic(`/repos/${repo}/issues/${number}`);
      const comments = issue.comments ? await githubOrPublic(`/repos/${repo}/issues/${number}/comments?per_page=10`) : [];
      return reply({
        ...shown({ ...issue, repository: { full_name: repo } }),
        labels: (issue.labels ?? []).map((l: any) => l.name),
        author: issue.user?.login,
        body: String(issue.body ?? "").slice(0, 8000),
        comments: comments.map((c: any) => ({ author: c.user?.login, body: String(c.body ?? "").slice(0, 2000) })),
      });
    } catch (error) {
      return fail(String((error as Error).message ?? error));
    }
  },
);

server.tool("hn_whoami", "Who you are logged in as on Hacker News.", {}, async () => {
  try {
    const { header } = await host(`/v1/accounts/hn/cookies?url=${encodeURIComponent("https://news.ycombinator.com/")}`);
    const page = await (await fetch("https://news.ycombinator.com/", { headers: { Cookie: header, "User-Agent": "evo-issue-desk" } })).text();
    const me = page.match(/<a id="me" href="user\?id=([^"]+)"/)?.[1];
    if (!me) {
      await host("/v1/accounts/hn/signed-out", "POST").catch(() => undefined);
      return fail("Hacker News no longer knows you; log in again in evo.");
    }
    return reply({ user: me });
  } catch (error) {
    return fail(String((error as Error).message ?? error));
  }
});

await log("info", `issue-desk backend started (${STATE} issues, ${PAGE_SIZE} per list)`);
await server.connect(new StdioServerTransport());
