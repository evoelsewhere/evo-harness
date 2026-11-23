import * as React from "react";
import {
  BookOpenIcon,
  BotIcon,
  BoxIcon,
  BrainIcon,
  BriefcaseIcon,
  BugIcon,
  Building2Icon,
  CameraIcon,
  CloudIcon,
  CodeIcon,
  CpuIcon,
  DatabaseIcon,
  FlaskConicalIcon,
  FolderIcon,
  Gamepad2Icon,
  GlobeIcon,
  GraduationCapIcon,
  HeartIcon,
  LaptopIcon,
  LayoutDashboardIcon,
  LeafIcon,
  LockIcon,
  type LucideIcon,
  MonitorIcon,
  MusicIcon,
  LayersIcon,
  PaletteIcon,
  PenToolIcon,
  RocketIcon,
  ServerIcon,
  ShieldIcon,
  ShoppingCartIcon,
  SmartphoneIcon,
  StarIcon,
  TerminalIcon,
  WrenchIcon,
  ZapIcon,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { LOCAL_GROUP } from "@/stores/session-store";
import { useWorkspaceIconStore, type ChosenIcon } from "@/stores/workspace-icon-store";
import { initials } from "./sidebar-model";

// A workspace's icon: the one the person chose, else the project's logo,
// else its initials; and the dialog that chooses it
// (docs/features/workspace-rail/spec.md, FR-13 – FR-15, A-8).

export interface IconGroup {
  id: string;
  name: string;
  path?: string;
}

/** The icons a workspace can take, by saved name. */
export const WORKSPACE_ICONS: Record<string, LucideIcon> = {
  code: CodeIcon,
  terminal: TerminalIcon,
  rocket: RocketIcon,
  globe: GlobeIcon,
  database: DatabaseIcon,
  server: ServerIcon,
  cloud: CloudIcon,
  bot: BotIcon,
  brain: BrainIcon,
  cpu: CpuIcon,
  smartphone: SmartphoneIcon,
  monitor: MonitorIcon,
  dashboard: LayoutDashboardIcon,
  palette: PaletteIcon,
  pen: PenToolIcon,
  book: BookOpenIcon,
  school: GraduationCapIcon,
  briefcase: BriefcaseIcon,
  building: Building2Icon,
  cart: ShoppingCartIcon,
  flask: FlaskConicalIcon,
  bug: BugIcon,
  shield: ShieldIcon,
  lock: LockIcon,
  game: Gamepad2Icon,
  music: MusicIcon,
  camera: CameraIcon,
  heart: HeartIcon,
  star: StarIcon,
  zap: ZapIcon,
  leaf: LeafIcon,
  box: BoxIcon,
  layers: LayersIcon,
  wrench: WrenchIcon,
  folder: FolderIcon,
};

/** Look for the project's logo (once per run) and read the chosen icons (once). */
function useIconSources(group: IconGroup) {
  const loaded = useWorkspaceIconStore((s) => s.loaded);
  const chosen = useWorkspaceIconStore((s) => s.chosen[group.id]);
  const logo = useWorkspaceIconStore((s) => s.logos[group.id]);
  React.useEffect(() => {
    if (!loaded) void useWorkspaceIconStore.getState().load();
  }, [loaded]);
  React.useEffect(() => {
    if (group.path) useWorkspaceIconStore.getState().findLogo(group.id, group.path);
  }, [group.id, group.path]);
  return { chosen, logo: logo ?? null };
}

/**
 * The inside of a workspace's icon tile: `md` fills a 32 px rail tile, `sm`
 * sits in a chat row (16 px).
 */
export function WorkspaceIcon({ group, size = "md", className }: { group: IconGroup; size?: "md" | "sm"; className?: string }) {
  const { chosen, logo } = useIconSources(group);
  const small = size === "sm";
  const iconSize = small ? "size-3.5" : "size-[18px]";
  if (group.id === LOCAL_GROUP) return <LaptopIcon className={cn(iconSize, className)} aria-hidden />;
  const Icon = chosen?.kind === "icon" ? WORKSPACE_ICONS[chosen.name] : undefined;
  if (Icon) return <Icon className={cn(iconSize, className)} aria-hidden />;
  if (chosen?.kind === "emoji") {
    return (
      <span aria-hidden className={cn("leading-none", small ? "text-[11px]" : "text-base", className)}>
        {chosen.text}
      </span>
    );
  }
  const useLogo = logo && (!chosen || chosen.kind === "logo");
  if (useLogo) {
    return <img src={logo} alt="" draggable={false} className={cn("object-contain", small ? "size-3.5 rounded-[3px]" : "size-[22px] rounded-md", className)} />;
  }
  return (
    <span aria-hidden className={cn("font-semibold tracking-wide", small ? "text-[10px]" : "text-xs", className)}>
      {initials(group.name)}
    </span>
  );
}

/** Pick a workspace's icon: automatic, its project logo, initials, one of the set, or an emoji (FR-14). */
export function ChangeIconDialog({ group, open, onOpenChange }: { group: IconGroup; open: boolean; onOpenChange(open: boolean): void }) {
  const { chosen, logo } = useIconSources(group);
  const choose = useWorkspaceIconStore((s) => s.choose);
  const [emoji, setEmoji] = React.useState("");
  const [error, setError] = React.useState<string | null>(null);
  React.useEffect(() => {
    if (open) {
      setEmoji(chosen?.kind === "emoji" ? chosen.text : "");
      setError(null);
    }
  }, [open, chosen]);

  const pick = (icon: ChosenIcon | null) => {
    choose(group.id, icon);
    onOpenChange(false);
  };
  const useEmoji = () => {
    const text = emoji.trim();
    // One emoji or up to two letters (A-8).
    if (!text || [...new Intl.Segmenter().segment(text)].length > 2) {
      setError("Enter one emoji or up to two characters.");
      return;
    }
    pick({ kind: "emoji", text });
  };
  const option = (selected: boolean) =>
    cn(
      "flex h-9 items-center justify-center gap-2 rounded-lg px-3 text-sm transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
      selected ? "bg-brand/15 text-brand" : "bg-muted/60 hover:bg-muted",
    );

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Icon for {group.name}</DialogTitle>
          <DialogDescription>Shown on the workspace rail and next to its chats in Needs you.</DialogDescription>
        </DialogHeader>
        <div className="grid grid-cols-3 gap-2">
          <button type="button" className={option(!chosen)} onClick={() => pick(null)}>
            Automatic
          </button>
          {logo && (
            <button type="button" className={option(chosen?.kind === "logo")} onClick={() => pick({ kind: "logo" })}>
              <img src={logo} alt="" className="size-4 object-contain" />
              Logo
            </button>
          )}
          <button type="button" className={option(chosen?.kind === "initials")} onClick={() => pick({ kind: "initials" })}>
            <span className="text-xs font-semibold">{initials(group.name)}</span>
            Initials
          </button>
        </div>
        <div className="grid grid-cols-7 gap-1.5" role="group" aria-label="Icons">
          {Object.entries(WORKSPACE_ICONS).map(([name, Icon]) => (
            <button
              key={name}
              type="button"
              title={name}
              aria-label={name}
              aria-pressed={chosen?.kind === "icon" && chosen.name === name}
              onClick={() => pick({ kind: "icon", name })}
              className={cn(option(chosen?.kind === "icon" && chosen.name === name), "px-0")}
            >
              <Icon className="size-4" />
            </button>
          ))}
        </div>
        <form
          className="flex flex-col gap-1"
          onSubmit={(event) => {
            event.preventDefault();
            useEmoji();
          }}
        >
          <div className="flex gap-2">
            <Input
              value={emoji}
              onChange={(event) => {
                setEmoji(event.target.value);
                setError(null);
              }}
              placeholder="🚀"
              aria-label="Emoji"
              aria-invalid={error ? true : undefined}
              className="w-24"
            />
            <Button type="submit" variant="outline">
              Use emoji
            </Button>
          </div>
          {error && <p className="text-xs text-destructive">{error}</p>}
        </form>
      </DialogContent>
    </Dialog>
  );
}
