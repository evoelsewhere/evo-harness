// How a block's values are written (docs/features/rich-replies/spec.md, FR-6).
// The model gives raw numbers and a format; the app writes them, so one field
// never mixes "$1K" with "$439.01". Separators and dates follow the locale;
// a currency always comes from its ISO code.

import type { Cell, Format } from "./types";

export interface FormatOptions {
  /** A BCP 47 locale; the system's when left out. */
  locale?: string;
  /** Show the sign of a change (+12.5%). */
  signed?: boolean;
}

const EMPTY = "—";
const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB", "PB"];

function number(locale: string | undefined, options: Intl.NumberFormatOptions): Intl.NumberFormat {
  return new Intl.NumberFormat(locale, options);
}

function bytes(value: number, locale: string | undefined): string {
  let size = Math.abs(value);
  let unit = 0;
  while (size >= 1024 && unit < BYTE_UNITS.length - 1) {
    size /= 1024;
    unit++;
  }
  return `${number(locale, { maximumFractionDigits: unit === 0 ? 0 : 1 }).format(size)} ${BYTE_UNITS[unit]}`;
}

function duration(seconds: number, locale: string | undefined): string {
  const total = Math.abs(seconds);
  if (total < 1) return `${Math.round(total * 1000)}ms`;
  if (total < 60) return `${number(locale, { maximumFractionDigits: 1 }).format(total)}s`;
  const whole = Math.round(total);
  const hours = Math.floor(whole / 3600);
  const minutes = Math.floor((whole % 3600) / 60);
  const rest = whole % 60;
  return hours > 0 ? `${hours}h ${minutes}m` : `${minutes}m ${rest}s`;
}

function date(value: number | string, locale: string | undefined): string | undefined {
  // A date without a time is a calendar day, the same in every time zone.
  const dayOnly = typeof value === "string" && /^\d{4}-\d{2}-\d{2}$/.test(value);
  const time = typeof value === "number" ? value : Date.parse(value);
  if (!Number.isFinite(time)) return undefined;
  return new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeZone: dayOnly ? "UTC" : undefined }).format(time);
}

/** Writes one value of a block in its format. */
export function formatValue(value: Cell | undefined, format?: Format, options: FormatOptions = {}): string {
  const { locale, signed = false } = options;
  if (value === null || value === undefined || value === "") return EMPTY;
  if (format === "date") return date(value, locale) ?? String(value);
  if (typeof value === "string") return value;
  if (!Number.isFinite(value)) return String(value);

  const sign: Intl.NumberFormatOptions["signDisplay"] = signed ? "exceptZero" : "auto";
  // Formats the app writes by hand get their sign by hand.
  const prefix = value < 0 ? "-" : signed && value > 0 ? "+" : "";

  if (typeof format === "object") {
    try {
      return number(locale, { style: "currency", currency: format.currency, signDisplay: sign }).format(value);
    } catch {
      return `${format.currency} ${number(locale, { minimumFractionDigits: 2, maximumFractionDigits: 2, signDisplay: sign }).format(value)}`;
    }
  }
  switch (format) {
    case "compact":
      return number(locale, { notation: "compact", maximumFractionDigits: 2, signDisplay: sign }).format(value);
    case "percent":
      return number(locale, { style: "percent", maximumFractionDigits: 1, signDisplay: sign }).format(value / 100);
    case "bytes":
      return prefix + bytes(value, locale);
    case "duration":
      return prefix + duration(value, locale);
    default:
      return number(locale, { maximumFractionDigits: 2, signDisplay: sign }).format(value);
  }
}
