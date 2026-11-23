import * as React from "react";
import {
  AppWindowIcon,
  FileIcon,
  FilePenIcon,
  FolderSearchIcon,
  GlobeIcon,
  MessagesSquareIcon,
  PlugIcon,
  BellIcon,
  BracesIcon,
  BrainIcon,
  ListChecksIcon,
  MessageCircleQuestionIcon,
  ScrollTextIcon,
  SearchIcon,
  ShapesIcon,
  TerminalIcon,
  WrenchIcon,
} from "lucide-react";
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
import { AgentIcon } from "@/components/icons/agent-icon";
import { BrowserIcon } from "@/components/icons/tool-icons";
import { type ToolPart } from "@/features/chat/transcript";
import { AgentRun } from "./agent-run";
import { CodeRun } from "./code-run";
import { CODE_TOOL } from "./code-run-model";
import { SessionProposal } from "./session-proposal";
import { FileLink } from "./file-link";
import { openLink } from "@/features/shell/app-commands";
import { TodoList } from "@/features/chat/todo-bar";
import { SentFiles } from "./sent-files";
import { WidgetFrame } from "@/components/widget-frame";
import { PluginCard } from "@/features/plugin-ui";
import { type Kind } from "./tool-kinds";
import { activity, skillBody, subject, toolHeader, toolKind, touchedLine, webPageBody, widgetLoading, type parseDiff } from "./tool-labels";

export { activity, toolKind };
import { useEngineStore } from "@/stores/engine-store";
import { useSessionStore } from "@/stores/session-store";

// shadcn/ui AI Elements · Tool on Trace, following a call through DeepSeek
// Harness's phases: preparing (the model is still writing the arguments, so
// the row already names the command or file), running, done, error. Each tool
// has its own body — a terminal for shells, a diff for edits, the file for
// reads — with the arguments and output as the fallback.

const icons: Record<Kind, React.ComponentType<{ className?: string }>> = {
  shell: TerminalIcon,
  read: FileIcon,
  write: FilePenIcon,
  edit: FilePenIcon,
  search: SearchIcon,
  list: FolderSearchIcon,
  web: GlobeIcon,
  browser: BrowserIcon,
  computer: AppWindowIcon,
  agent: AgentIcon,
  session: MessagesSquareIcon,
  skill: ScrollTextIcon,
  memory: BrainIcon,
  ask: MessageCircleQuestionIcon,
  todo: ListChecksIcon,
  code: BracesIcon,
  user: BellIcon,
  mcp: PlugIcon,
  visual: ShapesIcon,
  other: WrenchIcon,
};


/** Search results as links that open in the browser. */
function WebResults({ results }: { results: { title: string; url: string; snippet: string }[] }) {
  if (!results.length) return <p className="px-1 text-xs text-muted-foreground">No results.</p>;
  return (
    <ol className="flex flex-col gap-0.5">
      {results.map((r) => {
        let host = r.url;
        try {
          host = new URL(r.url).hostname.replace(/^www\./, "");
        } catch {
          // Keep the URL.
        }
        return (
          <li key={r.url}>
            <button
              type="button"
              title={r.url}
              onClick={() => openLink(r.url)}
              className="flex w-full min-w-0 flex-col rounded-md px-2 py-1 text-left transition-colors hover:bg-accent/60"
            >
              <span className="flex min-w-0 items-baseline gap-2">
                <span className="truncate text-[13px] text-link">{r.title || host}</span>
                <span className="shrink-0 text-[11px] text-muted-foreground">{host}</span>
              </span>
              {r.snippet && <span className="line-clamp-2 text-xs text-muted-foreground">{r.snippet}</span>}
            </button>
          </li>
        );
      })}
    </ol>
  );
}

/** One past chat a search found (`session_search`'s `structuredContent`). */
interface PastChat {
  id: string;
  title: string;
  date: string;
  summary?: string | null;
}

