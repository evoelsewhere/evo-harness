// docs/features/rich-replies/spec.md (FR-6).
import { describe, expect, test } from "bun:test";
import { formatValue } from "./format";

const en = { locale: "en-US" };
/** Intl writes non-breaking spaces; compare them as spaces. */
const spaced = (text: string) => text.replace(/\s/g, " ");

describe("values in a block, written by the app", () => {
  test("plain numbers", () => {
    expect(formatValue(1234.567, "number", en)).toBe("1,234.57");
    expect(formatValue(812, undefined, en)).toBe("812");
  });

  test("compact numbers", () => {
    expect(formatValue(3180000000, "compact", en)).toBe("3.18B");
    expect(formatValue(950, "compact", en)).toBe("950");
  });

  test("a percentage is given as a percentage", () => {
    expect(formatValue(12.5, "percent", en)).toBe("12.5%");
    expect(formatValue(100, "percent", en)).toBe("100%");
  });

  test("currency always in full, never mixed with a compact form", () => {
    const usd = { currency: "USD" };
    expect([168.89, 1004.12, 2.35].map((value) => formatValue(value, usd, en))).toEqual(["$168.89", "$1,004.12", "$2.35"]);
  });

  test("the currency comes from the code, not the locale", () => {
    expect(formatValue(2.35, { currency: "EUR" }, en)).toBe("€2.35");
    expect(spaced(formatValue(2.35, { currency: "USD" }, { locale: "de-DE" }))).toBe("2,35 $");
  });

  test("a currency code without a symbol shows the code", () => {
    expect(spaced(formatValue(5, { currency: "ZZZ" }, en))).toBe("ZZZ 5.00");
  });

  test("bytes", () => {
    expect(formatValue(0, "bytes", en)).toBe("0 B");
    expect(formatValue(1536, "bytes", en)).toBe("1.5 KB");
    expect(formatValue(5_000_000_000, "bytes", en)).toBe("4.7 GB");
  });

  test("durations from seconds", () => {
    expect(formatValue(0.35, "duration", en)).toBe("350ms");
    expect(formatValue(9.5, "duration", en)).toBe("9.5s");
    expect(formatValue(74.2, "duration", en)).toBe("1m 14s");
    expect(formatValue(3725, "duration", en)).toBe("1h 2m");
  });

  test("dates", () => {
    expect(formatValue("2026-11-10", "date", en)).toBe("Nov 10, 2026");
    expect(formatValue("soon", "date", en)).toBe("soon");
  });

  test("text and empty values", () => {
    expect(formatValue("7/7", "number", en)).toBe("7/7");
    expect(formatValue(null, "number", en)).toBe("—");
  });

  test("a signed change", () => {
    expect(formatValue(12.5, "percent", { ...en, signed: true })).toBe("+12.5%");
    expect(formatValue(-3, "number", { ...en, signed: true })).toBe("-3");
    expect(formatValue(-90, "duration", { ...en, signed: true })).toBe("-1m 30s");
    expect(formatValue(0, "number", { ...en, signed: true })).toBe("0");
  });
});
