// A plugin's settings, drawn from what its manifest describes
// (docs/features/plugin-settings/spec.md, FR-2 – FR-5): personal values, or
// a workspace's own for settings marked per workspace. Secrets are written
// to the plugin's vault and never shown back.

import * as React from "react";
import { FolderOpenIcon, KeyRoundIcon } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { plugins, type InstalledPlugin, type PluginSetting } from "@/lib/plugins";
import { cn } from "@/lib/utils";
import { useSessionStore } from "@/stores/session-store";
import { ErrorNote, Section, useBusy } from "./parts";

const PERSONAL = "__personal";

function Field({
  setting,
  value,
  fallback,
  problem,
  onChange,
}: {
  setting: PluginSetting;
  value: unknown;
  /** What applies when this is left empty (the personal value, or the default). */
  fallback?: unknown;
  problem?: string;
  onChange(value: unknown): void;
}) {
  const text = value === undefined || value === null ? "" : String(value);
  const placeholder = fallback === undefined || fallback === null || fallback === "" ? undefined : String(fallback);
  let control: React.ReactNode;
  switch (setting.kind) {
    case "boolean":
      control = <Switch checked={Boolean(value ?? fallback)} onCheckedChange={(on) => onChange(on)} />;
      break;
    case "multiline":
      control = <Textarea value={text} placeholder={placeholder} rows={3} onChange={(e) => onChange(e.target.value)} />;
      break;
    case "select":
      control = (
        <Select value={text || (placeholder ?? "")} items={setting.options.map((o) => ({ value: o, label: o }))} onValueChange={(next) => onChange(next)}>
          <SelectTrigger className="w-60">
            <SelectValue />
          </SelectTrigger>
          <SelectContent alignItemWithTrigger={false}>
            {setting.options.map((option) => (
              <SelectItem key={option} value={option}>
                {option}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      );
      break;
    case "folder":
    case "file":
      control = (
        <div className="flex gap-2">
          <Input value={text} placeholder={placeholder} onChange={(e) => onChange(e.target.value)} className="font-mono" />
          <Button
            size="sm"
            variant="outline"
            onClick={() =>
              void open({ directory: setting.kind === "folder", multiple: false, title: setting.title }).then((picked) => {
                if (typeof picked === "string") onChange(picked);
              })
            }
          >
            <FolderOpenIcon /> Browse
          </Button>
        </div>
      );
      break;
    default:
      control = (
        <Input
          type={setting.kind === "number" ? "number" : "text"}
          value={text}
          placeholder={placeholder}
          min={setting.kind === "number" ? (setting.min ?? undefined) : undefined}
          max={setting.kind === "number" ? (setting.max ?? undefined) : undefined}
          onChange={(e) => onChange(e.target.value)}
        />
      );
  }
  return (
    <label className="flex flex-col gap-1">
      <span className="text-xs font-medium">
        {setting.title}
        {setting.required && <span className="text-destructive"> *</span>}
      </span>
      {control}
      {problem ? (
        <span className="text-[11px] text-destructive">
          {setting.title} {problem}
        </span>
      ) : (
        setting.description && <span className="text-[11px] text-muted-foreground">{setting.description}</span>
      )}
    </label>
  );
}

/** A secret: set or not, replaced or cleared; its value is never shown (FR-5). */
function SecretField({ plugin, setting, workspace }: { plugin: InstalledPlugin; setting: PluginSetting; workspace: string | null }) {
  const entry = workspace ? `${setting.key}@${workspace}` : setting.key;
  const isSet = plugin.secretsSet.includes(entry);
  const [editing, setEditing] = React.useState(!isSet);
  const [value, setValue] = React.useState("");
  const { busy, error, run } = useBusy();
  React.useEffect(() => setEditing(!isSet), [isSet]);
  return (
    <div className="flex flex-col gap-1">
      <span className="flex items-center gap-1.5 text-xs font-medium">
        <KeyRoundIcon className="size-3.5 text-muted-foreground" />
        {setting.title}
        {setting.required && <span className="text-destructive"> *</span>}
        <span className={cn("ml-1 text-[11px] font-normal", isSet ? "text-status-ok" : "text-muted-foreground")}>{isSet ? "Set" : "Not set"}</span>
      </span>
      {editing ? (
        <div className="flex gap-2">
          <Input
            type="password"
            autoComplete="off"
            value={value}
            placeholder={isSet ? "Enter a new value" : undefined}
            onChange={(e) => setValue(e.target.value)}
            className="font-mono"
          />
          <Button
            size="sm"
            disabled={!value || busy !== null}
            onClick={() =>
              void run("set", async () => {
                await plugins.setSecret(plugin.id, setting.key, value, workspace);
                setValue("");
              })
            }
          >
            Save
          </Button>
          {isSet && (
            <Button size="sm" variant="ghost" onClick={() => setEditing(false)}>
              Cancel
            </Button>
          )}
        </div>
      ) : (
        <div className="flex gap-2">
          <Button size="sm" variant="outline" onClick={() => setEditing(true)}>
            Replace
          </Button>
          <Button size="sm" variant="ghost" disabled={busy !== null} onClick={() => void run("clear", () => plugins.clearSecret(plugin.id, setting.key, workspace))}>
            Clear
          </Button>
        </div>
      )}
      {setting.description && <span className="text-[11px] text-muted-foreground">{setting.description}</span>}
      <span className="text-[11px] text-muted-foreground">Kept in your system keychain; only {plugin.name}'s own code receives it.</span>
      <ErrorNote error={error} />
    </div>
  );
}

export function SettingsForm({ plugin }: { plugin: InstalledPlugin }) {
  const workspaces = useSessionStore((s) => s.workspaces);
  const perWorkspace = plugin.settings.some((s) => s.scope === "workspace");
  const [scope, setScope] = React.useState(PERSONAL);
  const workspace = scope === PERSONAL ? null : scope;
  const saved = workspace ? (plugin.workspaceConfig[workspace] ?? {}) : plugin.config;
  const [values, setValues] = React.useState<Record<string, unknown>>(saved);
  const [problems, setProblems] = React.useState<Record<string, string>>({});
  const { busy, error, run } = useBusy();
  React.useEffect(() => {
    setValues(workspace ? (plugin.workspaceConfig[workspace] ?? {}) : plugin.config);
    setProblems({});
  }, [workspace, plugin.config, plugin.workspaceConfig]);

  if (plugin.settings.length === 0) {
    return <p className="text-xs text-muted-foreground">{plugin.name} has no settings.</p>;
  }
  const shown = plugin.settings.filter((s) => !workspace || s.scope === "workspace");
  const plain = shown.filter((s) => s.kind !== "secret");
  const secrets = shown.filter((s) => s.kind === "secret");
  const groups = [...new Set(plain.map((s) => s.group ?? ""))];
  const dirty = JSON.stringify(values) !== JSON.stringify(saved);
  const setup = plugin.waiting.find((w) => w.part === "setup");

  const save = () =>
    run("save", async () => {
      const result = await plugins.saveSettings(plugin.id, values, workspace);
      setProblems(Object.fromEntries(result.problems.map((p) => [p.key, p.message])));
    });

  return (
    <div className="flex flex-col gap-5">
      {setup && (
        <div className="rounded-lg bg-brand/8 px-3 py-2 text-xs">
          Needs setup: fill in {setup.needs.map((key) => plugin.settings.find((s) => s.key === key)?.title ?? key).join(", ")}. Until then its code does not run.
        </div>
      )}
      {perWorkspace && workspaces.length > 0 && (
        <div className="flex items-center gap-2 text-xs">
          <span className="text-muted-foreground">Values for</span>
          <Select
            value={scope}
            items={[{ value: PERSONAL, label: "You (every workspace)" }, ...workspaces.map((w) => ({ value: w.id, label: w.name }))]}
            onValueChange={(next) => setScope(String(next ?? PERSONAL))}
          >
            <SelectTrigger className="w-60">
              <SelectValue />
            </SelectTrigger>
            <SelectContent alignItemWithTrigger={false}>
              <SelectItem value={PERSONAL}>You (every workspace)</SelectItem>
              {workspaces.map((w) => (
                <SelectItem key={w.id} value={w.id}>
                  {w.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          {workspace && <span className="text-muted-foreground">Only settings marked per workspace; empty ones use yours.</span>}
        </div>
      )}
      {plain.length > 0 &&
        groups.map((group) => (
          <Section key={group || "settings"} title={group || "Settings"}>
            {plain
              .filter((s) => (s.group ?? "") === group)
              .map((setting) => (
                <Field
                  key={setting.key}
                  setting={setting}
                  value={values[setting.key]}
                  fallback={workspace ? (plugin.config[setting.key] ?? setting.default) : setting.default}
                  problem={problems[setting.key]}
                  onChange={(value) => setValues((v) => ({ ...v, [setting.key]: value }))}
                />
              ))}
          </Section>
        ))}
      {plain.length > 0 && (
        <div className="flex items-center justify-end gap-2">
          <ErrorNote error={error} />
          <Button size="sm" disabled={!dirty || busy !== null} onClick={() => void save()}>
            {busy ? "Saving…" : "Save"}
          </Button>
        </div>
      )}
      {secrets.length > 0 && (
        <Section title="Secrets">
          {secrets.map((setting) => (
            <SecretField key={`${setting.key}-${workspace ?? ""}`} plugin={plugin} setting={setting} workspace={workspace} />
          ))}
        </Section>
      )}
    </div>
  );
}
