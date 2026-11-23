import * as React from "react";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { ColorPicker } from "../color-picker";
import { Radio } from "@base-ui/react/radio";
import { RadioGroup } from "@base-ui/react/radio-group";
import { useShallow } from "zustand/react/shallow";
import {
  CheckIcon,
  MonitorIcon,
  MoonIcon,
  PaletteIcon,
  PipetteIcon,
  RotateCcwIcon,
  SunIcon,
  type LucideIcon,
} from "lucide-react";
import { useTheme, type Theme } from "@/components/theme-provider";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { setZoom } from "@/features/shell/app-commands";
import { cn } from "@/lib/utils";
import {
  ACCENTS,
  APPEARANCE_DEFAULTS,
  CHAT_WIDTHS,
  CODE_FONTS,
  CUSTOM_ACCENT,
  CUSTOM_BACKGROUND,
  DARK_BACKGROUNDS,
  INTERFACE_FONTS,
  LIGHT_BACKGROUNDS,
  accentColors,
  nativeMaterial,
  shownBackground,
  useAppearanceStore,
  type ChatWidth,
  type MotionPreference,
  type SchemeMode,
} from "@/stores/appearance-store";
import { useUiStore } from "@/stores/ui-store";
import {
  ChoiceCard,
  ChoiceCards,
  Segmented,
  SettingsGroup,
  SettingsHeader,
  SettingsRow,
  StepSlider,
} from "../settings-controls";

const SCHEMES: { value: Theme; label: string; icon: LucideIcon }[] = [
  { value: "system", label: "System", icon: MonitorIcon },
  { value: "light", label: "Light", icon: SunIcon },
  { value: "dark", label: "Dark", icon: MoonIcon },
];

/** Interface size stops: the window zoom levels between 80% and 150%. */
const SIZE_STOPS = [0.8, 0.9, 1, 1.1, 1.25, 1.5];

const MOTION: { value: MotionPreference; label: string }[] = [
  { value: "system", label: "System" },
  { value: "reduced", label: "Reduced" },
  { value: "full", label: "Full" },
];

const percent = (value: number) => `${Math.round(value * 100)}%`;

/** The window and card colours of a scheme's picture, from its background. */
function previewSurfaces(mode: SchemeMode, background: string) {
  return mode === "light"
    ? { window: `color-mix(in oklab, ${background} 96%, #000000)`, card: `color-mix(in oklab, ${background} 30%, #ffffff)` }
    : { window: background, card: `color-mix(in oklab, ${background} 93%, #ffffff)` };
}

/** A small picture of the app in one colour scheme, for the scheme cards. */
function SchemePreview({ scheme }: { scheme: "light" | "dark" | "split" }) {
  const backgrounds = useAppearanceStore(
    useShallow((s) => ({ light: shownBackground(s, "light"), dark: shownBackground(s, "dark") })),
  );
  const pane = (dark: boolean) => {
    const surfaces = previewSurfaces(dark ? "dark" : "light", dark ? backgrounds.dark : backgrounds.light);
    return (
      <div className="flex h-full flex-1 gap-1 p-1.5" style={{ background: surfaces.window }}>
        <div className="flex w-1/3 flex-col gap-1">
          <div className={cn("h-1 w-3/4 rounded-full", dark ? "bg-white/25" : "bg-black/20")} />
          <div className={cn("h-1 w-1/2 rounded-full", dark ? "bg-white/15" : "bg-black/10")} />
          <div className={cn("h-1 w-2/3 rounded-full", dark ? "bg-white/15" : "bg-black/10")} />
        </div>
        <div className="flex flex-1 flex-col justify-end gap-1 rounded-sm p-1" style={{ background: surfaces.card }}>
          <div className={cn("h-1 w-4/5 rounded-full", dark ? "bg-white/20" : "bg-black/15")} />
          <div className="h-1.5 w-1/3 self-end rounded-full bg-brand" />
        </div>
      </div>
    );
  };
  return (
    <div className="flex h-16 w-full overflow-hidden rounded-md shadow-hairline">
      {scheme === "split" ? (
        <>
          {pane(false)}
          {pane(true)}
        </>
      ) : (
        pane(scheme === "dark")
      )}
    </div>
  );
}

