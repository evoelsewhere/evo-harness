// A mate's face (docs/features/evo-mates/spec.md, FR-33; ux.md 6): an
// open-bot-avatar character from Evo's own definitions on a round plate in
// the mate's hue. Still at rest; it moves only while the mate works or under
// the pointer, and never with reduced motion. Lists draw a static SVG; a
// `live` face mounts the animated avatar.

import * as React from "react";
import { renderAvatarSvg, type AvatarState } from "open-bot-avatar";
import { mountAvatar, type MountedAvatar } from "open-bot-avatar/dom";
import { useTheme } from "@/components/theme-provider";
import { cn } from "@/lib/utils";
import { faceById } from "./faces";

/** A mate's face as its profile stores it. */
export interface MateAvatar {
  definition?: string;
  hue?: number;
  material?: "flat" | "fluffy";
  /** A picture the person chose instead of a face. */
  picture?: string;
}

/** What a mate is doing, in the roster's words (ux.md 4). */
export type MateMood = "idle" | "thinking" | "working" | "queued" | "waiting" | "error" | "done";

export function faceStateOf(mood: MateMood | undefined): AvatarState {
  switch (mood) {
    case "thinking":
      return "thinking";
    case "working":
      return "working";
    case "waiting":
      return "waiting";
    case "error":
      return "blocked";
    case "done":
      return "done";
    default:
      return "idle";
  }
}

function hslToHex(h: number, s: number, l: number): string {
  const a = s * Math.min(l, 1 - l);
  const f = (n: number) => {
    const k = (n + h / 30) % 12;
    const c = l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1));
    return Math.round(c * 255)
      .toString(16)
      .padStart(2, "0");
  };
  return `#${f(0)}${f(8)}${f(4)}`;
}

/** The body colour and the plate behind it, for a hue, in each theme. */
export function faceColors(hue: number, dark: boolean): { body: string; plate: string } {
  const h = ((hue % 360) + 360) % 360;
  return dark
    ? { body: hslToHex(h, 0.62, 0.62), plate: hslToHex(h, 0.28, 0.2) }
    : { body: hslToHex(h, 0.68, 0.52), plate: hslToHex(h, 0.7, 0.93) };
}

export function MateFace({
  avatar,
  size = 36,
  mood,
  live = false,
  label,
  className,
}: {
  avatar: MateAvatar | null | undefined;
  size?: number;
  mood?: MateMood;
  /** Animate: the face moves while working, and follows the pointer when hovered. */
  live?: boolean;
  label?: string;
  className?: string;
}) {
  const { resolved } = useTheme();
  const { body, plate } = faceColors(avatar?.hue ?? 210, resolved === "dark");
  const definition = faceById(avatar?.definition);
  const state = faceStateOf(mood);
  const material = avatar?.material ?? "flat";
  const host = React.useRef<HTMLDivElement>(null);
  const mounted = React.useRef<MountedAvatar | null>(null);
  const [hovered, setHovered] = React.useState(false);
  // Moves only while it has a reason to (ux.md 4): working or hovered.
  const moving = live && (mood === "working" || mood === "thinking" || mood === "done" || hovered);

  React.useEffect(() => {
    if (!live || !host.current) return;
    mounted.current = mountAvatar(host.current, {
      definition,
      color: body,
      state,
      material,
      still: !moving,
      followPointer: moving,
      label: "",
    });
    return () => {
      mounted.current?.destroy();
      mounted.current = null;
    };
    // Mounted once per face; options follow below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [live, definition.id]);

  React.useEffect(() => {
    mounted.current?.update({ color: body, state, material, still: !moving, followPointer: moving });
  }, [body, state, material, moving]);

  const svg = React.useMemo(
    () =>
      live
        ? ""
        : // The face's own name is not the mate's: the plate carries the label.
          renderAvatarSvg(definition, { color: body, state, size: Math.round(size * 0.78) }).replace(
            /<title>[\s\S]*?<\/title>/g,
            "",
          ),
    [live, definition, body, state, size],
  );

  const inner = Math.round(size * 0.78);
  return (
    <div
      role="img"
      aria-label={label}
      className={cn("relative inline-flex shrink-0 items-center justify-center overflow-hidden rounded-full", className)}
      style={{ width: size, height: size, background: plate }}
      onPointerEnter={() => setHovered(true)}
      onPointerLeave={() => setHovered(false)}
    >
      {avatar?.picture ? (
        <img src={avatar.picture} alt="" className="size-full object-cover" draggable={false} />
      ) : live ? (
        <div ref={host} style={{ width: inner, height: inner }} />
      ) : (
        // A static SVG drawn by the avatar engine from Evo's own definition.
        <div style={{ width: inner, height: inner }} dangerouslySetInnerHTML={{ __html: svg }} />
      )}
    </div>
  );
}
