// The title a chat is shown with. Runs of one scheduled task share its
// name, so a run still named after its task shows its run date too
// ("Morning briefing · Nov 12"); a run the person renamed keeps their title
// (docs/features/ux-polish/spec.md, FR-5). Display only: the saved title
// is not changed.

export interface TitledChat {
  title: string;
  scheduled?: boolean;
  createdAt: number;
}

/** "Nov 12" in the person's locale. */
export const shortDay = (ms: number): string => new Date(ms).toLocaleDateString(undefined, { month: "short", day: "numeric" });

/**
 * `taskNames`: the scheduled tasks' names, or `null` while they are not
 * read yet (every scheduled run then gets its date).
 */
export function displayTitle(chat: TitledChat, taskNames: readonly string[] | null, day: (ms: number) => string = shortDay): string {
  if (!chat.title) return chat.title;
  // Named after a task: a run (also one saved before runs were flagged).
  const run = taskNames ? taskNames.includes(chat.title) : Boolean(chat.scheduled);
  return run ? `${chat.title} · ${day(chat.createdAt)}` : chat.title;
}
