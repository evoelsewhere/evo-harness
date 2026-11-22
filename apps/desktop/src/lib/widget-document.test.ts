// docs/features/office-v3-parity/spec.md (FR-13): the document a widget runs in.
import { describe, expect, test } from "bun:test";
import { WIDGET_BASE_CSS, readWidgetTheme, wrapWidgetHtml } from "./widget-document";

const style = (values: Record<string, string>) => ({ getPropertyValue: (name: string) => values[name] ?? "" });

describe("the widget theme", () => {
  const theme = readWidgetTheme(style({ "--foreground": "#fff", "--background": "#111", "--muted": "#222", "--muted-foreground": "#999", "--brand": "#07f" }));
  test("carries the names the visualize guidelines use", () => {
    expect(theme["--color-text-primary"]).toBe("#fff");
    expect(theme["--color-text-secondary"]).toBe("#999");
    expect(theme["--color-background"]).toBe("#111");
    expect(theme["--color-background-secondary"]).toBe("#222");
    expect(theme["--color-success"]).toBeTruthy();
    expect(theme["--color-warning"]).toBeTruthy();
    expect(theme["--color-error"]).toBeTruthy();
  });
  test("leaves out what the app does not define", () => {
    expect("--font-sans" in theme).toBe(false);
  });
});

describe("the widget document", () => {
  test("a fragment gets the base style before its own style", () => {
    const html = wrapWidgetHtml("<style>.a{color:red}</style><div class=a>x</div>", {}, "light");
    expect(html).toContain(WIDGET_BASE_CSS);
    expect(html.indexOf(WIDGET_BASE_CSS)).toBeLessThan(html.indexOf(".a{color:red}"));
  });
  test("a full document gets it inside its head, ahead of the head's own style", () => {
    const html = wrapWidgetHtml("<html><head><style>.a{}</style></head><body>x</body></html>", {}, "light");
    expect(html.indexOf(WIDGET_BASE_CSS)).toBeLessThan(html.indexOf(".a{}"));
  });
  test("the dark class is set when the app is dark, and not when it is light", () => {
    expect(wrapWidgetHtml("x", {}, "dark")).toContain("classList.add('dark')");
    expect(wrapWidgetHtml("x", {}, "light")).not.toContain("classList.add('dark')");
  });
  test("theme updates toggle the dark class", () => {
    expect(wrapWidgetHtml("x", {}, "light")).toContain("classList.toggle('dark'");
  });
  test("a value cannot close the style element", () => {
    const html = wrapWidgetHtml("x", { "--font-sans": "</style><script>alert(1)</script>" }, "light");
    expect(html).not.toContain("</style><script>alert(1)");
  });
});
