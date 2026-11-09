// What the agent is doing on a page, in a few words, for a card's bar. The shell sends the
// tool's name and a few short arguments (`agent://activity`); the rows of the
// transcript use their own, longer wording (components/ai-elements/tool.tsx).

type Args = Record<string, unknown>;

const text = (value: unknown, max = 28) => {
  const s = String(value ?? "");
  return s.length > max ? `${s.slice(0, max)}…` : s;
};

const BROWSER: Record<string, (a: Args) => string> = {
  browser_navigate: (a) => (["back", "forward", "reload"].includes(String(a.url)) ? `Going ${a.url}` : `Opening ${text(a.url, 40)}`),
  browser_tabs: () => "Checking tabs",
  browser_screenshot: () => "Taking a screenshot",
  browser_read_page: () => "Reading the page",
  browser_find: (a) => `Finding "${text(a.query)}"`,
  browser_click: (a) => (a.ref ? `Clicking ${a.ref}` : "Clicking"),
  browser_hover: (a) => (a.ref ? `Hovering ${a.ref}` : "Hovering"),
  browser_drag: () => "Dragging",
  browser_type: (a) => `Typing "${text(a.text, 20)}"`,
  browser_key: (a) => `Pressing ${text(a.key, 20)}`,
  browser_form_input: (a) => `Filling ${a.ref ?? "a field"}`,
  browser_scroll: (a) => `Scrolling ${a.direction ?? ""}`.trim(),
  browser_wait: (a) => (a.text ? `Waiting for "${text(a.text, 20)}"` : "Waiting"),
  browser_dialog: () => "Answering a dialog",
  browser_upload: () => "Attaching a file",
  browser_eval: () => "Running a script",
  browser_console: () => "Reading the console",
  browser_network: () => "Reading requests",
  browser_viewport: () => "Changing the viewport",
  browser_storage: () => "Changing storage",
  browser_storage_read: () => "Reading storage",
  browser_mock: () => "Mocking requests",
  browser_pdf: () => "Saving a PDF",
  browser_performance: () => "Measuring",
  browser_batch: () => "Running steps",
  browser_frames: () => "Listing frames",
};

/** The words for a tool call; `null` for a tool a card has nothing to say about. */
export function activityLabel(tool: string, args: Args = {}): string | null {
  if (tool === "computer_app") {
    // The shell sends the batch's first acting step as the person reads it ("click e12").
    const step = typeof args.action === "string" && args.action ? args.action : "";
    return step ? step.charAt(0).toUpperCase() + step.slice(1) : "Looking at the app";
  }
  const make = BROWSER[tool];
  if (make) return make(args);
  if (tool.startsWith("browser_")) return "Using the page";
  return null;
}
