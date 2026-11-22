import { BrainIcon } from "lucide-react";
import {
  TraceCollapsible,
  TraceCollapsibleContent,
  TraceCollapsibleTrigger,
  TraceDescription,
  TraceIcon,
  TraceLine,
  TraceTitle,
} from "@/components/ui/trace";
import { preview, reasoningTitle, seconds } from "@/features/chat/turn-view";

export { seconds };

// shadcn/ui AI Elements · Reasoning on Trace, with DeepSeek Harness's
// behaviour: always starts collapsed, shows a one-line preview of the thinking,
// reads "Thinking" (shimmering) while it streams and "Thought for Ns" after.

export function Reasoning({
  text,
  streaming,
  startedAt,
  endedAt,
}: {
  text: string;
  streaming: boolean;
  startedAt: number;
  endedAt?: number;
}) {
  const title = reasoningTitle({ streaming, startedAt, endedAt });
  const line = (
    <TraceLine status={streaming ? "running" : undefined}>
      <TraceIcon>
        <BrainIcon />
      </TraceIcon>
      <TraceTitle className="shrink-0">{title}</TraceTitle>
      {text && <TraceDescription className="min-w-0 shrink truncate text-muted-foreground/70">{preview(text, streaming)}</TraceDescription>}
    </TraceLine>
  );
  if (!text) return line;
  return (
    <TraceCollapsible>
      <TraceCollapsibleTrigger>{line}</TraceCollapsibleTrigger>
      <TraceCollapsibleContent className="whitespace-pre-wrap" data-selectable>
        {text}
      </TraceCollapsibleContent>
    </TraceCollapsible>
  );
}