function ColorScheme() {
  const { theme, setTheme } = useTheme();
  return (
    <SettingsRow
      labelId="scheme-label"
      label="Color scheme"
      description="Follow the system setting or pin the app to light or dark."
    >
      <RadioGroup
        value={theme}
        onValueChange={(value) => setTheme(value as Theme)}
        aria-labelledby="scheme-label"
        className="grid grid-cols-3 gap-3"
      >
        {SCHEMES.map(({ value, label, icon: Icon }) => (
          <Radio.Root
            key={value}
            value={value}
            className={cn(
              "group/scheme flex flex-col gap-2 rounded-lg p-1.5 text-left outline-none transition-colors",
              "hover:bg-accent/60 data-checked:bg-brand/5",
              "focus-visible:shadow-[0_0_0_2px_var(--background),0_0_0_4px_var(--ring)]",
            )}
          >
            <div className="rounded-md ring-offset-2 ring-offset-card group-data-checked/scheme:ring-2 group-data-checked/scheme:ring-brand">
              <SchemePreview scheme={value === "system" ? "split" : value} />
            </div>
            <span className="flex items-center gap-1.5 px-0.5 text-[13px] font-medium text-muted-foreground group-data-checked/scheme:text-foreground">
              <Icon className="size-3.5" />
              {label}
            </span>
          </Radio.Root>
        ))}
      </RadioGroup>
    </SettingsRow>
  );
}

function AccentPicker() {
  const accent = useAppearanceStore((s) => s.accent);
  const customAccent = useAppearanceStore((s) => s.customAccent);
  const setAccent = useAppearanceStore((s) => s.setAccent);
  const setCustomAccent = useAppearanceStore((s) => s.setCustomAccent);
  const { resolved } = useTheme();
  const [pickerOpen, setPickerOpen] = React.useState(false);
  const current = accent === CUSTOM_ACCENT ? "Custom" : (ACCENTS.find((a) => a.id === accent)?.label ?? "Blue");

  const swatch = cn(
    "relative flex size-7 items-center justify-center rounded-full outline-none transition-transform",
    "shadow-[inset_0_0_0_1px_rgb(0_0_0/0.12)] hover:scale-110",
    "data-checked:shadow-[0_0_0_2px_var(--card),0_0_0_4px_var(--swatch)]",
    "focus-visible:shadow-[0_0_0_2px_var(--card),0_0_0_4px_var(--ring)]",
  );

  return (
    <SettingsRow
      labelId="accent-label"
      label="Accent"
      description="Selected items, active tabs and panels, the send button, links, focus rings, text selection, check marks and progress. Other buttons stay neutral."
      control={<span className="font-mono text-xs text-muted-foreground">{current}</span>}
    >
      <RadioGroup
        value={accent}
        onValueChange={(value) => {
          if (value === CUSTOM_ACCENT) setPickerOpen(true);
          setAccent(value as string);
        }}
        aria-labelledby="accent-label"
        className="flex flex-wrap items-center gap-2.5"
      >
        {ACCENTS.map((option) => {
          const color = resolved === "dark" ? option.dark : option.light;
          return (
            <Radio.Root
              key={option.id}
              value={option.id}
              aria-label={option.label}
              title={option.label}
              className={swatch}
              style={{ background: color, "--swatch": color } as React.CSSProperties}
            >
              {accent === option.id && (
                <CheckIcon
                  className={cn("size-3.5", option.id === "graphite" ? "text-background" : "text-white")}
                  strokeWidth={3}
                />
              )}
            </Radio.Root>
          );
        })}
        <Radio.Root
          value={CUSTOM_ACCENT}
          aria-label="Custom colour"
          title="Custom colour"
          onClick={() => setPickerOpen(true)}
          className={swatch}
          style={
            {
              background:
                accent === CUSTOM_ACCENT
                  ? customAccent
                  : "conic-gradient(from 180deg, #e5484d, #ff990a, #6cda75, #3ec5e0, #3291ff, #bf89ec, #e5484d)",
              "--swatch": accent === CUSTOM_ACCENT ? customAccent : "var(--muted-foreground)",
            } as React.CSSProperties
          }
        >
          <PipetteIcon className="size-3.5 text-white drop-shadow-[0_1px_1px_rgb(0_0_0/0.5)]" strokeWidth={2.5} />
        </Radio.Root>
        <Popover open={pickerOpen} onOpenChange={setPickerOpen}>
          <PopoverTrigger aria-hidden tabIndex={-1} className="pointer-events-none -ml-2.5 size-0 opacity-0" />
          <PopoverContent align="start" className="w-auto p-3">
            <ColorPicker value={customAccent} onChange={setCustomAccent} />
          </PopoverContent>
        </Popover>
      </RadioGroup>
      <AccentSample />
    </SettingsRow>
  );
}

