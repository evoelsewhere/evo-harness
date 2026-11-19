// The `evo-ui` block format: what the model writes inside a reply and the app
// draws (docs/features/rich-replies/spec.md, FR-1, FR-15–FR-22; shapes in
// plan.md 3.3). Fields are only ever added within a format version (NFR-4).

/** The format version this app reads. */
export const FORMAT_VERSION = 1;

/** The fence info string that marks a block. */
export const BLOCK_FENCE = "evo-ui";

export const BLOCK_TYPES = ["stats", "bars", "chart", "table", "steps", "callout", "actions"] as const;
export type BlockType = (typeof BLOCK_TYPES)[number];

export const NAMED_FORMATS = ["number", "compact", "percent", "bytes", "duration", "date"] as const;
/** How the app writes a value (FR-6). `percent` takes a percentage (25 → 25%). */
export type Format = (typeof NAMED_FORMATS)[number] | { currency: string };

/** What a click does (FR-22). */
export type Action = { send: string } | { open: string };

export type Cell = string | number | null;

interface BlockBase {
  v: number;
  title?: string;
  source?: string;
}

export interface StatsBlock extends BlockBase {
  type: "stats";
  items: {
    label: string;
    value: number | string;
    format?: Format;
    delta?: number;
    deltaFormat?: Format;
    hint?: string;
  }[];
}

export interface BarsBlock extends BlockBase {
  type: "bars";
  format?: Format;
  max?: number;
  items: { label: string; value: number; action?: Action }[];
}

export const CHART_KINDS = ["line", "bar", "area"] as const;
export interface ChartBlock extends BlockBase {
  type: "chart";
  kind: (typeof CHART_KINDS)[number];
  stacked?: boolean;
  x: (string | number)[];
  format?: Format;
  series: { name: string; values: (number | null)[] }[];
}

export interface TableColumn {
  key: string;
  label: string;
  format?: Format;
  align?: "left" | "right";
}
export interface TableRow {
  cells: Record<string, Cell>;
  action?: Action;
}
export interface TableBlock extends BlockBase {
  type: "table";
  columns: TableColumn[];
  rows: TableRow[];
}

export const STEP_STATUSES = ["done", "current", "todo", "failed", "skipped"] as const;
export interface StepsBlock extends BlockBase {
  type: "steps";
  items: { title: string; detail?: string; status?: (typeof STEP_STATUSES)[number] }[];
}

export const CALLOUT_TONES = ["info", "success", "warning", "error"] as const;
export interface CalloutBlock extends BlockBase {
  type: "callout";
  tone: (typeof CALLOUT_TONES)[number];
  /** Inline Markdown only. */
  text: string;
}

export interface ActionsBlock extends BlockBase {
  type: "actions";
  items: { label: string; action: Action; primary?: boolean }[];
}

export type UiBlock = StatsBlock | BarsBlock | ChartBlock | TableBlock | StepsBlock | CalloutBlock | ActionsBlock;

/** Limits of FR-14. */
export const LIMITS = {
  bodyBytes: 32 * 1024,
  stats: 8,
  bars: 50,
  chartSeries: 6,
  chartPoints: 1000,
  tableRows: 500,
  tableColumns: 12,
  actions: 4,
} as const;