/**
 * The chats a past-chat search found; selecting one opens it
 * (docs/features/memory-topics-and-chat-recall/spec.md, FR-14).
 */
function PastChats({ hits }: { hits: PastChat[] }) {
  // Only chats still in the list can be opened from here.
  const listed = useSessionStore((s) => s.sessions);
  if (!hits.length) return <p className="px-1 text-xs text-muted-foreground">No past chat found.</p>;
  return (
    <ol className="flex flex-col gap-0.5">
      {hits.map((hit) => {
        const open = listed.some((chat) => chat.id === hit.id);
        const body = (
          <>
            <span className="flex min-w-0 items-baseline gap-2">
              <span className={open ? "truncate text-[13px] text-link" : "truncate text-[13px]"}>{hit.title}</span>
              <span className="shrink-0 text-[11px] text-muted-foreground tabular-nums">{hit.date}</span>
            </span>
            {hit.summary && <span className="line-clamp-2 text-xs text-muted-foreground">{hit.summary}</span>}
          </>
        );
        return (
          <li key={hit.id}>
            {open ? (
              <button
                type="button"
                title={`Open "${hit.title}"`}
                onClick={() => void useEngineStore.getState().selectSession(hit.id)}
                className="flex w-full min-w-0 flex-col rounded-md px-2 py-1 text-left transition-colors hover:bg-accent/60"
              >
                {body}
              </button>
            ) : (
              <div className="flex min-w-0 flex-col px-2 py-1">{body}</div>
            )}
          </li>
        );
      })}
    </ol>
  );
}

/** The lines a topic update took out and put in. */
function MemoryChange({ removed, added }: { removed: string[]; added: string[] }) {
  if (!removed.length && !added.length) return <p className="px-1 text-xs text-muted-foreground">No line changed.</p>;
  return (
    <CodeBlock size="sm">
      <CodeBlockContent wrap className="max-h-64" data-selectable>
        {removed.map((line, index) => (
          <CodeBlockLine key={`r${index}`} variant="removed" className="ps-6!">
            {line}
          </CodeBlockLine>
        ))}
        {added.map((line, index) => (
          <CodeBlockLine key={`a${index}`} variant="added" className="ps-6!">
            {line}
          </CodeBlockLine>
        ))}
      </CodeBlockContent>
    </CodeBlock>
  );
}


function Terminal({ command, output, failed }: { command?: string; output: string; failed: boolean }) {
  return (
    <CodeBlock size="sm" variant={failed ? "destructive" : "default"}>
      <CodeBlockContent wrap className="max-h-64" data-selectable>
        {command && (
          <CodeBlockLine>
            <span className="text-muted-foreground select-none">$ </span>
            {command}
          </CodeBlockLine>
        )}
        {(output || (failed ? "" : "(no output)")).split("\n").map((line, index) => (
          <CodeBlockLine key={index}>{line}</CodeBlockLine>
        ))}
      </CodeBlockContent>
    </CodeBlock>
  );
}

function Diff({ diff, path }: { diff: ReturnType<typeof parseDiff>; path?: string }) {
  return (
    <CodeBlock size="sm">
      {path && (
        <CodeBlockHeader>
          <CodeBlockTitle className="font-mono">{path}</CodeBlockTitle>
        </CodeBlockHeader>
      )}
      <CodeBlockContent className="max-h-72" data-selectable>
        {diff.lines.map((line, index) => (
          <CodeBlockLine key={index} variant={line.variant}>
            {line.text}
          </CodeBlockLine>
        ))}
      </CodeBlockContent>
    </CodeBlock>
  );
}

function Plain({ text, failed, title }: { text: string; failed: boolean; title?: string }) {
  return (
    <CodeBlock size="sm" variant={failed ? "destructive" : "default"}>
      {title && (
        <CodeBlockHeader>
          <CodeBlockTitle className="font-mono">{title}</CodeBlockTitle>
        </CodeBlockHeader>
      )}
      <CodeBlockContent wrap className="max-h-64" data-selectable>
        {text || "(no output)"}
      </CodeBlockContent>
    </CodeBlock>
  );
}

