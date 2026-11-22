import { BracesIcon } from "lucide-react";
import { CodeBlock, CodeBlockContent, CodeBlockHeader, CodeBlockLine, CodeBlockTitle } from "@/components/ui/code-block";
import {
  TraceCollapsible,
  TraceCollapsibleContent,
  TraceCollapsibleTrigger,
  TraceDescription,
  TraceIcon,
  TraceLine,
  TraceTitle,
} from "@/components/ui/trace";
import { type ToolPart } from "@/features/chat/transcript";
import { codeCalls, codeRunMeta, codeRunSummary } from "./code-run-model";
import { Tool } from "./tool";

// A `run_code` call as one Code row (docs/features/code-mode/spec.md, FR-15):
// what the program was for, its status, calls and time; open, the code and
// each call it made as its usual card, then what it returned.

export function CodeRun({ part, streaming, lineOnly }: { part: ToolPart; streaming: boolean; lineOnly?: boolean }) {
  const summary = codeRunSummary(part);
  const live = summary.status === "running" && streaming;
  const failed = summary.status === "failed" || summary.status === "interrupted" || summary.status === "stopped";
  const title = summary.description || (summary.status === "running" ? "Running code" : "Ran code");
  const meta = codeRunMeta(summary);
  const line = (
    <TraceLine status={live ? "running" : failed ? "error" : undefined}>
      <TraceIcon>
        <BracesIcon />
      </TraceIcon>
      <TraceTitle>{title}</TraceTitle>
      <TraceDescription className="text-xs">{meta ? `Code · ${meta}` : "Code"}</TraceDescription>
    </TraceLine>
  );
  if (lineOnly || part.phase === "preparing") return line;
  const calls = codeCalls(part);
  return (
    <TraceCollapsible defaultOpen={summary.status === "failed" || summary.status === "stopped" ? true : undefined}>
      <TraceCollapsibleTrigger>{line}</TraceCollapsibleTrigger>
      <TraceCollapsibleContent>
        <div className="flex flex-col gap-2">
          {summary.code && (
            <CodeBlock size="sm">
              <CodeBlockHeader>
                <CodeBlockTitle className="font-mono">code.js</CodeBlockTitle>
              </CodeBlockHeader>
              <CodeBlockContent className="max-h-72" data-selectable>
                {summary.code.split("\n").map((text, index) => (
                  <CodeBlockLine key={index}>{text}</CodeBlockLine>
                ))}
              </CodeBlockContent>
            </CodeBlock>
          )}
          {calls.length > 0 && (
            <div className="flex flex-col gap-0.5 border-l pl-3" aria-label="Calls from code">
              {calls.map((call) => (
                <Tool key={call.key} part={call} streaming={live && call.phase === "running"} />
              ))}
            </div>
          )}
          {summary.status !== "running" && part.output && (
            <CodeBlock size="sm" variant={failed ? "destructive" : "default"}>
              <CodeBlockHeader>
                <CodeBlockTitle className="font-mono">{failed ? "Error" : "Result"}</CodeBlockTitle>
              </CodeBlockHeader>
              <CodeBlockContent wrap className="max-h-64" data-selectable>
                {part.output}
              </CodeBlockContent>
            </CodeBlock>
          )}
        </div>
      </TraceCollapsibleContent>
    </TraceCollapsible>
  );
}
