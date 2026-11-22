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
      /* A light around the page while Evo works in it: a crisp accent edge and a
         soft glow spreading inwards from it, breathing slowly. */
      .frame { position: fixed; inset: 0; pointer-events: none; border-radius: 0;
        box-shadow: inset 0 0 0 2px rgba(0, 114, 245, 0.75), inset 0 0 14px 3px rgba(0, 114, 245, 0.45), inset 0 0 44px 10px rgba(0, 114, 245, 0.18);
        animation: evo-breathe 2.6s ease-in-out infinite; }
      @keyframes evo-breathe {
        0%, 100% { box-shadow: inset 0 0 0 2px rgba(0, 114, 245, 0.75), inset 0 0 14px 3px rgba(0, 114, 245, 0.45), inset 0 0 44px 10px rgba(0, 114, 245, 0.18); }
        50% { box-shadow: inset 0 0 0 2px rgba(0, 114, 245, 0.9), inset 0 0 22px 5px rgba(0, 114, 245, 0.55), inset 0 0 70px 16px rgba(0, 114, 245, 0.24); }
      }
      .dot { animation: evo-pulse 1.6s ease-in-out infinite; }
      @keyframes evo-pulse { 50% { box-shadow: 0 0 0 4px rgba(0, 114, 245, 0.18); } }
      @media (prefers-reduced-motion: reduce) { .frame, .dot { animation: none; } }
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
      /* The agent's virtual cursor, as the app draws it over a page's picture
         (src/features/cards/agent-cursor.tsx): an arrow that glides to each of
         the agent's mouse events and a ring where it presses. In a person's own
         browser it stays where the agent last put it for as long as the agent
         is in control, so you can always see where it is; it is gone when you
         take over or the agent lets go of the tab. */
      .cursor { position: fixed; top: 0; left: 0; pointer-events: none; transition: transform 150ms ease-out, opacity 150ms ease-out; opacity: 0; }
      .cursor svg { position: relative; margin: -2.3px 0 0 -2.3px; filter: drop-shadow(0 0 3px rgba(96, 165, 250, 0.7)); }
      .ripple { position: absolute; top: 0; left: 0; width: 24px; height: 24px; border-radius: 50%; border: 1.5px solid #9aa0ab; box-sizing: border-box; opacity: 0; }
      .ripple.go { animation: evo-ripple 450ms ease-out forwards; }
      @keyframes evo-ripple { from { opacity: 0.9; transform: translate(-50%, -50%) scale(0.35); } to { opacity: 0; transform: translate(-50%, -50%) scale(2.2); } }
    </style>
    <div class="frame"></div>
    <div class="cursor" aria-hidden="true"><span class="ripple"></span><svg width="17" height="17" viewBox="0 0 17 17"><path d="M2.8 2.8 L16.02 7.25 L8.6 8.6 L7.25 16.02 Z" fill="#9ccbff" stroke="#9ccbff" stroke-width="3.4" stroke-linejoin="round"/><path d="M2.8 2.8 L16.02 7.25 L8.6 8.6 L7.25 16.02 Z" fill="#1c2033" stroke="#1c2033" stroke-width="1.8" stroke-linejoin="round"/></svg></div>
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
  const cursor = root.querySelector(".cursor");
  const ripple = root.querySelector(".ripple");
  /** The agent puts the mouse at (x, y) of the page; a press restarts the ring. */
  const point = ({ kind, x, y }) => {
    if (!Number.isFinite(x) || !Number.isFinite(y)) return;
    cursor.style.transform = `translate(${x}px, ${y}px)`;
    cursor.style.opacity = "1";
    if (kind === "down") {
      ripple.classList.remove("go");
      void ripple.offsetWidth;
      ripple.classList.add("go");
    }
  };
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
    cursor.hidden = taken;
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
      pointer(event) {
        mount();
        point({ kind: String(event?.kind || ""), x: Number(event?.x), y: Number(event?.y) });
      },
    }),
  });
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", mount, { once: true });
  else mount();
  render();
})();
