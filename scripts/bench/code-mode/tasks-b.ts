// Set B of the code mode benchmark: multi-step tasks, each needing several
// dependent steps (change, then run a check, fix what it finds, write a
// report). Set A (`tasks.ts`) is mostly one shell one-liner per task, so with
// code mode off it already runs near the two-turn floor (one call, one
// answer) and cannot show what code mode is for.
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { HANDLERS, writeProject, type Task } from "./tasks";

const N = HANDLERS.length;

const files = (dir: string, root: string, ext: string): string[] =>
  existsSync(join(dir, root))
    ? readdirSync(join(dir, root), { recursive: true, encoding: "utf8" })
        .filter((name) => name.endsWith(ext))
        .map((name) => join(root, name).replace(/\\/g, "/"))
    : [];
const read = (dir: string, path: string) => (existsSync(join(dir, path)) ? readFileSync(join(dir, path), "utf8") : "");
const handlerText = (dir: string) =>
  files(dir, "src/handlers", ".js")
    .filter((f) => !f.endsWith("utils.js") && !f.endsWith("legacy.js"))
    .map((f) => read(dir, f));

/** The sample project of set A plus the check scripts set B runs. */
export function writeProjectB(dir: string): void {
  writeProject(dir);
  const put = (path: string, text: string) => {
    mkdirSync(dirname(join(dir, path)), { recursive: true });
    writeFileSync(join(dir, path), text);
  };
  put(
    "scripts/verify-rename.sh",
    '#!/bin/sh\nif grep -rq retryPolicy src/handlers; then echo "FAIL: retryPolicy is still used"; exit 1; fi\necho "OK: renamed"\n',
  );
  put(
    "scripts/check-license.sh",
    '#!/bin/sh\nfor f in $(find src -name "*.js"); do head -1 "$f" | grep -q "SPDX-License-Identifier: MIT" || { echo "MISSING $f"; bad=1; }; done\n[ -z "$bad" ] && echo "OK: all files have the header"\nexit ${bad:-0}\n',
  );
  put(
    "scripts/check-imports.sh",
    '#!/bin/sh\nif grep -rq \'require("./util")\' src/handlers; then echo "FAIL: ./util is gone"; exit 1; fi\necho "OK: imports"\n',
  );
  put("CHANGELOG.md", "# Changelog\n");
}

export const TASKS_B: Task[] = [
  {
    id: "rename-and-verify",
    prompt: "Rename retryPolicy to retryOptions in every file under src/handlers, then run scripts/verify-rename.sh and tell me its output.",
    check: (dir, answer) => handlerText(dir).every((t) => !t.includes("retryPolicy") && t.includes("retryOptions")) && /OK: renamed/.test(answer),
  },
  {
    id: "stub-broken-links",
    prompt: "Find the links in the project's Markdown files that point to local files that do not exist, and create each missing file with a single line `# TODO`. Then tell me which files you created.",
    check: (dir) =>
      ["docs/api.md", "docs/faq.md", "CHANGES.md", "docs/deploy/README.md", "LICENSE"].every((path) => read(dir, path).trim() === "# TODO"),
  },
  {
    id: "todo-report",
    prompt: "Write TODO.md with one line per file under src/handlers that has TODO comments, in the form `<file name>: <count>`, sorted by file name. Then tell me the total.",
    check: (dir, answer) => {
      const lines = read(dir, "TODO.md").split("\n").filter((l) => /:\s*\d+/.test(l));
      return lines.length === N && new RegExp(`\\b${N + Math.ceil(N / 2)}\\b`).test(answer);
    },
  },
  {
    id: "license-and-check",
    prompt: "Add `// SPDX-License-Identifier: MIT` as the first line of every .js file under src that lacks it, then run scripts/check-license.sh and tell me its result.",
    check: (dir, answer) =>
      files(dir, "src", ".js").every((f) => read(dir, f).startsWith("// SPDX-License-Identifier: MIT\n")) && /OK: all files/.test(answer),
  },
  {
    id: "largest-report",
    prompt: "Write reports/largest.txt listing the five .js files under src with the most lines, one per line as `<file name> <lines>`, largest first.",
    check: (dir) => {
      const lines = read(dir, "reports/largest.txt").trim().split("\n");
      return lines.length === 5 && lines[0].includes("tax4.js") && lines[1].includes("prices4.js");
    },
  },
  {
    id: "imports-and-check",
    prompt: "src/handlers/util.js was renamed to utils.js. Fix every require of ./util, then run scripts/check-imports.sh and tell me its output.",
    check: (dir, answer) => handlerText(dir).every((t) => t.includes('require("./utils")')) && /OK: imports/.test(answer),
  },
  {
    id: "bump-and-changelog",
    prompt: "Bump every package under packages/ from 1.2.3 to 1.2.4, and add one line per package to CHANGELOG.md in the form `- <package name> 1.2.4`.",
    check: (dir) => {
      const versions = files(dir, "packages", "package.json").every((f) => JSON.parse(read(dir, f)).version === "1.2.4");
      const log = read(dir, "CHANGELOG.md");
      return versions && ["core", "cli", "web", "shared"].every((name) => log.includes(`@sample/${name} 1.2.4`));
    },
  },
  {
    id: "align-configs",
    prompt: "Make every file under config/ have the same keys: add each key a file is missing with the value null. Keep existing values.",
    check: (dir) => {
      const configs = files(dir, "config", ".json").map((f) => JSON.parse(read(dir, f)));
      const keys = ["port", "host", "debug", "logLevel", "sentry"];
      return configs.length === 3 && configs.every((c) => keys.every((k) => k in c)) && configs[0].port !== undefined;
    },
  },
  {
    id: "fix-failing-check",
    prompt: "Run scripts/check-a.sh, scripts/check-b.sh and scripts/check-c.sh. Make the failing one exit 0 instead (keep its echo line), run all three again, and tell me the results.",
    check: (dir) => /exit 0/.test(read(dir, "scripts/check-b.sh")) && /check b/.test(read(dir, "scripts/check-b.sh")),
  },
  {
    id: "exports-and-index",
    prompt: "In every file under src/handlers that has `export default function (input)`, change it to `export function handler(input)`. Then write src/index.js that re-exports each one as `export { handler as <file name without .js>Handler } from \"./handlers/<file>\";`, one line per file.",
    check: (dir) => {
      const index = read(dir, "src/index.js");
      return handlerText(dir).every((t) => t.includes("export function handler(input)")) && (index.match(/Handler } from/g) ?? []).length === N;
    },
  },
  {
    id: "md-lines-json",
    prompt: "Write reports/md-lines.json: an object mapping each Markdown file's path (relative to the project root) to its number of lines.",
    check: (dir) => {
      try {
        const data = JSON.parse(read(dir, "reports/md-lines.json"));
        const keys = Object.keys(data).map((k) => k.replace(/\\/g, "/").replace(/^\.\//, ""));
        return ["README.md", "docs/guide.md", "docs/setup.md"].every((k) => keys.includes(k));
      } catch {
        return false;
      }
    },
  },
  {
    id: "attempts-by-index",
    prompt: "Every handler under src/handlers sets `maxAttempts`. Set it to 3 in every handler whose current value is above 10, then tell me how many files you changed.",
    check: (dir, answer) => {
      const values = handlerText(dir).map((t) => Number(/maxAttempts: (\d+)/.exec(t)?.[1] ?? -1));
      return values.every((v) => v <= 10) && values.filter((v) => v === 3).length === 1 + (N - 9) && new RegExp(`\\b${N - 9}\\b`).test(answer);
    },
  },
];
