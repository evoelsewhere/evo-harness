// Line, bar and area charts in a reply, drawn as SVG by hand and loaded only
// with the first chart (docs/features/rich-replies/spec.md, FR-7, FR-17,
// NFR-2, NFR-3). Colours: DESIGN.md "Data visualisation".

import * as React from "react";
import { Button } from "@/components/ui/button";
import { formatValue } from "@/lib/ui-blocks/format";
import type { ChartBlock, TableBlock } from "@/lib/ui-blocks/types";
import { Table } from "./table";

const HEIGHT = 220;
const DASHES = ["", "5 3", "1.5 3", "7 3 1.5 3"];

const color = (index: number) => (index < 4 ? `var(--block-viz-${index + 1})` : "var(--block-viz-rest)");
const dash = (index: number) => (index < 4 ? DASHES[index] : "");

function niceStep(raw: number): number {
  if (!(raw > 0)) return 1;
  const power = 10 ** Math.floor(Math.log10(raw));
  const fraction = raw / power;
  return (fraction <= 1 ? 1 : fraction <= 2 ? 2 : fraction <= 5 ? 5 : 10) * power;
}

function ticksFor(min: number, max: number): number[] {
  const step = niceStep((max - min) / 4);
  const ticks: number[] = [];
  for (let tick = Math.floor(min / step) * step; tick <= Math.ceil(max / step) * step + step / 2; tick += step) {
    ticks.push(Number(tick.toPrecision(12)));
  }
  return ticks;
}

function Marker({ shape, x, y, stroke }: { shape: number; x: number; y: number; stroke: string }) {
  const common = { fill: "var(--card)", stroke, strokeWidth: 1.5 };
  switch (shape % 4) {
    case 1:
      return <rect x={x - 2.5} y={y - 2.5} width={5} height={5} {...common} />;
    case 2:
      return <path d={`M${x},${y - 3} L${x + 3},${y + 2.5} L${x - 3},${y + 2.5} Z`} {...common} />;
    case 3:
      return <path d={`M${x},${y - 3} L${x + 3},${y} L${x},${y + 3} L${x - 3},${y} Z`} {...common} />;
    default:
      return <circle cx={x} cy={y} r={2.5} {...common} />;
  }
}

function useWidth(ref: React.RefObject<HTMLDivElement | null>): number {
  const [width, setWidth] = React.useState(0);
  React.useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    setWidth(element.clientWidth);
    const observer = new ResizeObserver(([entry]) => setWidth(Math.round(entry.contentRect.width)));
    observer.observe(element);
    return () => observer.disconnect();
  }, [ref]);
  return width;
}

