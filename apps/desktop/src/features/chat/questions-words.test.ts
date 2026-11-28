import { describe, expect, test } from "bun:test";
import { questionWords } from "./questions-panel";

// A question card names who asks: the agent in Evo Harness, the mate in Evo
// Mates (docs/features/evo-mates/spec.md, NFR-7; qa-2026-11-28, A2).
describe("who a question card says is asking", () => {
  test("Evo Harness keeps the agent", () => {
    expect(questionWords(3)).toEqual({ heading: "The agent has 3 questions", skip: "Skip — let the agent decide", label: "Questions from the agent" });
  });

  test("a mate's card names the mate", () => {
    expect(questionWords(1, "Mai")).toEqual({ heading: "Mai has a question", skip: "Skip — let Mai decide", label: "Questions from Mai" });
  });
});
