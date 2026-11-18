// Evo Webbridge's in-page indicator (docs/features/browser-bridge/spec.md
// FR-15 – FR-17). The shell adds it to every document of a tab the agent works
// in, through the debugging API, and sets its state with
// `window.__evoOverlay.set({ state, chat })`: working | taken | off. Its
// buttons call the `__evoWebbridge` binding, which reaches the shell as an
// event; nothing the page does can act for the person beyond pressing them.
(() => {
  if (window.__evoOverlay || window.top !== window) return;
  const host = document.createElement("evo-webbridge");
  host.style.cssText = "all: initial; position: fixed; inset: 0; pointer-events: none; z-index: 2147483647;";
  const root = host.attachShadow({ mode: "closed" });
  root.innerHTML = `
    <style>
      .frame { position: fixed; inset: 0; box-shadow: inset 0 0 0 2px rgba(0, 114, 245, 0.55); pointer-events: none; }
      .bar { position: fixed; top: 10px; left: 50%; transform: translateX(-50%); display: flex; align-items: center; gap: 10px;
        padding: 6px 6px 6px 12px; border-radius: 8px; background: #fff; color: #171717; pointer-events: auto;
        box-shadow: 0 0 0 1px rgba(0, 114, 245, 0.6), 0 4px 16px rgba(0, 0, 0, 0.12);
        font: 500 12px/1.2 "Geist", system-ui, -apple-system, "Segoe UI", sans-serif; }
      .dot { width: 8px; height: 8px; border-radius: 50%; background: #0072f5; }
      .taken .dot { background: #8f8f8f; }
      button { font: inherit; font-weight: 400; border: 0; border-radius: 6px; padding: 5px 10px; cursor: pointer;
        background: #fff; color: #171717; box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.18); }
      button.stop { background: #171717; color: #fff; box-shadow: none; }
      button:focus-visible { outline: 2px solid #0072f5; outline-offset: 2px; }
      [hidden] { display: none !important; }
    </style>
    <div class="frame"></div>
    <div class="bar" role="status">
      <span class="dot"></span><span class="text"></span>
      <button class="take" type="button">Take over</button>
      <button class="back" type="button" hidden>Hand back</button>
      <button class="stop" type="button">Stop</button>
    </div>`;
  const bar = root.querySelector(".bar");
  const frame = root.querySelector(".frame");
  const text = root.querySelector(".text");
  const take = root.querySelector(".take");
  const back = root.querySelector(".back");
  const stop = root.querySelector(".stop");
  const say = (action) => {
    try {
      window.__evoWebbridge?.(JSON.stringify({ action }));
    } catch {
      // The binding is gone (the debugger detached).
    }
  };
  // The person's own input (FR-17): the agent's next act waits until they stop. The
  // shell tells it apart from the agent's input, which the browser reports alike.
  let reported = 0;
  const used = (event) => {
    if (!event.isTrusted || event.composedPath().includes(host)) return;
    const now = Date.now();
    if (now - reported < 400) return;
    reported = now;
    say("input");
  };
  for (const type of ["keydown", "pointerdown", "wheel"]) window.addEventListener(type, used, { capture: true, passive: true });
  take.addEventListener("click", () => say("takeover"));
  back.addEventListener("click", () => say("handback"));
  stop.addEventListener("click", () => say("stop"));
  let state = { state: "off", chat: "" };
  const render = () => {
    const on = state.state !== "off";
    host.hidden = !on;
    if (!on) return;
    const taken = state.state === "taken";
    bar.classList.toggle("taken", taken);
    frame.hidden = taken;
    text.textContent = taken ? `You have control · ${state.chat}` : `Evo is working · ${state.chat}`;
    take.hidden = taken;
    back.hidden = !taken;
  };
  const mount = () => {
    if (!host.isConnected) (document.documentElement || document).appendChild(host);
  };
  Object.defineProperty(window, "__evoOverlay", {
    value: Object.freeze({
      set(next) {
        state = { state: String(next?.state || "off"), chat: String(next?.chat || "") };
        mount();
        render();
      },
    }),
  });
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", mount, { once: true });
  else mount();
  render();
})();
