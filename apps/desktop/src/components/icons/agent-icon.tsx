import * as React from "react";
import { renderAvatarSvg, validateAvatar, type AvatarDefinition } from "open-bot-avatar";
import { mountAvatar, type MountedAvatar } from "open-bot-avatar/dom";
import { faceById } from "@/components/mate-face/faces";
import { useTheme } from "@/components/theme-provider";
import { cn } from "@/lib/utils";

// evo's agents wear the Evo Mates faces (docs/features/subagents-and-nested-sessions/spec.md,
// FR-25; docs/features/evo-mates/ux.md 6), so both apps look alike. The
// built-in agents have a face each; any other agent gets one from its name,
// the same every time. A face is drawn by the avatar engine in the agent's
// hue, still at rest; a running agent's face moves (reduced motion keeps it
// still). Where an icon takes the text colour, the face's silhouette is drawn
// in that colour with the eyes cut out.

type Builtin = "general" | "explore" | "plan" | "background";

const builtinFaces: Record<Builtin, string> = {
  general: "pebble",
  explore: "scout",
  plan: "quill",
  background: "buoy",
};

/** Faces for every other agent: none of the built-ins' own. */
const otherFaces = ["gear", "prism", "compass", "spark"];

/** FNV-1a: a stable 32-bit number for a name. */
function hash(name: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < name.length; i++) {
    h ^= name.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

function builtinOf(agentType?: string, background?: boolean): Builtin | null {
  if (background) return "background";
  if (!agentType || agentType === "general" || agentType === "general-purpose") return "general";
  if (agentType === "explore" || agentType === "plan") return agentType;
  return null;
}

/** The agent's face id: fixed for the built-ins, from the name for any other. */
export function agentFaceId(agentType?: string, background?: boolean): string {
  const builtin = builtinOf(agentType, background);
  if (builtin) return builtinFaces[builtin];
  return otherFaces[(hash(agentType!) >>> 8) % otherFaces.length]!;
}

// The engine leaves a margin round a face in its box; an icon draws it 24%
// larger so the body fills the box like the icons beside it.
// At 14–20 px the engine's eyes (about 4 × 11 of 100 units) are under a
// pixel wide: agents' faces are drawn with larger eyes so they still read.
const ICON_EYES = 1.6;
const iconFaces = new Map<string, AvatarDefinition>();

/** The face as an agent icon draws it: the mate face, eyes larger. */
export function agentFace(agentType?: string, background?: boolean): AvatarDefinition {
  const face = faceById(agentFaceId(agentType, background));
  let icon = iconFaces.get(face.id);
  if (!icon) {
    const larger = validateAvatar({ ...face, eyes: { ...face.eyes, scale: face.eyes.scale * ICON_EYES } });
    icon = larger.ok ? larger.value : face;
    iconFaces.set(face.id, icon);
  }
  return icon;
}

// Each agent has a colour of its own, so it is known at a glance wherever it
// shows: a hue on a fixed lightness and chroma that reads on both themes.
const builtinHues: Record<Builtin, number> = { general: 250, explore: 165, plan: 70, background: 295 };
const hues = [20, 45, 95, 135, 185, 210, 235, 270, 320, 345];

/** The agent's hue (OKLCH degrees): fixed for the built-ins, from the name for any other. */
export function agentHue(agentType?: string, background?: boolean): number {
  const builtin = builtinOf(agentType, background);
  if (builtin) return builtinHues[builtin];
  return hues[(hash(agentType!) >>> 20) % hues.length]!;
}

/** Sets `--agent-hue` for the agent; pair it with `agentTint` classes. */
export function agentHueStyle(agentType?: string, background?: boolean): React.CSSProperties {
  return { "--agent-hue": agentHue(agentType, background) } as React.CSSProperties;
}

/** Text and soft background in the agent's colour, read from `--agent-hue`. */
export const agentTint = {
  text: "text-[oklch(0.6_0.16_var(--agent-hue))] dark:text-[oklch(0.75_0.14_var(--agent-hue))]",
  bg: "bg-[oklch(0.6_0.16_var(--agent-hue)/0.13)] dark:bg-[oklch(0.75_0.14_var(--agent-hue)/0.16)]",
};

/** OKLCH to #rrggbb, clipped to sRGB: the engine takes hex colours only. */
export function oklchToHex(l: number, c: number, hue: number): string {
  const h = (hue * Math.PI) / 180;
  const a = c * Math.cos(h);
  const b = c * Math.sin(h);
  const l_ = (l + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m_ = (l - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s_ = (l - 0.0894841775 * a - 1.291485548 * b) ** 3;
  const linear = [
    4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_,
    -1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_,
    -0.0041960863 * l_ - 0.7034186147 * m_ + 1.707614701 * s_,
  ];
  return `#${linear
    .map((v) => {
      const x = Math.min(1, Math.max(0, v));
      const srgb = x <= 0.0031308 ? 12.92 * x : 1.055 * x ** (1 / 2.4) - 0.055;
      return Math.round(srgb * 255)
        .toString(16)
        .padStart(2, "0");
    })
    .join("")}`;
}

/** The face's body colour: the agent's hue at `agentTint.text`'s lightness and chroma. */
export function agentColor(agentType?: string, background?: boolean, dark = false): string {
  const hue = agentHue(agentType, background);
  return dark ? oklchToHex(0.75, 0.14, hue) : oklchToHex(0.6, 0.16, hue);
}

// A static face is a sizeable SVG; rows of the same agent share one.
const staticSvgs = new Map<string, string>();

/** The agent's face at rest, as SVG markup in its colour for the theme. */
export function agentFaceSvg(agentType?: string, background?: boolean, dark = false): string {
  return staticSvg(agentFace(agentType, background), agentColor(agentType, background, dark));
}

function staticSvg(face: AvatarDefinition, color: string): string {
  const key = `${face.id}|${color}`;
  let svg = staticSvgs.get(key);
  if (!svg) {
    // Fluid (no size): the icon's box sizes it, inline so that no `[&_svg]:size-*`
    // of a parent shrinks it. The face's own name is not the agent's.
    svg = renderAvatarSvg(face, { color })
      .replace(/<title>[\s\S]*?<\/title>/g, "")
      .replace(/^<svg /, '<svg style="width:100%;height:100%;display:block" ');
    staticSvgs.set(key, svg);
  }
  return svg;
}

/** The face's silhouette in the text colour, eyes cut out. */
function Silhouette({ face, className, style }: { face: AvatarDefinition; className?: string; style?: React.CSSProperties }) {
  const id = `agent-${React.useId().replace(/[^a-zA-Z0-9_-]/g, "")}`;
  const { x, y, spacing } = face.eyes;
  // Larger than the engine's at any scale: at 14 px an eye is still a mark.
  const w = 9;
  const h = 20;
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 100 100"
      width="24"
      height="24"
      fill="currentColor"
      aria-hidden="true"
      className={cn("shrink-0", className)}
      style={style}
    >
      <mask id={id}>
        <path d={face.body.path} fill="white" />
        <rect x={x - spacing / 2 - w / 2} y={y - h / 2} width={w} height={h} rx={w / 2} fill="black" />
        <rect x={x + spacing / 2 - w / 2} y={y - h / 2} width={w} height={h} rx={w / 2} fill="black" />
      </mask>
      <rect width="100" height="100" mask={`url(#${id})`} />
    </svg>
  );
}

/** A running face: the engine mounted in the page, in its `working` state. */
function LiveFace({ face, color }: { face: AvatarDefinition; color: string }) {
  const host = React.useRef<HTMLDivElement>(null);
  const mounted = React.useRef<MountedAvatar | null>(null);

  React.useEffect(() => {
    if (!host.current) return;
    mounted.current = mountAvatar(host.current, { definition: face, color, state: "working", label: "" });
    return () => {
      mounted.current?.destroy();
      mounted.current = null;
    };
    // Mounted once per face; the colour follows below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [face]);

  React.useEffect(() => {
    mounted.current?.update({ color });
  }, [color]);

  return <div ref={host} className="absolute -inset-[12%]" />;
}

export function AgentIcon({
  agentType,
  background,
  live,
  tinted,
  className,
  style,
}: {
  className?: string;
  style?: React.CSSProperties;
  /** Drawn in the agent's own colour instead of the text colour. */
  tinted?: boolean;
  /** `general`, `explore`, `plan` have their own faces; any other name gets one of its own. */
  agentType?: string;
  /** A background agent's face. */
  background?: boolean;
  /** Running: the face moves. */
  live?: boolean;
}) {
  const { resolved } = useTheme();
  const face = agentFace(agentType, background);
  if (!tinted) return <Silhouette face={face} className={className} style={style} />;
  const color = agentColor(agentType, background, resolved === "dark");
  return (
    <span
      aria-hidden="true"
      data-agent-face={face.id}
      className={cn("relative inline-block size-4 shrink-0", className)}
      style={{ ...agentHueStyle(agentType, background), ...style }}
    >
      {live ? (
        <LiveFace face={face} color={color} />
      ) : (
        <span className="absolute -inset-[12%]" dangerouslySetInnerHTML={{ __html: staticSvg(face, color) }} />
      )}
    </span>
  );
}
