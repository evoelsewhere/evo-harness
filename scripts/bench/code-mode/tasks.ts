// The 12 tasks of the code mode benchmark (docs/features/code-mode/spec.md,
// SC-1) and the sample project they run in. Each task is the kind of
// multi-step work code mode is for: the same change across files, a search
// filtered before reading, independent commands. A check decides from the
// files and the agent's final answer whether the task was done.
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

export interface Task {
  id: string;
  prompt: string;
  check(dir: string, answer: string): boolean;
}

const files = (dir: string, root: string, ext: string): string[] =>
  existsSync(join(dir, root))
    ? readdirSync(join(dir, root), { recursive: true, encoding: "utf8" })
        .filter((name) => name.endsWith(ext))
        .map((name) => join(root, name).replace(/\\/g, "/"))
    : [];
const read = (dir: string, path: string) => readFileSync(join(dir, path), "utf8");

/** Write the sample project into `dir`. */
export function writeProject(dir: string): void {
  const put = (path: string, text: string) => {
    mkdirSync(dirname(join(dir, path)), { recursive: true });
    writeFileSync(join(dir, path), text);
  };
  const handlers = HANDLERS;
  handlers.forEach((name, i) => {
    put(
      `src/handlers/${name}.js`,
      [
        `const { request } = require("./util");`,
        `const retryPolicy = { maxAttempts: ${i + 2} };`,
        ``,
        `// TODO: validate the input`,
        `export default function (input) {`,
        `  console.log("handling ${name}", input);`,
        `  if (!input) console.error("no input");`,
        `  return request("/${name}", input, retryPolicy);`,
        `}`,
        ...(i % 2 === 0 ? [`// TODO: add paging`] : []),
        ...Array.from({ length: i * 3 }, (_, n) => `// note ${n}`),
      ].join("\n") + "\n",
    );
  });
  put("src/handlers/utils.js", `exports.request = (path, input, policy) => ({ path, input, policy });\n`);
  put("src/handlers/legacy.js", `// SPDX-License-Identifier: MIT\nconsole.log("legacy");\n`);
  for (const [name, version] of [["core", "1.2.3"], ["cli", "1.2.3"], ["web", "1.2.3"], ["shared", "1.2.3"]]) {
    put(`packages/${name}/package.json`, JSON.stringify({ name: `@sample/${name}`, version, private: true }, null, 2) + "\n");
  }
  put("config/dev.json", JSON.stringify({ port: 3000, host: "localhost", debug: true, logLevel: "debug" }, null, 2));
  put("config/test.json", JSON.stringify({ port: 3001, host: "localhost", debug: true }, null, 2));
  put("config/prod.json", JSON.stringify({ port: 80, host: "example.invalid", logLevel: "warn", sentry: "on" }, null, 2));
  put("docs/guide.md", "# Guide\n\nSee [setup](setup.md), [api](api.md) and [faq](faq.md).\n\nMore in [changes](../CHANGES.md).\n");
  put("docs/setup.md", "# Setup\n\nRun it. Back to the [guide](guide.md). See [deploy](deploy/README.md).\n");
  put("README.md", "# Sample\n\nRead the [guide](docs/guide.md) and the [license](LICENSE).\n");
  for (const name of ["a", "b", "c"]) {
    put(`scripts/check-${name}.sh`, `#!/bin/sh\necho "check ${name}"\nexit ${name === "b" ? 1 : 0}\n`);
  }
}

/** Handler files in the sample project: enough that per-file work adds up. */
const NAMES = ["users", "orders", "billing", "search", "auth", "files", "carts", "stock", "prices", "tax"];
export const HANDLERS = [1, 2, 3, 4].flatMap((n) => NAMES.map((name) => `${name}${n}`));
const N = HANDLERS.length;
const TODOS = N + Math.ceil(N / 2);
/** The three handlers with the most lines (they grow with their index). */
const LARGEST = HANDLERS.slice(-3).map((name) => `${name}.js`);

export const TASKS: Task[] = [
  {
    id: "rename-option",
    prompt: "Rename the variable retryPolicy to retryOptions in every file under src/handlers.",
    check: (dir) => {
      const all = files(dir, "src/handlers", ".js").map((f) => read(dir, f)).join("\n");
      return !all.includes("retryPolicy") && (all.match(/retryOptions/g) ?? []).length === 2 * N;
    },
  },
  {
    id: "broken-links",
    prompt: "List the links in the Markdown files of this project that point to local files that do not exist. Answer with their paths.",
    check: (_dir, answer) => ["api.md", "faq.md", "CHANGES.md", "deploy/README.md", "LICENSE"].every((path) => answer.includes(path)),
  },
  {
    id: "todo-count",
    prompt: "How many TODO comments are there under src in total? Answer with the number.",
    check: (_dir, answer) => new RegExp(`\\b${TODOS}\\b`).test(answer),
  },
  {
    id: "license-header",
    prompt: "Add the line `// SPDX-License-Identifier: MIT` as the first line of every .js file under src that does not start with it.",
    check: (dir) =>
      files(dir, "src", ".js").every((f) => {
        const text = read(dir, f);
        return text.startsWith("// SPDX-License-Identifier: MIT\n") && text.split("SPDX-License-Identifier").length === 2;
      }),
  },
  {
    id: "largest-files",
    prompt: "Which three .js files under src have the most lines? Answer with their file names.",
    check: (_dir, answer) => LARGEST.every((name) => answer.includes(name)),
  },
  {
    id: "fix-imports",
    prompt: "src/handlers/util.js was renamed to utils.js. Fix every require that still points to ./util.",
    check: (dir) => {
      const all = files(dir, "src/handlers", ".js").map((f) => read(dir, f)).join("\n");
      return !/require\("\.\/util"\)/.test(all) && (all.match(/require\("\.\/utils"\)/g) ?? []).length === N;
    },
  },
  {
    id: "version-bump",
    prompt: "Bump the version of every package under packages/ from 1.2.3 to 1.2.4.",
    check: (dir) => files(dir, "packages", "package.json").every((f) => JSON.parse(read(dir, f)).version === "1.2.4"),
  },
  {
    id: "remove-console-log",
    prompt: "Remove every console.log line under src, but keep console.error.",
    check: (dir) => {
      const all = files(dir, "src", ".js").map((f) => read(dir, f)).join("\n");
      return !all.includes("console.log") && (all.match(/console\.error/g) ?? []).length === N;
    },
  },
  {
    id: "config-keys",
    prompt: "Which keys appear in some of the files under config/ but not in all of them? Answer with the key names.",
    check: (_dir, answer) => ["debug", "logLevel", "sentry"].every((key) => answer.includes(key)),
  },
  {
    id: "check-scripts",
    prompt: "Run the three scripts scripts/check-a.sh, scripts/check-b.sh and scripts/check-c.sh and tell me which ones fail.",
    check: (_dir, answer) => /check-b/.test(answer) && !/check-a\.sh[^.]*fail/i.test(answer),
  },
  {
    id: "named-exports",
    prompt: "In every file under src/handlers, change `export default function (input)` to `export function handler(input)`.",
    check: (dir) => {
      const all = files(dir, "src/handlers", ".js").map((f) => read(dir, f)).join("\n");
      return !all.includes("export default") && (all.match(/export function handler\(input\)/g) ?? []).length === N;
    },
  },
  {
    id: "markdown-lines",
    prompt: "How many lines do all the Markdown files of this project have together? Answer with the number.",
    check: (_dir, answer) => /\b11\b/.test(answer),
  },
];
