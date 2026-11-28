import { describe, expect, test } from "bun:test";
import type { FileStat, SearchHit } from "@/features/files/fs";
import { locate, type FileLookup } from "./open-file-ref";

// docs/features/rich-replies/spec.md (FR-37): a file a reply names that is
// not there says "File not found". Found checking the file links in Evo
// Mates (qa-2026-11-28, H5): `fs_stat` answers `{ exists: false }` for a
// missing file, and every answer counted as found.
const files = new Set(["/root/notes.md", "/root/sub/deep.md"]);
const fs: FileLookup = {
  statFile: async (path: string) => ({ exists: files.has(path), dir: false, size: 0, mtime: 0 }) as FileStat,
  searchFiles: async (_root: string, query: string) =>
    [...files].filter((path) => path.endsWith(`/${query}`)).map((path) => ({ path }) as SearchHit),
};

describe("finding a file a reply names", () => {
  test("a missing file is not found", async () => {
    expect(await locate("missing.md", "/root", fs)).toBeNull();
  });

  test("a file there is found, with its line", async () => {
    expect(await locate("notes.md:12", "/root", fs)).toEqual({ path: "/root/notes.md", line: 12 });
  });

  test("a bare name is found in a folder below", async () => {
    expect(await locate("deep.md", "/root", fs)).toEqual({ path: "/root/sub/deep.md", line: undefined });
  });
});
