// Evo's own mate faces: open-bot-avatar definitions, a silhouette in a
// 0 0 100 100 box and the eyes' place (docs/features/evo-mates/ux.md 6;
// spec FR-33). Drawn for Evo; no other product's characters.

import { validateAvatar, type AvatarDefinition } from "open-bot-avatar";

const raw: AvatarDefinition[] = [
  {
    version: 1,
    id: "pebble",
    name: "Pebble",
    body: { path: "M50 14 C74 14 88 32 88 54 C88 76 72 88 50 88 C28 88 12 76 12 54 C12 32 26 14 50 14 Z" },
    eyes: { x: 50, y: 48, spacing: 16, scale: 1 },
  },
  {
    version: 1,
    id: "scout",
    name: "Scout",
    body: { path: "M50 10 C62 26 84 40 84 60 C84 78 69 90 50 90 C31 90 16 78 16 60 C16 40 38 26 50 10 Z" },
    eyes: { x: 50, y: 58, spacing: 15, scale: 1 },
  },
  {
    version: 1,
    id: "quill",
    name: "Quill",
    body: { path: "M34 12 H66 Q80 12 80 26 V76 Q80 90 66 90 H34 Q20 90 20 76 V26 Q20 12 34 12 Z" },
    eyes: { x: 50, y: 42, spacing: 16, scale: 1 },
  },
  {
    version: 1,
    id: "gear",
    name: "Gear",
    body: { path: "M28 16 H72 Q86 16 86 30 V70 Q86 84 72 84 H28 Q14 84 14 70 V30 Q14 16 28 16 Z" },
    eyes: { x: 50, y: 48, spacing: 18, scale: 1 },
  },
  {
    version: 1,
    id: "prism",
    name: "Prism",
    body: { path: "M50 10 Q54 10 58 12.5 L80 25 Q84 27.5 84 32 V68 Q84 72.5 80 75 L58 87.5 Q54 90 50 90 Q46 90 42 87.5 L20 75 Q16 72.5 16 68 V32 Q16 27.5 20 25 L42 12.5 Q46 10 50 10 Z" },
    eyes: { x: 50, y: 48, spacing: 16, scale: 1 },
  },
  {
    version: 1,
    id: "compass",
    name: "Compass",
    body: { path: "M50 12 C71 12 88 29 88 50 C88 71 71 88 50 88 C29 88 12 71 12 50 C12 29 29 12 50 12 Z" },
    eyes: { x: 50, y: 46, spacing: 16, scale: 1 },
  },
  {
    version: 1,
    id: "spark",
    name: "Spark",
    body: { path: "M22 34 C22 20 36 16 50 22 C64 16 80 20 80 36 C88 48 86 72 70 80 C58 88 42 88 30 80 C14 72 12 48 22 34 Z" },
    eyes: { x: 50, y: 50, spacing: 17, scale: 1 },
  },
  {
    version: 1,
    id: "buoy",
    name: "Buoy",
    body: { path: "M34 26 H66 C79 26 90 37 90 50 C90 63 79 74 66 74 H34 C21 74 10 63 10 50 C10 37 21 26 34 26 Z" },
    eyes: { x: 50, y: 48, spacing: 18, scale: 1 },
  },
];

/** Every face, validated once; an invalid one is left out. */
export const faces: AvatarDefinition[] = raw.flatMap((definition) => {
  const result = validateAvatar(definition);
  return result.ok ? [result.value] : [];
});

export const DEFAULT_FACE = "pebble";

export function faceById(id: string | undefined | null): AvatarDefinition {
  return faces.find((face) => face.id === id) ?? faces.find((face) => face.id === DEFAULT_FACE) ?? faces[0]!;
}
