import { describe, expect, test } from "bun:test";
import { useMatesUiStore } from "./mates-ui-store";

// docs/features/evo-mates-messaging/spec.md (FR-7, FR-8); review 1a #5.
describe("marks follow a turn's saved entry id", () => {
  test("Delete for me and Edited move from the local id to the entry id", () => {
    const ui = useMatesUiStore.getState();
    ui.hide("c1", "local-1:reply:0");
    ui.hide("c1", "other:prompt");
    ui.markEdited("c1", "local-1:prompt");
    useMatesUiStore.getState().rekey("c1", "local-1", "entry-9");
    const s = useMatesUiStore.getState();
    expect(s.hidden.c1).toEqual(["entry-9:reply:0", "other:prompt"]);
    expect(s.edited.c1).toEqual(["entry-9:prompt"]);
  });
});
