// A sortable table in a reply (docs/features/rich-replies/spec.md, FR-18,
// FR-8, NFR-3). Sorting is view state only: it is not kept on reopen (FR-5).

import * as React from "react";
import { ArrowDownIcon, ArrowUpIcon } from "lucide-react";
import { formatValue } from "@/lib/ui-blocks/format";
import type { Cell, TableBlock } from "@/lib/ui-blocks/types";
import { cn } from "@/lib/utils";
import { actionHint, runBlockAction } from "./block-action";

type Sort = { key: string; dir: 1 | -1 } | null;

function compare(a: Cell, b: Cell): number {
  if (a === null || a === undefined) return b === null || b === undefined ? 0 : 1;
  if (b === null || b === undefined) return -1;
  if (typeof a === "number" && typeof b === "number") return a - b;
  return String(a).localeCompare(String(b), undefined, { numeric: true });
}

export function Table({ block }: { block: TableBlock }) {
  const [sort, setSort] = React.useState<Sort>(null);
  const rows = React.useMemo(() => {
    if (!sort) return block.rows;
    return [...block.rows].sort((a, b) => {
      const order = compare(a.cells[sort.key], b.cells[sort.key]);
      // Empty cells stay last in both directions.
      return a.cells[sort.key] === null || b.cells[sort.key] === null ? order : order * sort.dir;
    });
  }, [block.rows, sort]);
  const numeric = (key: string) => block.rows.some((row) => typeof row.cells[key] === "number");

  const toggle = (key: string) =>
    setSort((current) => (current?.key !== key ? { key, dir: 1 } : current.dir === 1 ? { key, dir: -1 } : null));

  return (
    <div className="-mx-1 max-h-[28rem] overflow-auto px-1">
      <table className="w-full border-collapse text-sm">
        <thead className="sticky top-0 bg-card">
          <tr>
            {block.columns.map((column) => {
              const right = column.align ? column.align === "right" : numeric(column.key);
              const active = sort?.key === column.key;
              return (
                <th
                  key={column.key}
                  scope="col"
                  aria-sort={active ? (sort.dir === 1 ? "ascending" : "descending") : "none"}
                  className={cn("border-b border-border p-0 font-medium text-muted-foreground", right ? "text-right" : "text-left")}
                >
                  <button
                    type="button"
                    onClick={() => toggle(column.key)}
                    className={cn(
                      "inline-flex w-full cursor-pointer items-center gap-1 px-2 py-2 text-xs hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none",
                      right && "justify-end",
                      active && "text-foreground",
                    )}
                  >
                    {column.label}
                    {active && (sort.dir === 1 ? <ArrowUpIcon className="size-3" /> : <ArrowDownIcon className="size-3" />)}
                  </button>
                </th>
              );
            })}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, index) => {
            const action = row.action;
            return (
              <tr
                key={index}
                title={action ? actionHint(action) : undefined}
                tabIndex={action ? 0 : undefined}
                onClick={action ? () => runBlockAction(action) : undefined}
                onKeyDown={
                  action
                    ? (event) => {
                        if (event.key === "Enter") runBlockAction(action);
                      }
                    : undefined
                }
                className={cn(
                  "border-b border-border last:border-b-0",
                  action && "cursor-pointer hover:bg-muted focus-visible:bg-muted focus-visible:outline-none",
                )}
              >
                {block.columns.map((column) => {
                  const value = row.cells[column.key];
                  const right = column.align ? column.align === "right" : numeric(column.key);
                  return (
                    <td
                      key={column.key}
                      className={cn("px-2 py-1.5 align-top", right ? "text-right tabular-nums" : "text-left")}
                    >
                      {formatValue(value, column.format)}
                    </td>
                  );
                })}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
