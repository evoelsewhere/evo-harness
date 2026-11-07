// Run the Rust tests of the desktop shell (src-tauri).
//
//   bun run test:shell                       # every test
//   bun run test:shell agent_tools::gate     # a filter (passed to the test binary)
//
// On Windows the shell's unit-test executable links the Tauri runtime, which needs
// Common Controls v6. Tauri embeds that manifest in the app binary only, so a bare
// `cargo test` ends with STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139) before any test
// runs. This script builds the tests, puts an external manifest
// (scripts/windows-tests.manifest) beside each test executable and runs it.
// Elsewhere it is plain `cargo test`.

import { $ } from "bun";
import { copyFileSync, existsSync, readFileSync, utimesSync } from "node:fs";
import path from "node:path";

const shell = path.resolve(import.meta.dir, "../src-tauri");
const args = process.argv.slice(2);

if (process.platform !== "win32") {
  const result = await $`cargo test ${args}`.cwd(shell).nothrow();
  process.exit(result.exitCode);
}

const build = await $`cargo test --no-run --message-format=json`.cwd(shell).nothrow().quiet();
if (build.exitCode !== 0) {
  process.stderr.write(build.stderr);
  process.exit(build.exitCode);
}

// The executables cargo built for the test profile.
const executables: string[] = [];
for (const line of build.stdout.toString().split("\n")) {
  if (!line.startsWith("{")) continue;
  try {
    const message = JSON.parse(line) as { reason?: string; executable?: string | null; profile?: { test?: boolean } };
    if (message.reason === "compiler-artifact" && message.executable && message.profile?.test) executables.push(message.executable);
  } catch {
    // Not a cargo message.
  }
}
if (executables.length === 0) throw new Error("cargo built no test executable");

const manifest = path.resolve(import.meta.dir, "windows-tests.manifest");
let code = 0;
for (const executable of executables) {
  const beside = `${executable}.manifest`;
  if (!existsSync(beside) || readFileSync(beside, "utf8") !== readFileSync(manifest, "utf8")) {
    copyFileSync(manifest, beside);
    // The loader caches an executable's manifest: a new modification time makes it look again.
    const now = new Date();
    utimesSync(executable, now, now);
  }
  const run = await $`${executable} ${args}`.cwd(shell).nothrow();
  if (run.exitCode !== 0) code = run.exitCode;
}
process.exit(code);
