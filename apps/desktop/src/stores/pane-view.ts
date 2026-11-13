// What a split-view pane sees of the engine store: the store as if the pane's
// chat were the active one. The focused pane's chat is the active one, so it
// sees the store itself; the other pane gets a view with its chat's runtime
// mirrored into the top-level fields (docs/features/chat-split-view/plan.md,
// 3.3; FR-3).

interface WithRuntimes {
  activeId: string | null;
  runtimes: Record<string, unknown>;
}

/** One view per store snapshot and chat: selectors over it stay stable between updates. */
const views = new WeakMap<object, Map<string, unknown>>();

export function paneView<S extends WithRuntimes, R>(store: S, chat: string, mirror: (runtime: R | undefined) => object): S {
  if (chat === store.activeId) return store;
  let byChat = views.get(store);
  if (!byChat) {
    byChat = new Map();
    views.set(store, byChat);
  }
  let view = byChat.get(chat) as S | undefined;
  if (!view) {
    view = { ...store, activeId: chat, ...mirror(store.runtimes[chat] as R | undefined) };
    byChat.set(chat, view);
  }
  return view;
}
