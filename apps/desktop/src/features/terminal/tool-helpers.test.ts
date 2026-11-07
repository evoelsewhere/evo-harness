// Run with `bun test` from apps/desktop. Spec: docs/features/agent-terminal (FR-5, FR-7, FR-14).
import { describe, expect, test } from "bun:test";
import type { ShellInfo } from "./pty";
import { isSecretPrompt, keyBytes, keyLabel, matchShell } from "./tool-helpers";

describe("keyBytes", () => {
  test("named keys and ctrl letters", () => {
    expect(keyBytes("Enter")).toBe("\r");
    expect(keyBytes("ctrl+c")).toBe("\x03");
    expect(keyBytes("Ctrl+Z")).toBe("\x1a");
    expect(keyBytes("pageup")).toBe("\x1b[5~");
    expect(keyBytes("up")).toBe("\x1b[A");
    expect(keyBytes("f13")).toBeNull();
    expect(keyLabel("ctrl+c")).toBe("Ctrl+C");
  });
});

describe("isSecretPrompt", () => {
  test("password prompts", () => {
    expect(isSecretPrompt("[sudo] password for kim: ")).toBe(true);
    expect(isSecretPrompt("Enter passphrase for key '/home/k/.ssh/id':")).toBe(true);
    expect(isSecretPrompt("user@host's password: ")).toBe(true);
    expect(isSecretPrompt("$ npm run dev")).toBe(false);
    expect(isSecretPrompt("Password reset email sent.")).toBe(false);
  });
});

describe("matchShell", () => {
  const shell = (id: string, name: string, program: string): ShellInfo => ({ id, name, program, kind: "posix", isDefault: false });
  const shells = [
    shell("pwsh", "PowerShell", "C:/Program Files/PowerShell/7/pwsh.exe"),
    shell("git-bash", "Git Bash", String.raw`C:\Program Files\Git\bin\bash.exe`),
    shell("wsl:Ubuntu", "WSL: Ubuntu", "C:/Windows/System32/wsl.exe"),
  ];
  test("by id, name and program", () => {
    expect(matchShell(shells, "git-bash")?.id).toBe("git-bash");
    expect(matchShell(shells, "Git Bash")?.id).toBe("git-bash");
    expect(matchShell(shells, "bash.exe")?.id).toBe("git-bash");
    expect(matchShell(shells, "WSL:Ubuntu".replace(":", ":"))?.id).toBe("wsl:Ubuntu");
    expect(matchShell(shells, "tcsh")).toBeUndefined();
  });
});
