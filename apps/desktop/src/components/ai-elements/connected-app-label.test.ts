// The transcript line of a Connected apps call (docs/features/connected-apps/spec.md, FR-19).
import { describe, expect, test } from "bun:test";
import { type ToolPart } from "@/features/chat/transcript";
import { connectedAppLabel, toolHeader } from "./tool-labels";

const part = (name: string, args: unknown, phase: ToolPart["phase"] = "done"): ToolPart => ({
  kind: "tool",
  key: "call:k1",
  callId: "k1",
  name,
  args: args as any,
  argsText: "",
  phase,
  output: "",
  details: null,
  startedAt: 0,
  endedAt: 10,
});

describe("connected apps labels", () => {
  test("an app tool reads as Called <service> · <tool> (<account>)", () => {
    const args = { tools: [{ tool_slug: "GMAIL_SEND_EMAIL", connected_account_id: "work", arguments: { to: "a@example.com" } }] };
    expect(toolHeader(part("mcp__connected_apps__COMPOSIO_MULTI_EXECUTE_TOOL", args), false).label).toBe("Called Gmail · GMAIL_SEND_EMAIL (work)");
    expect(connectedAppLabel("COMPOSIO_MULTI_EXECUTE_TOOL", args, "running")).toBe("Calling Gmail · GMAIL_SEND_EMAIL (work)");
    const two = { tools: [{ tool_slug: "GITHUB_LIST_ISSUES" }, { tool_slug: "GITHUB_GET_REPO" }] };
    expect(connectedAppLabel("COMPOSIO_MULTI_EXECUTE_TOOL", two, "done")).toBe("Called Github · GITHUB_LIST_ISSUES +1");
  });

  test("through the search pair and for the meta-tools", () => {
    const args = { name: "mcp__connected_apps__COMPOSIO_MULTI_EXECUTE_TOOL", arguments: { tools: [{ tool_slug: "SLACK_POST_MESSAGE", account: "team" }] } };
    expect(toolHeader(part("mcp_call_tool", args), false).label).toBe("Called Slack · SLACK_POST_MESSAGE (team)");
    expect(connectedAppLabel("COMPOSIO_SEARCH_TOOLS", { queries: [{ use_case: "read my mail" }] }, "done")).toBe("Searched connected apps · read my mail");
    expect(connectedAppLabel("COMPOSIO_MANAGE_CONNECTIONS", { toolkits: ["notion"] }, "done")).toBe("Asked to connect · Notion");
    expect(connectedAppLabel("COMPOSIO_GET_TOOL_SCHEMAS", {}, "error")).toBe("Failed connected apps · get tool schemas");
  });

  test("other connectors keep their line", () => {
    expect(toolHeader(part("mcp__github__create_issue", {}), false).label).toBe("Called github · create_issue");
  });
});
