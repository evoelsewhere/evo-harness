// What the quick entry box answers itself and what it sends to the main
// window (docs/features/quick-entry/spec.md, FR-14; plan 3.5 "Questions").

import type { PermissionRequest } from "@/features/chat/transcript";

/**
 * - `inBox`: a short approval or one question, answered in the box;
 * - `inBoxWithReview`: an edit, answered in the box with **Review in Evo**;
 * - `elsewhere`: "Evo needs you" with **Open in Evo**.
 */
export type Answerable = "inBox" | "inBoxWithReview" | "elsewhere";

export function answerable(request: PermissionRequest): Answerable {
  // The app's own tools ask through the shell: acting in an app or on the
  // computer, the person's browser, a plugin's consent. Their choices are
  // answered in the box like any approval (FR-14, changed 2026-11-23).
  if (request.requestId.startsWith("shell:")) return request.options.length > 0 ? "inBox" : "elsewhere";
  if (request.kind === "questions") return request.questions?.length === 1 ? "inBox" : "elsewhere";
  if (request.approval?.kind === "edit") return "inBoxWithReview";
  if (request.approval || request.kind === "confirm") return "inBox";
  return "elsewhere";
}
