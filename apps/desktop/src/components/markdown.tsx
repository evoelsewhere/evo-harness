"use client";

import * as React from "react";
import { Streamdown, useIsCodeFenceIncomplete, type ExtraProps } from "streamdown";
import {
  CodeBlock,
  CodeBlockActions,
  CodeBlockContent,
  CodeBlockCopyButton,
  CodeBlockHeader,
  CodeBlockLine,
  CodeBlockTitle,
} from "@/components/ui/code-block";
import { useHighlightedCode } from "@/hooks/use-highlighted-code";
import { MermaidBlock } from "@/components/reply-blocks/mermaid-block";
import { UiBlock } from "@/components/reply-blocks/ui-block";
import { FILE_LINK, linkFiles } from "@/components/file-refs";
import { BLOCK_FENCE } from "@/lib/ui-blocks/types";
import { useSmoothText } from "@/hooks/use-smooth-text";
import { cn } from "@/lib/utils";

// shadcn/ui · Code Block › Markdown: render the reply with Streamdown and
// route fenced code to a CodeBlock, so code gets line highlighting and a copy
// button. Streamdown sets `data-block` on fenced code only.

/** Replies draw `evo-ui` and `mermaid` fences (docs/features/rich-replies/spec.md, FR-1, FR-9). */
const ReplyBlocksContext = React.createContext(false);

/** A diff line's mark: added, removed or neither (`+++`/`---` headers are neither). */
function diffVariant(line: string): "added" | "removed" | "default" {
  if (line.startsWith("+") && !line.startsWith("+++")) return "added";
  if (line.startsWith("-") && !line.startsWith("---")) return "removed";
  return "default";
}

function MarkdownCodeBlock({ code, language }: { code: string; language: string }) {
  // True while the closing fence has not arrived yet.
  const incomplete = useIsCodeFenceIncomplete();
  const lines = useHighlightedCode(code, language);
  const raw = language === "diff" ? code.split("\n") : null;

  return (
    <CodeBlock className="my-3">
      <CodeBlockHeader>
        <CodeBlockTitle>{language || "text"}</CodeBlockTitle>
        <CodeBlockActions>
          <CodeBlockCopyButton value={code} disabled={incomplete} />
        </CodeBlockActions>
      </CodeBlockHeader>
      <CodeBlockContent className="max-h-[28rem]" data-selectable>
        {lines.map((line, index) => (
          <CodeBlockLine key={index} variant={raw ? diffVariant(raw[index] ?? "") : "default"}>
            {line}
          </CodeBlockLine>
        ))}
      </CodeBlockContent>
    </CodeBlock>
  );
}

function MarkdownCode({ node: _node, className, children, ...props }: React.ComponentProps<"code"> & ExtraProps) {
  // Streamdown marks fenced code with data-block. Everything else is inline.
  if (!("data-block" in props)) {
    return (
      <code className="rounded-sm bg-muted px-1 py-0.5 font-mono text-[0.9em]" {...props}>
        {children}
      </code>
    );
  }
  const code = typeof children === "string" ? children : "";
  const language = /language-(\S+)/.exec(className ?? "")?.[1] ?? "";
  return <MarkdownFence code={code} language={language} />;
}

function MarkdownFence({ code, language }: { code: string; language: string }) {
  const replyBlocks = React.useContext(ReplyBlocksContext);
  const incomplete = useIsCodeFenceIncomplete();
  const source = () => <MarkdownCodeBlock code={code} language={language === BLOCK_FENCE ? "json" : language} />;
  if (replyBlocks && language === BLOCK_FENCE) return <UiBlock source={code} incomplete={incomplete} renderSource={source} />;
  if (replyBlocks && language === "mermaid" && !incomplete) return <MermaidBlock code={code} renderSource={source} />;
  return <MarkdownCodeBlock code={code} language={language} />;
}

const components = { code: MarkdownCode };

/** The agent's reply as markdown (shadcn/ui AI Elements · Message). */
export const Markdown = React.memo(function Markdown({
  children,
  className,
  streaming = false,
  onLocalLink,
  blocks = false,
}: {
  children: string;
  className?: string;
  /** Pass while the reply streams, so incomplete fences render safely. */
  streaming?: boolean;
  /** Files the text names (relative links, paths, file names) call this with the reference instead of being blocked (rich-replies FR-37). */
  onLocalLink?(href: string): void;
  /** The agent's reply: draw `evo-ui` blocks and Mermaid diagrams (docs/features/rich-replies/spec.md, FR-1, FR-9). */
  blocks?: boolean;
}) {
  const localComponents = React.useMemo(
    () =>
      onLocalLink
        ? {
            ...components,
            a: ({ node: _node, href, children: label, ...props }: React.ComponentProps<"a"> & ExtraProps) =>
              href?.startsWith(FILE_LINK) ? (
                <button
                  type="button"
                  onClick={() => onLocalLink(decodeURIComponent(href.slice(FILE_LINK.length)))}
                  className="cursor-pointer text-link underline-offset-4 hover:underline"
                >
                  {label}
                </button>
              ) : (
                <a href={href} {...props}>
                  {label}
                </a>
              ),
          }
        : components,
    [onLocalLink],
  );
  // Streamed text is revealed at a steady pace rather than in network bursts.
  const smooth = useSmoothText(children, streaming);
  const text = onLocalLink ? linkFiles(smooth.text) : smooth.text;
  return (
    <div
      data-selectable
      className={cn(
        "max-w-none text-sm leading-6 text-foreground",
        "[&_a]:text-link [&_a]:underline-offset-4 hover:[&_a]:underline",
        "[&_ol]:list-decimal [&_ol]:pl-5 [&_ul]:list-disc [&_ul]:pl-5 [&_li]:my-1",
        "[&_h1]:text-xl [&_h1]:font-semibold [&_h1]:tracking-tight [&_h2]:text-base [&_h2]:font-semibold [&_h3]:font-medium",
        "[&_p]:my-2 [&>*:first-child]:mt-0 [&>*:last-child]:mb-0",
        className,
      )}
    >
      <ReplyBlocksContext.Provider value={blocks}>
        <Streamdown controls={false} components={localComponents} isAnimating={streaming || smooth.revealing}>
          {text}
        </Streamdown>
      </ReplyBlocksContext.Provider>
    </div>
  );
});
