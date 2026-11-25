// A mate's secure card arrives as a request of its own kind, with the
// secret's name and no value (docs/features/evo-mates/spec.md, FR-43).
import { describe, expect, test } from "bun:test";
import { initialTranscript, transcriptReducer } from "./transcript";

describe("secret cards", () => {
  test("a secret request keeps its kind and fields", () => {
    const state = transcriptReducer(initialTranscript, {
      type: "engine",
      message: {
        type: "ui_dialog_request",
        requestId: "shell:agent-ask:1",
        ui: {
          kind: "secret",
          name: "GITHUB_TOKEN",
          service: "GitHub",
          reason: "open issues",
          title: "Ada needs your GitHub secret",
          description: "Ada needs it to open issues",
          options: [{ title: "Save" }, { title: "Cancel" }],
        },
      } as any,
    });
    const request = state.permissions?.[0];
    expect(request?.kind).toBe("secret");
    expect(request?.secret).toEqual({ name: "GITHUB_TOKEN", service: "GitHub", reason: "open issues" });
    expect(request?.options.map((o) => o.title)).toEqual(["Save", "Cancel"]);
  });

  test("other requests have no secret", () => {
    const state = transcriptReducer(initialTranscript, {
      type: "engine",
      message: { type: "ui_dialog_request", requestId: "r", ui: { title: "Allow?", options: [{ title: "Allow" }] } } as any,
    });
    expect(state.permissions?.[0]?.secret).toBeUndefined();
  });
});