export function Tool({
  part,
  streaming,
  lineOnly,
}: {
  part: ToolPart;
  streaming: boolean;
  /** Just the row, never a body (a subagent's steps). */
  lineOnly?: boolean;
}) {
  // A program the agent ran is one Code row with its calls inside
  // (docs/features/code-mode/spec.md, FR-15).
  if (part.name === CODE_TOOL) return <CodeRun part={part} streaming={streaming} lineOnly={lineOnly} />;
  const { kind, about, phase, stopped, live, failed, child, label: shownLabel, fileVerb, diff } = toolHeader(part, streaming);
  const Icon = icons[kind];
  // A file the call read or changed opens in the Files tool, at the line it touched.
  const fileTitle =
    about && fileVerb ? (
      <>
        {fileVerb} <FileLink path={about} line={touchedLine(part)} />
      </>
    ) : null;

  const line = (
    <TraceLine status={live ? "running" : failed ? "error" : undefined}>
      <TraceIcon>
        {kind === "agent" ? (
          <AgentIcon agentType={child?.agentType} background={child?.background} live={live} tinted />
        ) : (
          <Icon />
        )}
      </TraceIcon>
      <TraceTitle className={kind === "shell" ? "font-mono text-[13px]" : undefined}>{fileTitle ?? shownLabel}</TraceTitle>
      {stopped && fileTitle && <TraceDescription className="text-xs">stopped</TraceDescription>}
      {child?.background && <TraceDescription className="text-xs">background</TraceDescription>}
      {diff && (diff.added || diff.removed) ? (
        <TraceDescription className="font-mono text-xs">
          <span className="text-status-ok">+{diff.added}</span> <span className="text-destructive">-{diff.removed}</span>
        </TraceDescription>
      ) : null}
    </TraceLine>
  );

  // A widget shows under its line: a placeholder while the model writes it,
  // then the widget itself (also when the chat is replayed: it is rebuilt
  // from the call's arguments).
  if (part.name === "show_widget" && !lineOnly && phase !== "error") {
    const args = (part.args && typeof part.args === "object" ? part.args : {}) as Record<string, unknown>;
    const html = phase !== "preparing" && typeof args.widget_code === "string" ? args.widget_code : "";
    const loading = widgetLoading(part);
    return (
      <div className="flex flex-col gap-2">
        {line}
        <WidgetFrame
          html={html}
          title={typeof args.title === "string" && args.title ? args.title : "Widget"}
          height={typeof args.height === "number" ? args.height : undefined}
          loadingMessages={loading ? [loading] : undefined}
        />
      </div>
    );
  }
  // Not expandable while the model is still writing the call.
  if (phase === "preparing" || lineOnly) return line;
  // Files the agent sent stay open under the line: they are the result.
  // A failed call keeps its result too (`isError`): only a sent one shows files.
  const sent = part.name === "send_file" && !part.details?.externalToolResult?.isError ? part.details?.externalToolResult?.structuredContent : undefined;
  if (sent && Array.isArray(sent.files) && sent.files.length) {
    return (
      <div className="@container flex flex-col">
        {line}
        <SentFiles files={sent.files} caption={sent.caption} />
      </div>
    );
  }
  // A plugin's card stays under its line, drawn from the call's result, so a
  // reopened chat shows it again (docs/features/plugin-ui/spec.md, FR-7).
  const card = part.name === "plugin_show_card" && !part.details?.externalToolResult?.isError ? part.details?.externalToolResult?.structuredContent : undefined;
  if (card && typeof card.plugin === "string" && typeof card.card === "string") {
    return (
      <div className="flex w-full flex-col gap-2">
        {line}
        <PluginCard plugin={card.plugin} card={card.card} title={typeof card.title === "string" ? card.title : undefined} data={card.data} />
      </div>
    );
  }
  // A proposed (or started) session is a card the person acts on.
  if (part.name === "start_session" && phase === "done") {
    return (
      <div className="flex flex-col gap-2">
        {line}
        <SessionProposal part={part} />
      </div>
    );
  }
  // A subagent shows its steps from the start, open while it runs; a continued one (`message_agent`) the same way.
  if (kind === "agent") {
    return (
      <TraceCollapsible defaultOpen={live || failed ? true : undefined}>
        <TraceCollapsibleTrigger>{line}</TraceCollapsibleTrigger>
        <TraceCollapsibleContent>
          <AgentRun part={{ ...part, phase }} live={live} />
        </TraceCollapsibleContent>
      </TraceCollapsible>
    );
  }
  if (phase === "running" && !part.output) return line;

  let body: React.ReactNode;
  if (kind === "browser" || kind === "computer")
    body = (
      <div className="flex flex-col gap-2">
        {part.images?.map((src, index) => (
          <img key={index} src={src} alt={kind === "computer" ? "App screenshot" : "Browser screenshot"} className="max-h-96 w-auto self-start rounded-md ring-1 ring-border" />
        ))}
        {part.output && <Plain text={part.output} failed={failed} />}
      </div>
    );
  else if (kind === "todo" && !failed && Array.isArray((part.args as any)?.todos))
    body = <TodoList todos={(part.args as any).todos} live={false} />;
  else if (kind === "ask" && !failed)
    body = <Plain text={part.output} failed={false} title="Answers" />;
  else if (part.name === "web_search" && !failed && Array.isArray(part.details?.results))
    body = <WebResults results={part.details.results} />;
  else if (part.name === "web_fetch" && !failed)
    body = (
      <Plain
        text={webPageBody(part.output)}
        failed={false}
        title={part.details?.title ? String(part.details.title) : String(part.details?.finalUrl ?? "Page")}
      />
    );
  else if (kind === "shell") body = <Terminal command={subject(part)} output={part.output} failed={failed} />;
  else if (diff && !failed) body = <Diff diff={diff} path={about} />;
  else if (kind === "read" && !failed) body = <Plain text={part.output} failed={false} title={about} />;
  else if (kind === "skill" && !failed) body = <Plain text={skillBody(part.output)} failed={false} title="Instructions" />;
  else if (part.name === "session_search" && !failed && Array.isArray(part.details?.externalToolResult?.structuredContent?.hits))
    body = <PastChats hits={part.details.externalToolResult.structuredContent.hits} />;
  else if (kind === "memory" && !failed && (part.args as any)?.action === "update" && part.details?.diff)
    body = (
      <MemoryChange
        removed={Array.isArray(part.details.diff.removed) ? part.details.diff.removed : []}
        added={Array.isArray(part.details.diff.added) ? part.details.diff.added : []}
      />
    );
  else if (kind === "memory" && !failed && (part.args as any)?.action === "save")
    body = <Plain text={String((part.args as any)?.content ?? "")} failed={false} title={String((part.args as any)?.description ?? "Memory")} />;
  else if (kind === "other" || kind === "mcp")
    body = (
      <div className="flex flex-col gap-2">
        <Plain
          text={JSON.stringify((part.name === "mcp_call_tool" ? (part.args as any)?.arguments : part.args) ?? {}, null, 2)}
          failed={false}
          title="Input"
        />
        <Plain text={part.output} failed={failed} title={failed ? "Error" : "Output"} />
      </div>
    );
  else body = <Plain text={part.output} failed={failed} />;

  return (
    <TraceCollapsible defaultOpen={failed ? true : undefined}>
      <TraceCollapsibleTrigger>{line}</TraceCollapsibleTrigger>
      <TraceCollapsibleContent>{body}</TraceCollapsibleContent>
    </TraceCollapsible>
  );
}
