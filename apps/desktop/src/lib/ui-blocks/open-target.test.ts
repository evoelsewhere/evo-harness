// docs/features/rich-replies/spec.md (FR-22, NFR-1).
import { describe, expect, test } from "bun:test";
import { resolveOpenTarget } from "./open-target";

const win = "C:\\Users\\me\\project";
const posix = "/home/me/project";

describe("an open action's target", () => {
  test("a relative file inside the chat's folder, with a line", () => {
    expect(resolveOpenTarget("crates/evo-engine/src/lib.rs:12", win)).toEqual({
      kind: "file",
      path: "C:\\Users\\me\\project\\crates\\evo-engine\\src\\lib.rs",
      line: 12,
    });
    expect(resolveOpenTarget("./src/a.ts", posix)).toEqual({ kind: "file", path: "/home/me/project/src/a.ts", line: undefined });
  });

  test("an absolute path inside the folder, any case on Windows", () => {
    expect(resolveOpenTarget("c:/users/me/PROJECT/readme.md:3:7", win)).toMatchObject({ kind: "file", line: 3 });
  });

  test("paths that leave the folder are refused", () => {
    expect(resolveOpenTarget("../secrets.txt", win).kind).toBe("refused");
    expect(resolveOpenTarget("src/../../other/a.ts", posix).kind).toBe("refused");
    expect(resolveOpenTarget("C:\\Windows\\win.ini", win).kind).toBe("refused");
    expect(resolveOpenTarget("/etc/passwd", posix).kind).toBe("refused");
    expect(resolveOpenTarget("~/.ssh/id_rsa", posix).kind).toBe("refused");
    expect(resolveOpenTarget("/home/me/project-other/a.ts", posix).kind).toBe("refused");
  });

  test("https links open; other schemes do not", () => {
    expect(resolveOpenTarget("https://example.com/a?b=1", win)).toEqual({ kind: "link", url: "https://example.com/a?b=1" });
    expect(resolveOpenTarget("http://example.com", win).kind).toBe("refused");
    expect(resolveOpenTarget("file:///C:/Windows/win.ini", win).kind).toBe("refused");
    expect(resolveOpenTarget("javascript:alert(1)", win).kind).toBe("refused");
  });

  test("without a folder, files are refused but links still open", () => {
    expect(resolveOpenTarget("a.ts", undefined).kind).toBe("refused");
    expect(resolveOpenTarget("https://example.com", undefined).kind).toBe("link");
  });

  test("an empty target is refused", () => {
    expect(resolveOpenTarget("  ", posix).kind).toBe("refused");
  });
});