export default function Chart({ block }: { block: ChartBlock }) {
  const box = React.useRef<HTMLDivElement>(null);
  const width = useWidth(box);
  const [hover, setHover] = React.useState<number | null>(null);
  const [showData, setShowData] = React.useState(false);

  const n = block.x.length;
  const stacked = Boolean(block.stacked) && block.kind !== "line";
  // Stacked series sit on the running total of the ones before (negatives as 0).
  const bases = React.useMemo(() => {
    if (!stacked) return null;
    const totals = new Array<number>(n).fill(0);
    return block.series.map((series) =>
      series.values.map((value, index) => {
        const base = totals[index];
        totals[index] += Math.max(value ?? 0, 0);
        return [base, totals[index]] as const;
      }),
    );
  }, [block.series, n, stacked]);

  const all = stacked ? bases!.flat().map(([, top]) => top) : block.series.flatMap((series) => series.values.filter((v): v is number => v !== null));
  let min = Math.min(0, ...all);
  let max = Math.max(0, ...all);
  if (min === max) max = min + 1;
  const ticks = ticksFor(min, max);
  min = ticks[0];
  max = ticks[ticks.length - 1];

  const label = (value: number) => formatValue(value, block.format);
  const left = Math.min(96, Math.max(...ticks.map((tick) => label(tick).length)) * 6.5 + 10);
  const plot = { left, right: 8, top: 8, bottom: 24 };
  const innerW = Math.max(width - plot.left - plot.right, 1);
  const innerH = HEIGHT - plot.top - plot.bottom;
  const y = (value: number) => plot.top + innerH - ((value - min) / (max - min)) * innerH;
  const band = innerW / n;
  const x = (index: number) =>
    block.kind === "bar" ? plot.left + band * (index + 0.5) : plot.left + (n === 1 ? innerW / 2 : (index * innerW) / (n - 1));
  const labelEvery = Math.max(1, Math.ceil(n / Math.max(1, Math.floor(innerW / 64))));

  const onMove = (event: React.PointerEvent<SVGSVGElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    const px = event.clientX - rect.left - plot.left;
    const index = block.kind === "bar" ? Math.floor(px / band) : Math.round((px / innerW) * (n - 1));
    setHover(index >= 0 && index < n ? index : null);
  };

  const summary = `${block.kind} chart of ${block.series.map((series) => series.name).join(", ")} over ${n} points`;
  const dataTable: TableBlock = {
    v: 1,
    type: "table",
    columns: [{ key: "__x", label: "" }, ...block.series.map((series, index) => ({ key: `s${index}`, label: series.name, format: block.format }))],
    rows: block.x.map((value, row) => ({
      cells: Object.fromEntries([["__x", value], ...block.series.map((series, index) => [`s${index}`, series.values[row]])]),
    })),
  };

  return (
    <div className="flex flex-col gap-3">
      {block.series.length > 1 && (
        <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
          {block.series.map((series, index) => (
            <span key={index} className="inline-flex items-center gap-1.5">
              <svg width="18" height="8" aria-hidden>
                <line x1="1" y1="4" x2="17" y2="4" stroke={color(index)} strokeWidth="2" strokeDasharray={dash(index)} />
              </svg>
              {series.name}
            </span>
          ))}
        </div>
      )}
      <div ref={box} className="relative w-full" style={{ height: HEIGHT }}>
        {width > 0 && (
          <svg
            width={width}
            height={HEIGHT}
            role="img"
            aria-label={summary}
            onPointerMove={onMove}
            onPointerLeave={() => setHover(null)}
            className="overflow-visible"
          >
            {ticks.map((tick) => (
              <g key={tick}>
                <line x1={plot.left} x2={width - plot.right} y1={y(tick)} y2={y(tick)} stroke="var(--border)" />
                <text x={plot.left - 6} y={y(tick)} dy="0.32em" textAnchor="end" className="fill-muted-foreground text-[11px] tabular-nums">
                  {label(tick)}
                </text>
              </g>
            ))}
            {block.x.map((value, index) =>
              index % labelEvery === 0 ? (
                <text key={index} x={x(index)} y={HEIGHT - 6} textAnchor="middle" className="fill-muted-foreground text-[11px]">
                  {String(value)}
                </text>
              ) : null,
            )}
            {hover !== null && <line x1={x(hover)} x2={x(hover)} y1={plot.top} y2={plot.top + innerH} stroke="var(--border)" strokeWidth={block.kind === "bar" ? band : 1} opacity={block.kind === "bar" ? 0.6 : 1} />}

            {block.kind === "bar"
              ? block.series.map((series, s) => {
                  const group = band * 0.7;
                  const each = stacked ? group : group / block.series.length;
                  return series.values.map((value, index) => {
                    if (value === null) return null;
                    const [from, to] = bases ? bases[s][index] : [0, value];
                    const top = y(Math.max(from, to));
                    const height = Math.max(Math.abs(y(from) - y(to)), value === 0 ? 0 : 1);
                    const left = x(index) - group / 2 + (stacked ? 0 : s * each);
                    return <rect key={`${s}-${index}`} x={left} y={top} width={Math.max(each - 1, 1)} height={height} rx={2} fill={color(s)} />;
                  });
                })
              : block.series.map((series, s) => {
                  const points = series.values.map((value, index) => {
                    if (value === null) return null;
                    const top = bases ? bases[s][index][1] : value;
                    return [x(index), y(top)] as const;
                  });
                  const segments: string[] = [];
                  let current = "";
                  points.forEach((point) => {
                    if (!point) {
                      if (current) segments.push(current);
                      current = "";
                    } else current += `${current ? "L" : "M"}${point[0]},${point[1]}`;
                  });
                  if (current) segments.push(current);
                  const area =
                    block.kind === "area"
                      ? points
                          .map((point, index) => (point ? [point, bases ? y(bases[s][index][0]) : y(Math.max(min, 0))] : null))
                          .filter((entry): entry is [readonly [number, number], number] => entry !== null)
                      : [];
                  return (
                    <g key={s}>
                      {area.length > 1 && (
                        <path
                          d={`M${area.map(([p]) => `${p[0]},${p[1]}`).join("L")}L${[...area].reverse().map(([p, base]) => `${p[0]},${base}`).join("L")}Z`}
                          fill={color(s)}
                          opacity={0.12}
                        />
                      )}
                      {segments.map((d, index) => (
                        <path key={index} d={d} fill="none" stroke={color(s)} strokeWidth={2} strokeDasharray={dash(s)} strokeLinejoin="round" strokeLinecap="round" />
                      ))}
                      {n <= 40 && points.map((point, index) => (point ? <Marker key={index} shape={s} x={point[0]} y={point[1]} stroke={color(s)} /> : null))}
                    </g>
                  );
                })}
          </svg>
        )}
        {hover !== null && width > 0 && (
          <div
            className="pointer-events-none absolute top-1 z-10 min-w-28 rounded-md bg-popover px-2.5 py-1.5 text-xs shadow-md ring-1 ring-foreground/10"
            style={x(hover) > width / 2 ? { right: width - x(hover) + 8 } : { left: x(hover) + 8 }}
          >
            <div className="mb-1 font-medium">{String(block.x[hover])}</div>
            {block.series.map((series, index) => (
              <div key={index} className="flex items-center justify-between gap-3">
                <span className="inline-flex items-center gap-1.5 text-muted-foreground">
                  <span className="inline-block size-2 rounded-full" style={{ background: color(index) }} />
                  {series.name}
                </span>
                <span className="tabular-nums">{formatValue(series.values[hover], block.format)}</span>
              </div>
            ))}
          </div>
        )}
      </div>
      <div>
        <Button variant="ghost" size="xs" aria-expanded={showData} onClick={() => setShowData((value) => !value)} className="text-muted-foreground">
          {showData ? "Hide data" : "Show data"}
        </Button>
        {showData && (
          <div className="mt-2">
            <Table block={dataTable} />
          </div>
        )}
      </div>
    </div>
  );
}
