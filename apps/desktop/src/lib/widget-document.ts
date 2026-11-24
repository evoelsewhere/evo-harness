// The document a `show_widget` widget runs in (docs/features/office-v3-parity/spec.md,
// FR-13; port of v3 web/src/components/WidgetRenderer.tsx `wrapHtml` and
// web/src/lib/widgetTheme.ts). No React.
//
// The widget's HTML gets, in its <head>: the v3 Content-Security-Policy (only
// the CDN allowlist may serve scripts; no network otherwise), the app's theme
// as CSS variables (v3 names, filled from evo's tokens), an in-memory
// localStorage/sessionStorage (an opaque-origin frame has none),
// `sendPrompt(text)` and a height reporter, both via postMessage to the
// parent, and a listener that applies theme updates from the parent.

// CDN allowlist for scripts (v3).
export const ALLOWED_CDNS = ["cdnjs.cloudflare.com", "cdn.jsdelivr.net", "unpkg.com", "esm.sh"];

/** Bounds for sizing the frame to its content (v3). */
export const MIN_WIDGET_HEIGHT = 120;
export const MAX_WIDGET_HEIGHT = 2400;

export const clampWidgetHeight = (height: number) => Math.min(Math.max(height, MIN_WIDGET_HEIGHT), MAX_WIDGET_HEIGHT);

/** Messages a widget posts to the app, and the one the app posts back. */
export const WIDGET_SEND_PROMPT = "widget_send_prompt";
export const WIDGET_RESIZE = "widget_resize";
export const WIDGET_THEME = "evo_widget_theme";
/** The event a widget's window gets after the app's theme, accent or background changed. */
export const WIDGET_THEME_EVENT = "evo:themechange";

export function buildWidgetCsp(): string {
  const scriptSrc = ["'self'", "blob:", "data:", ...ALLOWED_CDNS, "'unsafe-inline'", "'unsafe-eval'"];
  const styleSrc = ["'self'", "blob:", "data:", "'unsafe-inline'"];
  const imgSrc = ["'self'", "blob:", "data:"];
  const fontSrc = ["'self'", "blob:", "data:"];
  const connectSrc = ["'self'"];
  const frameSrc = ["'self'"];
  return [
    "default-src 'none'",
    `script-src ${scriptSrc.join(" ")}`,
    `style-src ${styleSrc.join(" ")}`,
    `img-src ${imgSrc.join(" ")}`,
    `font-src ${fontSrc.join(" ")}`,
    `connect-src ${connectSrc.join(" ")}`,
    `frame-src ${frameSrc.join(" ")}`,
    "object-src 'none'",
    "base-uri 'self'",
  ].join("; ");
}

/** The theme variables a widget may use (v3 names; the visualize guidelines refer to them). */
export const WIDGET_THEME_PROPERTIES = [
  "--bg-page",
  "--bg-card",
  "--bg-key",
  "--bg-muted",
  "--color-text",
  "--color-text-2",
  "--color-text-muted",
  "--color-text-subtle",
  "--color-text-on-accent",
  "--color-border",
  "--color-border-strong",
  "--color-accent",
  "--color-accent-soft",
  "--accent-blue",
  "--accent-blue-soft",
  "--accent-blue-text",
  "--accent-green",
  "--accent-green-soft",
  "--accent-green-text",
  "--accent-amber",
  "--accent-amber-soft",
  "--accent-red",
  "--accent-red-soft",
  "--font-sans",
  // The names the visualize guidelines use (crates/evo-engine/src/tools/visualize/*.md).
  "--color-text-primary",
  "--color-text-secondary",
  "--color-background",
  "--color-background-secondary",
  "--color-success",
  "--color-warning",
  "--color-error",
] as const;

export type WidgetTheme = Partial<Record<(typeof WIDGET_THEME_PROPERTIES)[number], string>>;

const soft = (color: string) => `color-mix(in srgb, ${color} 14%, transparent)`;

/**
 * The widget theme from evo's tokens (index.css): achromatic surfaces, the
 * one accent, status colours for green/amber/red.
 */