const BACKGROUND_MODES: { mode: SchemeMode; label: string; icon: LucideIcon }[] = [
  { mode: "light", label: "Light", icon: SunIcon },
  { mode: "dark", label: "Dark", icon: MoonIcon },
];

/** The app's base colour in each scheme: presets or a custom colour. */
function BackgroundPicker() {
  return (
    <SettingsRow
      label="Background"
      description="The base colour of the app in each scheme. Cards, menus, fills and the sidebar follow it; text keeps its colours."
    >
      <div className="flex flex-col gap-3">
        {BACKGROUND_MODES.map((item) => (
          <BackgroundRow key={item.mode} {...item} />
        ))}
      </div>
    </SettingsRow>
  );
}

/** Mica (Windows 11) or vibrancy (macOS) behind the sidebar, or the picked background. */
function Translucency() {
  const translucency = useAppearanceStore((s) => s.translucency);
  const setTranslucency = useAppearanceStore((s) => s.setTranslucency);
  const native = nativeMaterial();
  const available = native === "mica" || native === "vibrancy";
  const material = native === "vibrancy" ? "vibrancy" : "Mica";
  return (
    <SettingsRow
      labelId="translucency-label"
      label="Window translucency"
      description={
        available
          ? `Let the desktop show through the sidebar (${material}). Off, the sidebar and window use the background above.`
          : "This system has no window material; the sidebar uses the background above."
      }
      control={
        <Switch
          aria-labelledby="translucency-label"
          checked={available && translucency}
          disabled={!available}
          onCheckedChange={setTranslucency}
        />
      }
    />
  );
}

function BackgroundRow({ mode, label, icon: Icon }: { mode: SchemeMode; label: string; icon: LucideIcon }) {
  const selected = useAppearanceStore((s) => (mode === "light" ? s.backgroundLight : s.backgroundDark));
  const custom = useAppearanceStore((s) => (mode === "light" ? s.customBackgroundLight : s.customBackgroundDark));
  const setBackground = useAppearanceStore((s) => s.setBackground);
  const setCustomBackground = useAppearanceStore((s) => s.setCustomBackground);
  const [pickerOpen, setPickerOpen] = React.useState(false);
  const presets = mode === "light" ? LIGHT_BACKGROUNDS : DARK_BACKGROUNDS;
  const current =
    selected === CUSTOM_BACKGROUND ? "Custom" : (presets.find((p) => p.id === selected)?.label ?? "Neutral");
  const labelId = `background-${mode}-label`;
  // A swatch is a tiny window: the background with a card on it.
  const swatch = cn(
    "relative flex size-9 items-end justify-end overflow-hidden rounded-lg p-1 outline-none transition-transform",
    "shadow-[inset_0_0_0_1px_rgb(0_0_0/0.12)] hover:scale-105 dark:shadow-[inset_0_0_0_1px_rgb(255_255_255/0.14)]",
    "data-checked:shadow-[0_0_0_2px_var(--card),0_0_0_4px_var(--brand)]",
    "focus-visible:shadow-[0_0_0_2px_var(--card),0_0_0_4px_var(--ring)]",
  );
  const ink = mode === "light" ? "text-[#171717]" : "text-white";

  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
      <span id={labelId} className="flex w-14 shrink-0 items-center gap-1.5 text-[13px] text-muted-foreground">
        <Icon className="size-3.5" />
        {label}
      </span>
      <RadioGroup
        value={selected}
        onValueChange={(value) => {
          if (value === CUSTOM_BACKGROUND) setPickerOpen(true);
          setBackground(mode, value as string);
        }}
        aria-labelledby={labelId}
        className="flex flex-wrap items-center gap-2.5"
      >
        {presets.map((preset) => (
          <Radio.Root
            key={preset.id}
            value={preset.id}
            aria-label={`${label}: ${preset.label}`}
            title={preset.label}
            className={swatch}
            style={{ background: preset.color }}
          >
            {selected === preset.id && <CheckIcon className={cn("absolute top-1 left-1 size-3", ink)} strokeWidth={3} />}
            <span className="h-3 w-4 rounded-[3px]" style={{ background: previewSurfaces(mode, preset.color).card }} />
          </Radio.Root>
        ))}
        <Radio.Root
          value={CUSTOM_BACKGROUND}
          aria-label={`${label}: custom colour`}
          title="Custom colour"
          onClick={() => setPickerOpen(true)}
          className={cn(swatch, "items-center justify-center")}
          style={{
            background:
              selected === CUSTOM_BACKGROUND
                ? custom
                : mode === "light"
                  ? "conic-gradient(from 180deg, #f7e3e3, #f2e6cf, #e3eedd, #dfe7f2, #e9e2f7, #f7e3e3)"
                  : "conic-gradient(from 180deg, #2a1d33, #2e2418, #16281e, #12262e, #141c33, #2a1d33)",
          }}
        >
          <PipetteIcon className={cn("size-3.5", ink)} strokeWidth={2.5} />
        </Radio.Root>
        <Popover open={pickerOpen} onOpenChange={setPickerOpen}>
          <PopoverTrigger aria-hidden tabIndex={-1} className="pointer-events-none -ml-2.5 size-0 opacity-0" />
          <PopoverContent align="start" className="w-auto p-3">
            <ColorPicker value={custom} onChange={(hex) => setCustomBackground(mode, hex)} />
          </PopoverContent>
        </Popover>
      </RadioGroup>
      <span className="ml-auto font-mono text-xs text-muted-foreground">
        {current}
        {selected === CUSTOM_BACKGROUND && <span className="uppercase"> · {custom}</span>}
      </span>
    </div>
  );
}

