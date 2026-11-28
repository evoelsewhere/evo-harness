// Connected apps' ways to connect and the names of a mate's grants
// (docs/features/connected-apps/spec.md, FR-20, FR-25, FR-27; T29).
import { describe, expect, test } from "bun:test";
import { serviceOfServer, waysFor } from "./connected-apps";

describe("connected apps", () => {
  test("a service with an official server offers it first, then Composio with a key", () => {
    const notion = { slug: "notion", official: "notion", composio: true };
    expect(waysFor(notion, true)).toEqual([
      { kind: "official", id: "notion" },
      { kind: "composio", toolkit: "notion" },
    ]);
    expect(waysFor(notion, false)).toEqual([{ kind: "official", id: "notion" }]);
    expect(waysFor({ slug: "gmail", official: null, composio: true }, true)).toEqual([{ kind: "composio", toolkit: "gmail" }]);
    expect(waysFor({ slug: "atlassian", official: "atlassian", composio: false }, true)).toEqual([{ kind: "official", id: "atlassian" }]);
  });

  test("a mate's grant names an official server or Zapier", () => {
    expect(serviceOfServer("app:official_github")).toBe("official:github");
    expect(serviceOfServer("app:zapier")).toBe("zapier");
    expect(serviceOfServer("app:connected_apps")).toBeNull();
    expect(serviceOfServer("github")).toBeNull();
  });
});
