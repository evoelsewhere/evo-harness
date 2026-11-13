// New-chat suggestions that fit where the chat works: general tasks outside
// a project, project tasks in a folder, and reviewing the changes first in a
// repository that has some (docs/features/ux-polish/spec.md, FR-8).

import {
  BugIcon,
  FileSearchIcon,
  FlaskConicalIcon,
  GitCompareIcon,
  LightbulbIcon,
  ListChecksIcon,
  MailIcon,
  MapIcon,
  SearchIcon,
  type LucideIcon,
} from "lucide-react";

export interface Suggestion {
  icon: LucideIcon;
  label: string;
  hint: string;
  prompt: string;
}

export interface Place {
  /** The chat works in a folder (a workspace). */
  workspace: boolean;
  /** That folder is in a git repository. */
  repo: boolean;
  /** Files changed against HEAD. */
  changed: number;
}

const general: Suggestion[] = [
  { icon: ListChecksIcon, label: "Plan my day", hint: "Turn a list into steps", prompt: "Help me plan my day. Here is what I need to get done:\n" },
  { icon: SearchIcon, label: "Research a topic", hint: "Sources and a short summary", prompt: "Research this topic and give me a short summary with sources: " },
  { icon: MailIcon, label: "Draft a message", hint: "An email or a chat reply", prompt: "Draft a clear, friendly message about: " },
  { icon: LightbulbIcon, label: "Explain something", hint: "In plain words, with examples", prompt: "Explain this in plain words, with an example: " },
];

const explain: Suggestion = {
  icon: MapIcon,
  label: "Explain this project",
  hint: "Its structure and how the parts fit",
  prompt: "Give me a tour of this project: its structure, main parts and how they fit together.",
};
const bugs: Suggestion = {
  icon: BugIcon,
  label: "Find bugs",
  hint: "Likely problems in recent changes",
  prompt: "Look through the recent changes and point out likely bugs.",
};
const tests: Suggestion = {
  icon: FlaskConicalIcon,
  label: "Write tests",
  hint: "Cover code that has none yet",
  prompt: "Find code that lacks tests and write them.",
};
const docs: Suggestion = {
  icon: FileSearchIcon,
  label: "Summarize docs",
  hint: "The key points of the docs",
  prompt: "Summarize the documentation in this workspace.",
};

export function suggestionsFor(place: Place): Suggestion[] {
  if (!place.workspace) return general;
  if (!place.repo) return [explain, docs, tests, general[1]];
  if (place.changed > 0) {
    const review: Suggestion = {
      icon: GitCompareIcon,
      label: place.changed === 1 ? "Review the changed file" : `Review the ${place.changed} changed files`,
      hint: "What changed, and anything risky",
      prompt: "Review my uncommitted changes: summarise what changed and point out bugs or risks before I commit.",
    };
    return [review, explain, bugs, tests];
  }
  return [explain, bugs, tests, docs];
}
