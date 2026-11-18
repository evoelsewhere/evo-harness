// Build evo-engine and evo-bridge-host (Evo Webbridge's native messaging host)
// and stage them as Tauri sidecars.
//
//   bun scripts/prepare-sidecar.ts            # debug engine
//   bun scripts/prepare-sidecar.ts --release  # release engine
//
// Tauri's `bundle.externalBin` expects `src-tauri/binaries/evo-engine-<target-triple>[.exe]`
// and installs it next to the app executable as `evo-engine[.exe]`, which is
// where the shell looks for it (src-tauri/src/engine.rs). The host goes the
// same way; the browsers start it from beside the app (src-tauri/src/webbridge/setup.rs).

import { $ } from "bun";
import { copyFileSync, mkdirSync } from "node:fs";
import path from "node:path";

const release = process.argv.includes("--release");
const root = path.resolve(import.meta.dir, "../../..");
const desktop = path.resolve(import.meta.dir, "..");

const host = (await $`rustc -vV`.text()).match(/^host: (.+)$/m)?.[1]?.trim();
if (!host) throw new Error("could not read the host target triple from `rustc -vV`");

const profile = release ? "release" : "debug";
console.log(`building evo-engine and evo-bridge-host (${profile}) for ${host}…`);
await $`cargo build -p evo-engine -p evo-bridge-host ${release ? ["--release"] : []}`.cwd(root);

const exe = host.includes("windows") ? ".exe" : "";
const targetDir = path.join(desktop, "src-tauri", "binaries");
mkdirSync(targetDir, { recursive: true });
for (const name of ["evo-engine", "evo-bridge-host"]) {
  const source = path.join(root, "target", profile, `${name}${exe}`);
  const target = path.join(targetDir, `${name}-${host}${exe}`);
  copyFileSync(source, target);
  console.log(`staged ${path.relative(desktop, target)}`);
}
