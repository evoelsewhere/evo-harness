import { describe, expect, test } from "bun:test";
import type { ModelDescriptor } from "@/engine/protocol";
import { mateModelPatch } from "./model-picker";

// docs/features/evo-mates/spec.md (FR-7, FR-10, FR-28): the model picked for
// a mate is the mate's, so its seats, a restart and a profile change keep it.
describe("a mate's model choice", () => {
  const model = { id: "small-1", name: "Small 1", provider: "acme" } as ModelDescriptor;

  test("a picked model is saved to the mate's profile", () => {
    expect(mateModelPatch({ model })).toEqual({ provider: "acme", model: "small-1" });
  });

  test("a picked effort is saved too", () => {
    expect(mateModelPatch({ thinking: "low" })).toEqual({ thinking: "low" });
  });
});
