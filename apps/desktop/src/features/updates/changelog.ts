// The release notes as the new-version dialog lays them out
// (docs/features/app-updates/spec.md, FR-16). The release pipeline writes
// them from the Conventional Commit subjects (.github/scripts/release_notes.py):
//
//     ## What's changed
//     ### Features
//     - **scope:** Text ([abc1234](https://github.com/…/commit/…))
//     **Full Changelog**: https://github.com/…/compare/v4.3.0...v4.3.1
//
// Notes in any other shape are not parsed: the dialog shows them as Markdown.

export type ChangeKind = "breaking" | "features" | "fixes" | "performance" | "other";

export interface ChangeEntry {
  scope?: string;
  text: string;
  commit?: { sha: string; url: string };
}

export interface ChangeSection {
  title: string;
  kind: ChangeKind;
  entries: ChangeEntry[];
}

export interface Changelog {
  sections: ChangeSection[];
  /** "No user-facing changes" and other lines outside a section. */
  summary: string[];
  compareUrl?: string;
}

const HEADING = /^#{2,4}\s+(.+?)\s*#*$/;
const ITEM = /^[-*]\s+(.+)$/;
const SCOPE = /^\*\*([^*]+?):\*\*\s+(.+)$/;
const COMMIT = /\s+\(\[([0-9a-f]{7,40})\]\((https?:\/\/[^)\s]+)\)\)$/i;
const COMPARE = /^\*\*Full Changelog\*\*:\s*(https?:\/\/\S+)$/i;

export function changeKind(title: string): ChangeKind {
  const t = title.toLowerCase();
  if (t.includes("breaking")) return "breaking";
  if (t.startsWith("feat")) return "features";
  if (t.startsWith("fix") || t.startsWith("bug")) return "fixes";
  if (t.startsWith("perf")) return "performance";
  return "other";
}

function entry(line: string): ChangeEntry {
  let text = line.trim();
  let commit: ChangeEntry["commit"];
  const linked = COMMIT.exec(text);
  if (linked) {
    commit = { sha: linked[1].slice(0, 7), url: linked[2] };
    text = text.slice(0, linked.index).trim();
  }
  const scoped = SCOPE.exec(text);
  return scoped ? { scope: scoped[1].trim(), text: scoped[2].trim(), commit } : { text, commit };
}

/** The notes in sections, or `null` when they do not follow the release format. */
export function parseChangelog(markdown: string): Changelog | null {
  const sections: ChangeSection[] = [];
  const summary: string[] = [];
  let compareUrl: string | undefined;
  let current: ChangeSection | null = null;

  for (const raw of markdown.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line) continue;
    const compare = COMPARE.exec(line);
    if (compare) {
      compareUrl = compare[1];
      continue;
    }
    const heading = HEADING.exec(line);
    if (heading) {
      // The overall heading; the dialog has its own title.
      if (/^what'?s (changed|new)$/i.test(heading[1])) {
        current = null;
        continue;
      }
      current = {
        title: heading[1],
        kind: changeKind(heading[1]),
        entries: [],
      };
      sections.push(current);
      continue;
    }
    const item = ITEM.exec(line);
    if (item && current) {
      current.entries.push(entry(item[1]));
      continue;
    }
    // A wrapped item continues the previous entry.
    if (current && current.entries.length > 0 && /^\s/.test(raw)) {
      const last = current.entries[current.entries.length - 1];
      last.text = `${last.text} ${line}`;
      continue;
    }
    if (current) return null;
    summary.push(line);
  }

  const filled = sections.filter((section) => section.entries.length > 0);
  if (filled.length === 0 && summary.length === 0) return null;
  if (filled.length === 0 && summary.some((line) => ITEM.test(line) || line.startsWith("#"))) return null;
  return { sections: filled, summary, compareUrl };
}

/** "3 features · 5 fixes": the counts under the dialog's title. */
export function changeCounts(changelog: Changelog): string {
  const words: Record<ChangeKind, [string, string]> = {
    breaking: ["breaking change", "breaking changes"],
    features: ["feature", "features"],
    fixes: ["fix", "fixes"],
    performance: ["performance change", "performance changes"],
    other: ["other change", "other changes"],
  };
  const totals = new Map<ChangeKind, number>();
  for (const section of changelog.sections) {
    totals.set(section.kind, (totals.get(section.kind) ?? 0) + section.entries.length);
  }
  return [...totals].map(([kind, count]) => `${count} ${words[kind][count === 1 ? 0 : 1]}`).join(" · ");
}
