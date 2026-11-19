// An `evo-ui` fence in a reply (docs/features/rich-replies/spec.md, FR-1,
// FR-2, FR-3, FR-14): a placeholder while the fence is open, then the
// component, or its text with a note, or its source with "Couldn't draw this
// block". Parsing is memoised on the fence's text.

import * as React from "react";
import { Skeleton } from "@/components/ui/skeleton";
import { parseBlock, sniffType } from "@/lib/ui-blocks/parse";
import { blockText } from "@/lib/ui-blocks/text";
import type { BlockType, UiBlock as Block } from "@/lib/ui-blocks/types";
import { BlockFrame } from "./frame";
import { Actions, Bars, Callout, Stats, Steps } from "./simple-blocks";
import { Table } from "./table";

const Chart = React.lazy(() => import("./chart"));

const LABELS: Record<BlockType, string> = {
  stats: "Figures",
  bars: "Bar list",
  chart: "Chart",
  table: "Table",
  steps: "Steps",
  callout: "Note",
  actions: "Actions",
};

const PLACEHOLDER_HEIGHT: Record<BlockType, string> = {
  stats: "h-16",
  bars: "h-32",
  chart: "h-56",
  table: "h-40",
  steps: "h-24",
  callout: "h-6",
  actions: "h-7",
};

function Component({ block }: { block: Block }) {
  switch (block.type) {
    case "stats":
      return <Stats block={block} />;
    case "bars":
      return <Bars block={block} />;
    case "chart":
      return (
        <React.Suspense fallback={<Skeleton className="h-56 w-full" />}>
          <Chart block={block} />
        </React.Suspense>
      );
    case "table":
      return <Table block={block} />;
    case "steps":
      return <Steps block={block} />;
    case "callout":
      return <Callout block={block} />;
    case "actions":
      return <Actions block={block} />;
  }
}

export function UiBlock({
  source,
  incomplete,
  renderSource,
}: {
  /** The text inside the fence. */
  source: string;
  /** The fence has not closed yet (the reply is streaming). */
  incomplete: boolean;
  /** The block's source as an ordinary code block, for the fallback. */
  renderSource(): React.ReactNode;
}) {
  const result = React.useMemo(() => (incomplete ? null : parseBlock(source)), [source, incomplete]);

  if (!result) {
    const type = sniffType(source);
    return (
      <BlockFrame label={type ? `${LABELS[type]}, loading` : "Loading"} aria-busy>
        <Skeleton className={type ? `${PLACEHOLDER_HEIGHT[type]} w-full` : "h-16 w-full"} />
      </BlockFrame>
    );
  }
  if (result.status === "invalid") {
    return (
      <div className="my-3">
        {renderSource()}
        <p className="mt-1 text-xs text-muted-foreground">Couldn't draw this block: {result.message}</p>
      </div>
    );
  }
  const { block } = result;
  if (result.status === "text") {
    return (
      <BlockFrame title={block.title} label={LABELS[block.type]} source={block.source} note={result.note}>
        <pre className="max-h-[28rem] overflow-auto font-mono text-xs leading-5 whitespace-pre">{blockText({ ...block, title: undefined, source: undefined })}</pre>
      </BlockFrame>
    );
  }
  const note = result.dropped > 0 ? `${result.dropped} ${result.dropped === 1 ? "item" : "items"} could not be shown.` : undefined;
  return (
    <BlockFrame title={block.title} label={LABELS[block.type]} source={block.source} note={note} data-block-type={block.type}>
      <Component block={block} />
    </BlockFrame>
  );
}