export function readWidgetTheme(style: Pick<CSSStyleDeclaration, "getPropertyValue">): WidgetTheme {
  const read = (name: string, fallback = "") => style.getPropertyValue(name).trim() || fallback;
  const accent = read("--brand", "#0072f5");
  const green = read("--color-status-ok", "#45a557");
  const amber = read("--color-status-warn", "#ff990a");
  const red = read("--destructive", "#e5484d");
  const theme: WidgetTheme = {
    "--bg-page": read("--background"),
    "--bg-card": read("--card"),
    "--bg-key": read("--accent"),
    "--bg-muted": read("--muted"),
    "--color-text": read("--foreground"),
    "--color-text-2": read("--sidebar-foreground"),
    "--color-text-muted": read("--muted-foreground"),
    "--color-text-subtle": read("--muted-foreground"),
    "--color-text-on-accent": read("--brand-foreground", "#ffffff"),
    "--color-border": read("--border"),
    "--color-border-strong": read("--input"),
    "--color-accent": accent,
    "--color-accent-soft": soft(accent),
    "--accent-blue": accent,
    "--accent-blue-soft": soft(accent),
    "--accent-blue-text": accent,
    "--accent-green": green,
    "--accent-green-soft": soft(green),
    "--accent-green-text": green,
    "--accent-amber": amber,
    "--accent-amber-soft": soft(amber),
    "--accent-red": red,
    "--accent-red-soft": soft(red),
    "--font-sans": read("--app-font-sans"),
    "--color-text-primary": read("--foreground"),
    "--color-text-secondary": read("--muted-foreground"),
    "--color-background": read("--background"),
    "--color-background-secondary": read("--muted"),
    "--color-success": green,
    "--color-warning": amber,
    "--color-error": red,
  };
  for (const key of Object.keys(theme) as (keyof WidgetTheme)[]) if (!theme[key]) delete theme[key];
  return theme;
}

export function buildWidgetThemeCss(theme: WidgetTheme): string {
  const declarations = Object.entries(theme)
    .filter(([, value]) => Boolean(value))
    .map(([property, value]) => `${property}: ${value}`);
  return declarations.length ? `:root { ${declarations.join("; ")}; }` : "";
}

/**
 * The page's base style, before the widget's own: the app's font, size and
 * text colour. The frame's document starts with the browser's defaults (a
 * serif face), so a widget that sets no font of its own would not match the
 * app. The widget's styles come later and override these.
 */
export const WIDGET_BASE_CSS =
  "html { font-family: var(--font-sans, ui-sans-serif, system-ui, sans-serif); font-size: 14px; line-height: 1.5; color: var(--color-text, CanvasText); background: var(--bg-page, Canvas); }" +
  // Media never pushes the page wider than the chat column.
  " img, video, canvas { max-width: 100%; } svg[viewBox] { max-width: 100%; height: auto; }";

/**
 * Sets the theme as inline style on `<html>`. The visualize guide shows a
 * `:root { --color-background: #ffffff; … }` palette and models copy it into
 * the widget's own `<style>`, which comes after ours and would win: inline
 * style beats any stylesheet rule on the root, so the app's theme holds and
 * the widget's palette is only a fallback.
 */
function themeSeedScript(theme: WidgetTheme, colorScheme?: "light" | "dark"): string {
  const json = JSON.stringify(theme).replace(/</g, "\\u003c");
  const dark = colorScheme === "dark" ? "document.documentElement.classList.add('dark');" : "";
  return `<script>(function(t){var s=document.documentElement.style;for(var k in t)s.setProperty(k,t[k]);${dark}})(${json});</script>`;
}

/** Keep `</style>` and friends from closing the element a value is written into. */
const inert = (text: string) => text.replace(/</g, "\\3c ");

const STORAGE_SHIM = `<script>(function(){function createStorage(){var data=new Map();return{get length(){return data.size},key:function(index){return Array.from(data.keys())[index]||null},getItem:function(key){key=String(key);return data.has(key)?data.get(key):null},setItem:function(key,value){data.set(String(key),String(value));},removeItem:function(key){data.delete(key);},clear:function(){data.clear()}}}['localStorage','sessionStorage'].forEach(function(name){try{void window[name]}catch(e){Object.defineProperty(window,name,{value:createStorage(),configurable:true})}})})();</script>`;

