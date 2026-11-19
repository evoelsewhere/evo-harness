// `evo-ui` blocks in the side panel, as text (docs/features/rich-replies/spec.md,
// FR-28, FR-4). The side panel does not draw blocks; it shows the same text
// the desktop copies. A port of apps/desktop/src/lib/ui-blocks/{format,text}.ts
// for the stage-1 components: keep the two in step.

const TYPES = ["stats", "bars", "chart", "table", "steps", "callout", "actions"];
const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB", "PB"];
const isNumber = (value) => typeof value === "number" && Number.isFinite(value);
const nf = (options) => new Intl.NumberFormat(undefined, options);

function duration(seconds) {
  const total = Math.abs(seconds);
  if (total < 1) return `${Math.round(total * 1000)}ms`;
  if (total < 60) return `${nf({ maximumFractionDigits: 1 }).format(total)}s`;
  const whole = Math.round(total);
  const hours = Math.floor(whole / 3600);
  const minutes = Math.floor((whole % 3600) / 60);
  return hours > 0 ? `${hours}h ${minutes}m` : `${minutes}m ${whole % 60}s`;
}

function format(value, how, signed = false) {
  if (value === null || value === undefined || value === "") return "—";
  if (how === "date") {
    const day = typeof value === "string" && /^\d{4}-\d{2}-\d{2}$/.test(value);
    const time = typeof value === "number" ? value : Date.parse(value);
    return Number.isFinite(time)
      ? new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeZone: day ? "UTC" : undefined }).format(time)
      : String(value);
  }
  if (!isNumber(value)) return String(value);
  const signDisplay = signed ? "exceptZero" : "auto";
  const prefix = value < 0 ? "-" : signed && value > 0 ? "+" : "";
  if (how && typeof how === "object" && typeof how.currency === "string") {
    try {
      return nf({ style: "currency", currency: how.currency, signDisplay }).format(value);
    } catch {
      return `${how.currency} ${nf({ minimumFractionDigits: 2, maximumFractionDigits: 2, signDisplay }).format(value)}`;
    }
  }
  if (how === "compact") return nf({ notation: "compact", maximumFractionDigits: 2, signDisplay }).format(value);
  if (how === "percent") return nf({ style: "percent", maximumFractionDigits: 1, signDisplay }).format(value / 100);
  if (how === "duration") return prefix + duration(value);
  if (how === "bytes") {
    let size = Math.abs(value);
    let unit = 0;
    while (size >= 1024 && unit < BYTE_UNITS.length - 1) {
      size /= 1024;
      unit++;
    }
    return `${prefix}${nf({ maximumFractionDigits: unit ? 1 : 0 }).format(size)} ${BYTE_UNITS[unit]}`;
  }
  return nf({ maximumFractionDigits: 2, signDisplay }).format(value);
}

const list = (value) => (Array.isArray(value) ? value.filter((item) => item && typeof item === "object") : []);

function body(block) {
  switch (block.type) {
    case "stats":
      return list(block.items).map((item) => {
        let line = `${item.label}: ${format(item.value, item.format)}`;
        if (isNumber(item.delta)) line += ` (${format(item.delta, item.deltaFormat ?? item.format, true)})`;
        return item.hint ? `${line} — ${item.hint}` : line;
      });
    case "bars":
      return list(block.items).map((item) => `${item.label}: ${format(item.value, block.format)}`);
    case "chart": {
      const series = list(block.series);
      const x = Array.isArray(block.x) ? block.x : [];
      return [
        ["", ...series.map((one) => one.name)].join("\t"),
        ...x.map((label, index) =>
          [String(label), ...series.map((one) => format((one.values ?? [])[index], block.format))].join("\t"),
        ),
      ];
    }
    case "table": {
      // A bare name is a column whose key and label are that name.
      const columns = (Array.isArray(block.columns) ? block.columns : [])
        .map((column) => (typeof column === "string" && column ? { key: column, label: column } : column))
        .filter((column) => column && typeof column === "object");
      return [
        columns.map((column) => column.label ?? column.key).join("\t"),
        ...list(block.rows).map((row) => columns.map((column) => format(row[column.key], column.format)).join("\t")),
      ];
    }
    case "steps":
      return list(block.items).map(
        (item, index) => `${index + 1}. ${item.status ? `[${item.status}] ` : ""}${item.title}${item.detail ? ` — ${item.detail}` : ""}`,
      );
    case "callout":
      return [`${String(block.tone ?? "info").replace(/^./, (c) => c.toUpperCase())}: ${block.text ?? ""}`];
    case "actions":
      return [list(block.items).map((item) => `[${item.label}]`).join(" ")];
    default:
      return [];
  }
}

function read(source) {
  try {
    const block = JSON.parse(source);
    return block && typeof block === "object" && TYPES.includes(block.type) ? block : null;
  } catch {
    return null;
  }
}

/** A block's name: `[title]`, `[type]` or `[block]`. */
export function blockTitle(source) {
  const block = read(source);
  if (block) return `[${typeof block.title === "string" ? block.title : block.type}]`;
  const type = /"type"\s*:\s*"([a-z-]+)"/.exec(source)?.[1];
  return `[${TYPES.includes(type) ? type : "block"}]`;
}

/** A block as text, or its name when it cannot be read. Never its JSON. */
export function blockText(source) {
  const block = read(source);
  if (!block) return blockTitle(source);
  const lines = [...(typeof block.title === "string" ? [block.title] : []), ...body(block)];
  if (typeof block.source === "string") lines.push(`Source: ${block.source}`);
  return lines.join("\n");
}
