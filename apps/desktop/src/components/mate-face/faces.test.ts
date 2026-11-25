import { describe, expect, test } from "bun:test";
import { faceById, faces } from "./faces";
import { faceColors, faceStateOf } from "./mate-face";

// docs/features/evo-mates/spec.md (FR-33).
describe("mate faces", () => {
  test("every face definition is valid for open-bot-avatar", () => {
    expect(faces.map((face) => face.id)).toEqual(["pebble", "scout", "quill", "gear", "prism", "compass", "spark", "buoy"]);
  });

  test("an unknown face falls back to the default", () => {
    expect(faceById("nobody").id).toBe("pebble");
  });

  test("states map to the engine's states (ux.md 6)", () => {
    expect(faceStateOf(undefined)).toBe("idle");
    expect(faceStateOf("working")).toBe("working");
    expect(faceStateOf("waiting")).toBe("waiting");
    expect(faceStateOf("error")).toBe("blocked");
  });

  test("a hue gives a body colour and a plate per theme", () => {
    const light = faceColors(210, false);
    const dark = faceColors(210, true);
    expect(light.body).toMatch(/^#[0-9a-f]{6}$/);
    expect(light.plate).not.toBe(dark.plate);
  });
});