const SEND_PROMPT = `<script>
  window.sendPrompt = function(prompt) {
    if (typeof prompt !== 'string' || !prompt.trim()) return;
    window.parent.postMessage({ type: '${WIDGET_SEND_PROMPT}', prompt: prompt.trim() }, '*');
  };
</script>`;

// Reports the content's height so the frame grows to fit instead of
// clipping it behind an inner scrollbar.
const RESIZE = `<script>(function(){
  function postHeight(){
    var body = document.body;
    var el = body || document.documentElement;
    var h = el.getBoundingClientRect ? el.getBoundingClientRect().height : Math.max(document.documentElement.scrollHeight, body ? body.scrollHeight : 0);
    if (!h) h = Math.max(document.documentElement.scrollHeight, body ? body.scrollHeight : 0);
    window.parent.postMessage({ type: '${WIDGET_RESIZE}', height: Math.ceil(h) + 2 }, '*');
  }
  function init(){
    postHeight();
    if (window.ResizeObserver) {
      var ro = new ResizeObserver(postHeight);
      ro.observe(document.documentElement);
      if (document.body) ro.observe(document.body);
    }
    window.addEventListener('load', postHeight);
    setTimeout(postHeight, 50);
    setTimeout(postHeight, 300);
  }
  if (document.readyState === 'loading') { document.addEventListener('DOMContentLoaded', init); } else { init(); }
})();</script>`;

// Theme updates from the app (only from the parent window).
const THEME_LISTENER = `<script>(function(){
  window.addEventListener('message', function(event){
    if (event.source !== window.parent) return;
    var data = event.data;
    if (!data || data.type !== '${WIDGET_THEME}' || !data.theme || typeof data.theme !== 'object') return;
    var style = document.documentElement.style;
    if (data.colorScheme === 'dark' || data.colorScheme === 'light') document.documentElement.classList.toggle('dark', data.colorScheme === 'dark');
    for (var key in data.theme) {
      if (/^--[a-z0-9-]+$/.test(key) && typeof data.theme[key] === 'string') style.setProperty(key, data.theme[key]);
    }
    if (data.colorScheme === 'dark' || data.colorScheme === 'light') style.colorScheme = data.colorScheme;
    // Canvas charts cannot read CSS variables: they listen for this and redraw.
    window.dispatchEvent(new Event('${WIDGET_THEME_EVENT}'));
  });
})();</script>`;

// A theme variable's current value, for canvas code (Chart.js) that cannot use `var()`.
const THEME_COLOR = `<script>window.themeColor = function(name){ return getComputedStyle(document.documentElement).getPropertyValue(name).trim(); };</script>`;

/** The widget's HTML as a full document with the CSP, theme, storage shim and bridges. */
export function wrapWidgetHtml(html: string, theme: WidgetTheme, colorScheme?: "light" | "dark"): string {
  const csp = buildWidgetCsp();
  const cspMeta = `<meta http-equiv="Content-Security-Policy" content="${csp.replaceAll('"', "&quot;")}">`;
  const themeCss = buildWidgetThemeCss(theme);
  const scheme = colorScheme ? `:root { color-scheme: ${colorScheme}; }` : "";
  const themeStyle = `<style id="evo-widget-theme">${inert(`${scheme}${themeCss}${WIDGET_BASE_CSS}`)}</style>`;
  // Inline theme, plus the `dark` class the guidelines' `.dark { ... }` palette keys on.
  const head = `${cspMeta}${themeStyle}${themeSeedScript(theme, colorScheme)}${STORAGE_SHIM}${SEND_PROMPT}${RESIZE}${THEME_LISTENER}${THEME_COLOR}`;
  if (/<head\b[^>]*>/i.test(html)) {
    return html.replace(/<head\b[^>]*>/i, (match) => `${match}${head}`);
  }
  return `<!doctype html><html><head><meta charset="utf-8">${head}</head><body>${html}</body></html>`;
}
