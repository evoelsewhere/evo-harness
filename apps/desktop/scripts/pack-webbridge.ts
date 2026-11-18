// Pack the Evo Webbridge extension for the Chrome Web Store and Edge Add-ons
// (docs/features/browser-bridge/tasks.md T33).
//
//   bun scripts/pack-webbridge.ts            # dist/evo-webbridge-<version>.zip
//
// The stores give the extension an id of their own, so the packed manifest
// leaves out `key` (which pins the id of the unpacked folder). Once a listing
// exists, its id goes into STORE_IDS in src-tauri/src/webbridge/ids.rs so the
// host lets it in.

import { $ } from "bun";
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";

const root = path.resolve(import.meta.dir, "../../..");
const source = path.join(root, "extensions", "evo-webbridge");
const dist = path.join(import.meta.dir, "..", "dist");
const stage = path.join(dist, "evo-webbridge");

const manifest = JSON.parse(readFileSync(path.join(source, "manifest.json"), "utf8"));
const version: string = manifest.version;
delete manifest.key;

rmSync(stage, { recursive: true, force: true });
mkdirSync(dist, { recursive: true });
cpSync(source, stage, { recursive: true });
writeFileSync(path.join(stage, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);

const zip = path.join(dist, `evo-webbridge-${version}.zip`);
rmSync(zip, { force: true });
// bsdtar (Windows 10+ in System32, macOS) writes zip files with -a; elsewhere use zip.
const tar = process.platform === "win32" ? path.join(process.env.SystemRoot ?? "C:\\Windows", "System32", "tar.exe") : "tar";
const packed = await $`${tar} -a -c -f ${zip} -C ${stage} .`.nothrow().quiet();
if (packed.exitCode !== 0) await $`zip -r -q ${zip} .`.cwd(stage);
rmSync(stage, { recursive: true, force: true });
console.log(`packed ${path.relative(process.cwd(), zip)} (version ${version}, no key)`);
