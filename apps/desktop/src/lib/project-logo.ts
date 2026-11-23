// Where a project usually keeps its logo, so the workspace rail can show it
// (docs/features/workspace-rail/spec.md, FR-13, A-7). Paths only: reading
// is done by the icon store with the shell's file commands.

/** Logos larger than this are not read. */
export const LOGO_MAX_BYTES = 1024 * 1024;

/** Folders whose sub-folders are projects of their own (monorepos). */
export const NESTED_ROOTS = ["apps", "packages"] as const;

/** Usual places, relative to a project folder, best first. */
const PLACES = [
  "icon.png",
  "icon.svg",
  "logo.svg",
  "logo.png",
  "favicon.svg",
  "favicon.png",
  "favicon.ico",
  "public/favicon.svg",
  "public/favicon.png",
  "public/favicon.ico",
  "public/logo.svg",
  "public/logo.png",
  "public/icon.png",
  "static/favicon.svg",
  "static/favicon.png",
  "static/favicon.ico",
  "assets/logo.svg",
  "assets/logo.png",
  "assets/icon.png",
  ".github/logo.svg",
  ".github/logo.png",
  "docs/logo.svg",
  "docs/logo.png",
  "src-tauri/icons/icon.png",
  "app/favicon.ico",
  "src/app/favicon.ico",
];

/**
 * Candidate logo paths for the project at `root`: its own places, then those
 * of each nested project `nested` (names of folders under `nestedRoot`).
 */
export function logoCandidates(root: string, nested: readonly string[], nestedRoot: string = NESTED_ROOTS[0]): string[] {
  const sep = root.includes("\\") ? "\\" : "/";
  const base = root.replace(/[\\/]+$/, "");
  const join = (...parts: string[]) => [base, ...parts].join(sep).replace(/[\\/]/g, sep);
  return [...PLACES.map((place) => join(place)), ...nested.flatMap((name) => PLACES.map((place) => join(nestedRoot, name, place)))];
}

const MIME: Record<string, string> = {
  png: "image/png",
  svg: "image/svg+xml",
  ico: "image/x-icon",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  webp: "image/webp",
};

/** The image type of a logo file, or `null` for a file that is not one we show. */
export function logoMime(path: string): string | null {
  const ext = /\.([a-z0-9]+)$/i.exec(path)?.[1]?.toLowerCase();
  return (ext && MIME[ext]) || null;
}
