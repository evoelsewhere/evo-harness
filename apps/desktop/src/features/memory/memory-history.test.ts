import { describe, expect, test } from "bun:test";
import { whoChanged } from "./memory-history";

// Who changed a memory: in Evo Mates the mate by name, never "the agent"
// (evo-mates NFR-7; qa-2026-11-28, B8).
describe("who changed a memory", () => {
  test("Evo Harness says the agent", () => {
    expect(whoChanged("agent")).toBe("the agent");
    expect(whoChanged("person")).toBe("you");
  });
  test("a mate's memory names the mate", () => {
    expect(whoChanged("agent", "Mai")).toBe("Mai");
    expect(whoChanged("dream", "Mai")).toBe("the dream");
    expect(whoChanged(undefined, "Mai")).toBeUndefined();
  });
});