/** The accent applied to the things it colours. */
function AccentSample() {
  const accent = useAppearanceStore((s) => s.accent);
  const customAccent = useAppearanceStore((s) => s.customAccent);
  const colors = accentColors({ accent, customAccent });
  return (
    <div className="mt-4 flex flex-wrap items-center gap-x-5 gap-y-3 rounded-lg bg-muted/60 px-4 py-3 text-[13px] shadow-hairline">
      <span className="inline-flex items-center gap-1.5">
        <CheckIcon className="size-3.5 text-brand" strokeWidth={2.5} />
        Selected
      </span>
      <span className="h-1.5 w-16 overflow-hidden rounded-full bg-muted shadow-hairline">
        <span className="block h-full w-2/3 bg-brand" />
      </span>
      <span className="text-link underline-offset-2 hover:underline">A link to docs</span>
      <span className="inline-flex h-7 items-center rounded-md bg-background px-3 shadow-[0_0_0_2px_var(--muted),0_0_0_4px_var(--ring)]">
        Focused
      </span>
      <span>
        Some <span className="rounded-[2px] bg-brand/25 px-0.5">selected</span> text
      </span>
      <span className="ml-auto font-mono text-[11px] text-muted-foreground uppercase">
        {colors.light === colors.dark ? colors.light : `${colors.light} · ${colors.dark}`}
      </span>
    </div>
  );
}

