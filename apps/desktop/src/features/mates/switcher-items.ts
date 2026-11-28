// The quick switcher's list (docs/features/evo-mates-ui-v2/spec.md, FR-7,
// NFR-5): mates, channels, pages and actions, ranked for what the person
// typed; with nothing typed, the recent conversations first.

export interface SwitcherItem {
  /** `mate:<id>`, `channel:<id>`, `action:<id>`. */
  key: string;
  kind: "mate" | "channel" | "action";
  label: string;
  /** A mate's role, a channel's members, an action's hint. */
  detail?: string;
  /** More words that find it ("new", "add"). */
  keywords?: string[];
  /** When its conversation last changed; for the recent list. */
  at?: number;
}

/** How well `text` matches `query` (0 = not at all). */
function score(text: string, query: string): number {
  const t = text.toLowerCase();
  if (!t) return 0;
  if (t === query) return 100;
  if (t.startsWith(query)) return 80;
  if (t.split(/[\s\-_/#]+/).some((word) => word.startsWith(query))) return 60;
  if (t.includes(query)) return 40;
  // Letters in order ("nch" → "New channel").
  let at = 0;
  for (const ch of query) {
    at = t.indexOf(ch, at);
    if (at < 0) return 0;
    at++;
  }
  return 10;
}

const KIND_ORDER = { mate: 0, channel: 1, action: 2 } as const;

/**
 * The items to list for `query`, best first. With no query: the eight
 * most recent conversations, then every action.
 */
export function rankSwitcher(items: SwitcherItem[], query: string, recent = 8): SwitcherItem[] {
  const q = query.trim().toLowerCase();
  if (!q) {
    const places = items
      .filter((i) => i.kind !== "action")
      .sort((a, b) => (b.at ?? 0) - (a.at ?? 0))
      .slice(0, recent);
    return [...places, ...items.filter((i) => i.kind === "action")];
  }
  return items
    .map((item) => {
      const best = Math.max(
        score(item.label, q),
        Math.round(score(item.detail ?? "", q) * 0.5),
        ...(item.keywords ?? []).map((k) => Math.round(score(k, q) * 0.7)),
      );
      return { item, best };
    })
    .filter((x) => x.best > 0)
    .sort((a, b) => b.best - a.best || KIND_ORDER[a.item.kind] - KIND_ORDER[b.item.kind] || (b.item.at ?? 0) - (a.item.at ?? 0))
    .map((x) => x.item);
}
