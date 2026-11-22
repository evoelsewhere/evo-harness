// What Evo Webbridge's side panel shares with the desktop's thread: the
// transcript reducer that folds engine lines into turns, the rebuild of a
// saved chat, and the words of turns and tool rows. `bun run webbridge:chat`
// bundles this file into extensions/evo-webbridge/sidepanel/vendor/chat.js,
// so a chat streams and reads the same in the panel as in the app
// (docs/features/browser-bridge/spec.md, FR-10). A test keeps the bundle in
// step with the sources.

export {
  ENGINE_EXITED,
  initialTranscript,
  noteKind,
  transcriptFromMessages,
  transcriptReducer,
  turnReply,
  typedPrompt,
  userText,
  type Part,
  type ReasoningPart,
  type TextPart,
  type ToolPart,
  type TranscriptState,
  type Turn,
} from "@/features/chat/transcript";
export {
  childOf,
  parseDiff,
  skillBody,
  subject,
  toolHeader,
  toolKind,
  webPageBody,
  widgetLoading,
} from "@/components/ai-elements/tool-labels";
export {
  groupTitle,
  noteLabel,
  preview,
  reasoningTitle,
  runningInBackground,
  runningStep,
  seconds,
  segments,
  turnMetaLine,
} from "@/features/chat/turn-view";
export { explainError } from "@/features/chat/explain-error";
export { splitPrompt } from "@/features/chat/prompt-text";
export { parseAnnotatedMessage } from "@/lib/document-annotations";
export { replyMarkdown } from "@/features/chat/message-text";
