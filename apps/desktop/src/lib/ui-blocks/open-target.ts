// Where an `open` action may go (docs/features/rich-replies/spec.md, FR-22,
// NFR-1): a file inside the chat's folder, or an https link. Anything else is
// refused. The target comes from a model's reply, so it is untrusted.

export type OpenTarget =
  | { kind: "file"; path: string; line?: number }
  | { kind: "link"; url: string }
  | { kind: "refused"; reason: string };

const DRIVE = /^[a-z]:/i;
const SCHEME = /^[a-z][a-z0-9+.-]+:/i;

function split(path: string): string[] {
  return path.split(/[\\/]+/).filter(Boolean);
}

/** Resolves an `open` target against the chat's folder `root`. */
export function resolveOpenTarget(target: string, root: string | undefined): OpenTarget {
  const value = target.trim();
  if (!value) return { kind: "refused", reason: "The button has nothing to open." };

  if (SCHEME.test(value) && !DRIVE.test(value)) {
    let url: URL;
    try {
      url = new URL(value);
    } catch {
      return { kind: "refused", reason: "The link is not a valid address." };
    }
    if (url.protocol !== "https:") return { kind: "refused", reason: "Only https links open from a reply." };
    return { kind: "link", url: url.href };
  }
  if (value.startsWith("~")) return { kind: "refused", reason: "Only files in this chat's folder open from a reply." };
  if (!root) return { kind: "refused", reason: "This chat has no folder to open files from." };

  // `path:line` or `path:line:column`.
  const match = /^(.*?):(\d+)(?::\d+)?$/.exec(value);
  const path = match ? match[1] : value;
  const line = match ? Number(match[2]) : undefined;

  const windows = DRIVE.test(root) || root.includes("\\");
  const sep = windows ? "\\" : "/";
  const rootParts = split(root);
  const absolute = DRIVE.test(path) || path.startsWith("/") || path.startsWith("\\");
  const parts = absolute ? split(path) : [...rootParts, ...split(path)];
  const resolved: string[] = [];
  for (const part of parts) {
    if (part === ".") continue;
    if (part === "..") resolved.pop();
    else resolved.push(part);
  }

  const same = (a: string, b: string) => (windows ? a.toLowerCase() === b.toLowerCase() : a === b);
  const inside = rootParts.length <= resolved.length && rootParts.every((part, index) => same(part, resolved[index]));
  if (!inside) return { kind: "refused", reason: "Only files in this chat's folder open from a reply." };

  const leading = !DRIVE.test(root) && /^[\\/]/.test(root) ? sep : "";
  return { kind: "file", path: leading + resolved.join(sep), line };
}