function Typography() {
  const interfaceFont = useAppearanceStore((s) => s.interfaceFont);
  const codeFont = useAppearanceStore((s) => s.codeFont);
  const setInterfaceFont = useAppearanceStore((s) => s.setInterfaceFont);
  const setCodeFont = useAppearanceStore((s) => s.setCodeFont);
  const zoom = useUiStore((s) => s.zoom);

  return (
    <SettingsGroup title="Typography">
      <SettingsRow
        labelId="font-label"
        label="Interface font"
        description="Applies to every surface except code, which keeps the code font."
      >
        <ChoiceCards
          value={interfaceFont}
          onValueChange={setInterfaceFont}
          labelledBy="font-label"
          className="grid-cols-1 sm:grid-cols-3"
        >
          {INTERFACE_FONTS.map((font) => (
            <ChoiceCard
              key={font.id}
              value={font.id}
              preview="Aa"
              title={font.label}
              description={font.description}
              previewStyle={{ fontFamily: font.stack }}
            />
          ))}
        </ChoiceCards>
      </SettingsRow>

      <SettingsRow
        labelId="code-font-label"
        label="Code font"
        description="Code blocks, diffs, tool output and the terminal."
      >
        <ChoiceCards
          value={codeFont}
          onValueChange={setCodeFont}
          labelledBy="code-font-label"
          className="grid-cols-1 sm:grid-cols-2"
        >
          {CODE_FONTS.map((font) => (
            <ChoiceCard
              key={font.id}
              value={font.id}
              preview="{ }"
              title={font.label}
              description={font.description}
              previewStyle={{ fontFamily: font.stack }}
            />
          ))}
        </ChoiceCards>
      </SettingsRow>

      <SettingsRow
        label="Interface size"
        description={
          <>
            100% is the balanced default. Same as View → Zoom, or <kbd className="font-mono text-xs">Ctrl</kbd> with{" "}
            <kbd className="font-mono text-xs">+</kbd> / <kbd className="font-mono text-xs">−</kbd>.
          </>
        }
        control={<span className="font-mono text-xs text-muted-foreground tabular-nums">{percent(zoom)}</span>}
      >
        <StepSlider
          label="Interface size"
          stops={SIZE_STOPS}
          value={zoom}
          defaultValue={1}
          onValueChange={setZoom}
          format={percent}
        />
      </SettingsRow>
    </SettingsGroup>
  );
}

function Layout() {
  const chatWidth = useAppearanceStore((s) => s.chatWidth);
  const setChatWidth = useAppearanceStore((s) => s.setChatWidth);
  const motion = useAppearanceStore((s) => s.motion);
  const setMotion = useAppearanceStore((s) => s.setMotion);
  const railNames = useAppearanceStore((s) => s.railNames);
  const setRailNames = useAppearanceStore((s) => s.setRailNames);

  return (
    <SettingsGroup title="Layout & motion">
      <SettingsRow
        labelId="rail-names-label"
        label="Workspace names"
        description="Show each workspace's name under its icon on the sidebar's rail. Off, the rail shows icons only and takes less room."
        control={<Switch aria-labelledby="rail-names-label" checked={railNames} onCheckedChange={setRailNames} />}
      />
      <SettingsRow
        labelId="width-label"
        label="Conversation width"
        description="How wide messages and the composer run in the chat column."
        control={
          <Segmented<ChatWidth>
            value={chatWidth}
            onValueChange={setChatWidth}
            labelledBy="width-label"
            options={CHAT_WIDTHS.map((w) => ({ value: w.id, label: w.label }))}
          />
        }
      />
      <SettingsRow
        labelId="motion-label"
        label="Animations"
        description="Panel transitions, menus and list reveals. System follows your operating system's reduced-motion setting."
        control={
          <Segmented<MotionPreference>
            value={motion}
            onValueChange={setMotion}
            labelledBy="motion-label"
            options={MOTION}
          />
        }
      />
    </SettingsGroup>
  );
}

export function AppearanceSettings() {
  const state = useAppearanceStore();
  const zoom = useUiStore((s) => s.zoom);
  const { theme, setTheme } = useTheme();
  const isDefault =
    theme === "system" &&
    zoom === 1 &&
    (Object.keys(APPEARANCE_DEFAULTS) as (keyof typeof APPEARANCE_DEFAULTS)[]).every(
      (key) =>
        key === "customAccent" ||
        key === "customBackgroundLight" ||
        key === "customBackgroundDark" ||
        state[key] === APPEARANCE_DEFAULTS[key],
    );

  function resetAll() {
    state.reset();
    setTheme("system");
    setZoom(1);
  }

  return (
    <div className="flex flex-col gap-8">
      <SettingsHeader
        icon={PaletteIcon}
        title="Appearance"
        description="Colors, backgrounds, type and motion. Changes apply immediately and stay on this machine."
      >
        <Button variant="ghost" size="sm" disabled={isDefault} onClick={resetAll} className="text-muted-foreground">
          <RotateCcwIcon />
          Reset
        </Button>
      </SettingsHeader>

      <SettingsGroup title="Theme">
        <ColorScheme />
        <AccentPicker />
        <BackgroundPicker />
        <Translucency />
      </SettingsGroup>

      <Typography />
      <Layout />
    </div>
  );
}
