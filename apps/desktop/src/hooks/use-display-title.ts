import * as React from "react";
import { displayTitle, type TitledChat } from "@/lib/chat-title";
import { useSchedulerStore } from "@/stores/scheduler-store";

let loading: Promise<void> | null = null;
const loadTasksOnce = () => (loading ??= useSchedulerStore.getState().load().catch(() => undefined));

/** A chat's shown title: a scheduled run gets its run date (docs/features/ux-polish/spec.md, FR-5). */
export function useDisplayTitle(chat: TitledChat | null | undefined, fallback = ""): string {
  const loaded = useSchedulerStore((s) => s.loaded);
  const tasks = useSchedulerStore((s) => s.tasks);
  const names = React.useMemo(() => (loaded ? tasks.map((task) => task.name) : null), [loaded, tasks]);
  // The task names tell runs apart: read them once, whoever asks first.
  React.useEffect(() => {
    if (!loaded) void loadTasksOnce();
  }, [loaded]);
  if (!chat) return fallback;
  return displayTitle({ ...chat, title: chat.title || fallback }, names);
}
