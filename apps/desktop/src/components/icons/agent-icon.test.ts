import { describe, expect, test } from "bun:test";
import { faceById } from "@/components/mate-face/faces";
import { agentColor, agentFace, agentFaceId, agentFaceSvg, agentHue, oklchToHex } from "./agent-icon";

// docs/features/subagents-and-nested-sessions/spec.md, FR-25: agents wear the
// Evo Mates faces (docs/features/evo-mates/ux.md 6).
describe("agent faces", () => {
  test("the built-in agents have a face each", () => {
    expect(agentFaceId("general")).toBe("pebble");
    expect(agentFaceId("general-purpose")).toBe("pebble");
    expect(agentFaceId(undefined)).toBe("pebble");
    expect(agentFaceId("explore")).toBe("scout");
    expect(agentFaceId("plan")).toBe("quill");
    expect(agentFaceId("explore", true)).toBe("buoy");
  });

  test("any other agent gets a face of its own from its name, the same every time", () => {
    const face = agentFaceId("reviewer");
    expect(["gear", "prism", "compass", "spark"]).toContain(face);
    expect(agentFaceId("reviewer")).toBe(face);
    expect(agentHue("reviewer")).toBe(agentHue("reviewer"));
    const many = new Set(["reviewer", "tester", "writer", "release", "docs", "triage", "migrator"].map((n) => agentFaceId(n)));
    expect(many.size).toBeGreaterThan(1);
  });

  test("an icon's face is the mate face with larger eyes", () => {
    const face = agentFace("plan");
    const mate = faceById("quill");
    expect(face.body.path).toBe(mate.body.path);
    expect(face.eyes.scale).toBeGreaterThan(mate.eyes.scale);
  });

  test("the colour is the agent's hue at the tint's lightness, per theme", () => {
    expect(oklchToHex(0.627955, 0.257683, 29.2339)).toBe("#ff0000");
    expect(oklchToHex(1, 0, 0)).toBe("#ffffff");
    const light = agentColor("explore");
    const dark = agentColor("explore", false, true);
    expect(light).toMatch(/^#[0-9a-f]{6}$/);
    expect(dark).not.toBe(light);
  });

  test("a face at rest is a fluid SVG without the face's own name", () => {
    const svg = agentFaceSvg("general");
    expect(svg.startsWith('<svg style="width:100%;height:100%;display:block" ')).toBe(true);
    expect(svg).not.toContain("<title>");
    expect(agentFaceSvg("general")).toBe(svg);
  });
});
