import * as React from "react";
import { cn } from "@/lib/utils";

// evo's agents: a soft, rounded body with two eyes cut out of it, so the eyes
// show whatever is behind (light or dark theme alike). The body takes the text
// colour and is shaded as a lit solid: a light from the top left, a highlight,
// edges that turn away into shadow. The built-in agents have a look each; any
// other agent gets a shape, tilt and gaze derived from its name, the same every
// time. While an agent runs it breathes, glances around and blinks now and then.

type Look = {
  body: React.ReactNode;
  eyes: React.ReactNode;
  /** Degrees, around the body's centre. */
  tilt?: number;
  /** Scans side to side while running instead of glancing now and then. */
  scans?: boolean;
};

const ball = <circle cx="12" cy="13" r="10.5" />;

const builtins: Record<"general" | "explore" | "plan" | "background", Look> = {
  // A ball glancing up and to the right.
  general: {
    body: ball,
    eyes: (
      <>
        <rect x="12.9" y="5.6" width="2.6" height="5.4" rx="1.3" transform="rotate(-24 14.2 8.3)" />
        <rect x="16.9" y="5.4" width="2.3" height="4.6" rx="1.15" transform="rotate(-24 18.05 7.7)" />
      </>
    ),
  },
  // Eyes to the side: looking for something.
  explore: {
    body: ball,
    scans: true,
    eyes: (
      <>
        <rect x="15.2" y="9" width="2.6" height="5.2" rx="1.3" transform="rotate(-8 16.5 11.6)" />
        <rect x="19" y="9.2" width="2.2" height="4.4" rx="1.1" transform="rotate(-8 20.1 11.4)" />
      </>
    ),
  },
  // Square and steady, looking straight at you.
  plan: {
    body: <rect x="2" y="3" width="20" height="20" rx="7" />,
    eyes: (
      <>
        <rect x="8.2" y="8.4" width="2.6" height="5.4" rx="1.3" />
        <rect x="13.2" y="8.4" width="2.6" height="5.4" rx="1.3" />
      </>
    ),
  },
  // Calm, half-closed eyes: works while you are away.
  background: {
    body: ball,
    eyes: (
      <>
        <rect x="12.4" y="9" width="4.2" height="2.2" rx="1.1" transform="rotate(-12 14.5 10.1)" />
        <rect x="17.4" y="8.2" width="3.6" height="2" rx="1" transform="rotate(-12 19.2 9.2)" />
      </>
    ),
  },
};

// Bodies for every other agent, each with where its eyes sit.
const shapes: { body: React.ReactNode; eyeX: number; eyeY: number }[] = [
  { body: ball, eyeX: 12, eyeY: 12.5 },
  // A soft square cushion.
  {
    body: <path d="M12 3C19.6 3 22 5.4 22 13S19.6 23 12 23 2 20.6 2 13 4.4 3 12 3Z" />,
    eyeX: 12,
    eyeY: 12.5,
  },
  // A drop with one sharper corner.
  {
    body: <path d="M5.5 3H13.5A9 9 0 0 1 22.5 12V14A9 9 0 0 1 13.5 23H10.5A9 9 0 0 1 1.5 14V7A4 4 0 0 1 5.5 3Z" />,
    eyeX: 13,
    eyeY: 12.5,
  },
  // A dome on a flat base.
  {
    body: <path d="M2.5 21.8V14A9.5 9.5 0 0 1 21.5 14V21.8Q21.5 23 20.3 23H3.7Q2.5 23 2.5 21.8Z" />,
    eyeX: 12,
    eyeY: 14.5,
  },
  // A tall slab.
  { body: <rect x="3" y="3" width="18" height="19.5" rx="5.5" />, eyeX: 12, eyeY: 12 },
];

const tilts = [-10, -6, 0, 0, 6, 10];
const gazesX = [-2, 0, 0, 2, 3];
const gazesY = [-1.5, 0, 0];

