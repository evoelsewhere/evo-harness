// docs/features/workspace-rail/spec.md (FR-13, A-7).
import { describe, expect, test } from "bun:test";
import { LOGO_MAX_BYTES, logoCandidates, logoMime, NESTED_ROOTS } from "./project-logo";

describe("logoCandidates", () => {
  test("root places first, then each nested project's", () => {
    const list = logoCandidates("C:\\code\\evo", ["desktop"]);
    expect(list[0]).toBe("C:\\code\\evo\\icon.png");
    expect(list).toContain("C:\\code\\evo\\public\\favicon.svg");
    expect(list).toContain("C:\\code\\evo\\apps\\desktop\\src-tauri\\icons\\icon.png");
    expect(list.indexOf("C:\\code\\evo\\public\\favicon.svg")).toBeLessThan(
      list.indexOf("C:\\code\\evo\\apps\\desktop\\src-tauri\\icons\\icon.png"),
    );
  });
  test("keeps the folder's own separator and drops a trailing one", () => {
    expect(logoCandidates("/home/me/site/", [])[0]).toBe("/home/me/site/icon.png");
  });
  test("nested projects are looked for under apps and packages", () => {
    expect(NESTED_ROOTS).toEqual(["apps", "packages"]);
    const list = logoCandidates("/r", ["web"], "packages");
    expect(list).toContain("/r/packages/web/public/favicon.ico");
  });
});

describe("logoMime", () => {
  test("image types by extension, anything else refused", () => {
    expect(logoMime("a/logo.PNG")).toBe("image/png");
    expect(logoMime("favicon.svg")).toBe("image/svg+xml");
    expect(logoMime("favicon.ico")).toBe("image/x-icon");
    expect(logoMime("x.jpeg")).toBe("image/jpeg");
    expect(logoMime("x.webp")).toBe("image/webp");
    expect(logoMime("logo.gif")).toBeNull();
    expect(logoMime("README")).toBeNull();
  });
  test("a 1 MB cap", () => {
    expect(LOGO_MAX_BYTES).toBe(1024 * 1024);
  });
});
