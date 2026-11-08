// Keeps the cards in step with the tabs they are views of (docs/features/floating-cards, FR-16):
// a tab that closes takes its card and chip with it. Only a tab that was there and is gone
// counts: after a reload the tabs are adopted a moment after the cards are restored.
// Kept apart from cards-store so the store (and agent-events, which uses it) do not pull in
// the browser store; the card layer imports this for its effect.

import { useBrowserStore } from "./browser-store";
import { forgetFrames } from "@/features/cards/stage-stream";
import { useCardsStore } from "./cards-store";

useBrowserStore.subscribe((state, previous) => {
  const now = new Set(state.all.map((t) => t.id));
  for (const tab of previous.all) {
    if (now.has(tab.id)) continue;
    useCardsStore.getState().removeTab("browser", tab.id);
    forgetFrames(tab.id);
  }
});