/** FNV-1a: a stable 32-bit number for a name. */
function hash(name: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < name.length; i++) {
    h ^= name.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

/** A custom agent's look, picked from its name. */
function lookFor(name: string): Look {
  const h = hash(name);
  const shape = shapes[h % shapes.length];
  const x = shape.eyeX + gazesX[(h >>> 4) % gazesX.length];
  const y = shape.eyeY + gazesY[(h >>> 8) % gazesY.length];
  const dots = (h >>> 12) % 4 === 0;
  const eyes = dots ? (
    <>
      <circle cx={x - 2.4} cy={y} r="1.9" />
      <circle cx={x + 2.4} cy={y + 0.4} r="1.5" />
    </>
  ) : (
    <>
      <rect x={x - 3.8} y={y - 2.7} width="2.6" height="5.4" rx="1.3" />
      <rect x={x + 1.3} y={y - 2.4} width="2.3" height="4.8" rx="1.15" />
    </>
  );
  return { body: shape.body, eyes, tilt: tilts[(h >>> 16) % tilts.length] };
}

// Each agent has a colour of its own, so it is known at a glance wherever it
// shows: a hue on a fixed lightness and chroma that reads on both themes.
const builtinHues = { general: 250, explore: 165, plan: 70, background: 295 };
const hues = [20, 45, 95, 135, 185, 210, 235, 270, 320, 345];

/** The agent's hue (OKLCH degrees): fixed for the built-ins, from the name for any other. */
export function agentHue(agentType?: string, background?: boolean): number {
  if (background) return builtinHues.background;
  if (!agentType || agentType === "general" || agentType === "general-purpose") return builtinHues.general;
  if (agentType === "explore" || agentType === "plan") return builtinHues[agentType];
  return hues[(hash(agentType) >>> 20) % hues.length];
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

function lookOf(agentType?: string, background?: boolean): Look {
  if (background) return builtins.background;
  if (!agentType || agentType === "general") return builtins.general;
  if (agentType === "explore" || agentType === "plan") return builtins[agentType];
  return lookFor(agentType);
}

export function AgentIcon({
  agentType,
  background,
  live,
  tinted,
  className,
  style,
  ...props
}: React.SVGProps<SVGSVGElement> & {
  /** Drawn in the agent's own colour instead of the text colour. */
  tinted?: boolean;
  /** `general`, `explore`, `plan` have their own looks; any other name gets one of its own. */
  agentType?: string;
  /** A background agent's calm look. */
  background?: boolean;
  /** Running: it breathes, glances around and blinks. */
  live?: boolean;
}) {
  const id = `agent-${React.useId().replace(/[^a-zA-Z0-9_-]/g, "")}`;
  const look = lookOf(agentType, background);
  const tilt = look.tilt ? `rotate(${look.tilt} 12 13)` : undefined;
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 24 24"
      width="24"
      height="24"
      fill="currentColor"
      aria-hidden="true"
      className={cn("shrink-0", className, tinted && agentTint.text)}
      style={tinted ? { ...agentHueStyle(agentType, background), ...style } : style}
      {...props}
    >
      <defs>
        {/* The body shows, the eyes are holes in it. */}
        <mask id={`${id}-body`}>
          <g transform={tilt}>
            <g fill="white">{look.body}</g>
            <g
              className={cn(
                "[transform-box:fill-box]",
                live &&
                  (look.scans
                    ? "motion-safe:animate-[agent-scan_2.8s_ease-in-out_infinite]"
                    : "motion-safe:animate-[agent-glance_6.5s_ease-in-out_infinite]"),
              )}
            >
              <g
                fill="black"
                className={cn(
                  "origin-center [transform-box:fill-box]",
                  live && "motion-safe:animate-[agent-blink_3.6s_ease-in-out_infinite]",
                )}
              >
                {look.eyes}
              </g>
            </g>
          </g>
        </mask>
        {/* The light: lit from the top left, edges falling into shadow. */}
        <radialGradient id={`${id}-shade`} gradientUnits="userSpaceOnUse" cx="10" cy="9" r="15" fx="7.5" fy="6.5">
          <stop offset="0" stopColor="#fff" stopOpacity="0.24" />
          <stop offset="0.45" stopColor="#fff" stopOpacity="0.04" />
          <stop offset="0.7" stopColor="#000" stopOpacity="0" />
          <stop offset="1" stopColor="#000" stopOpacity="0.24" />
        </radialGradient>
        <radialGradient id={`${id}-shine`} gradientUnits="userSpaceOnUse" cx="7.5" cy="7.5" r="5">
          <stop offset="0" stopColor="#fff" stopOpacity="0.3" />
          <stop offset="1" stopColor="#fff" stopOpacity="0" />
        </radialGradient>
      </defs>
      <g
        className={cn(
          "origin-bottom [transform-box:fill-box]",
          live &&
            (background
              ? "motion-safe:animate-[agent-breathe_4s_ease-in-out_infinite]"
              : "motion-safe:animate-[agent-breathe_2.2s_ease-in-out_infinite]"),
        )}
      >
        <g mask={`url(#${id}-body)`}>
          <rect width="24" height="24" fill="currentColor" />
          <rect width="24" height="24" fill={`url(#${id}-shade)`} />
          <rect width="24" height="24" fill={`url(#${id}-shine)`} />
        </g>
      </g>
    </svg>
  );
}
