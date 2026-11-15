// What a plugin may do (docs/features/plugin-settings/spec.md, FR-8, FR-9,
// FR-15, FR-17; plugin-ui FR-3, FR-16): each permission with the plugin's
// reason, whether it is needed by its parts, whether evo can hold its code
// to it, and Take back / Allow.

import { Button } from "@/components/ui/button";
import { plugins, type InstalledPlugin, type PermissionRow } from "@/lib/plugins";
import { askConsent } from "../plugin-consent";
import { ErrorNote, PermissionNotes, Section, useBusy } from "./parts";

/** The banner of a plugin with permissions still to approve. */
export function ReviewBanner({ plugin }: { plugin: InstalledPlugin }) {
  if (plugin.unapproved.length === 0) return null;
  const asked = plugin.unapproved.map((p) => p.label.toLowerCase()).join(", ");
  const code = plugin.unapproved.some((p) => p.id === "hooks" || p.id === "runtime");
  return (
    <div className="flex items-center gap-2 rounded-lg bg-brand/8 px-3 py-2 text-xs">
      <span className="min-w-0 flex-1">
        {plugin.pending
          ? `Version ${plugin.pending.version} is ready and asks for more: ${asked}.`
          : plugin.enabled && code
            ? `Review what it may do: ${asked}. Until then its code does not run; its skills still work.`
            : plugin.enabled
              ? `Not allowed now: ${asked}. Calls that need it fail.`
              : `Waiting for your approval: ${asked}.`}
      </span>
      <Button size="xs" onClick={() => askConsent({ id: plugin.id, needsConsent: plugin.unapproved, pendingVersion: plugin.pending?.version }, plugin.name)}>
        Review
      </Button>
    </div>
  );
}

export function PermissionsTab({ plugin }: { plugin: InstalledPlugin }) {
  const { busy, error, run } = useBusy();
  if (plugin.permissions.length === 0) {
    return <p className="text-xs text-muted-foreground">{plugin.name} asks for no permissions.</p>;
  }
  const required = plugin.permissions.filter((p) => !p.optional);
  const optional = plugin.permissions.filter((p) => p.optional);
  const row = (permission: PermissionRow) => {
    const allowed = plugin.approvedPermissions.includes(permission.id);
    const declined = permission.optional && plugin.optionalAnswers[permission.id] === false;
    return (
      <li key={permission.id} className="flex items-start gap-3">
        <span className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span className="text-[13px]" title={permission.id}>
            {permission.label}
          </span>
          <PermissionNotes permission={permission} />
        </span>
        <span className="mt-0.5 shrink-0 text-[11px] text-muted-foreground">{allowed ? "Allowed" : declined ? "Declined" : "Not allowed"}</span>
        {allowed ? (
          <Button
            size="xs"
            variant="ghost"
            disabled={busy !== null}
            onClick={() =>
              void run(permission.id, () =>
                permission.optional ? plugins.answerOptional(plugin.id, permission.id, false) : plugins.revoke(plugin.id, permission.id),
              )
            }
          >
            Take back
          </Button>
        ) : permission.optional ? (
          <Button size="xs" variant="outline" disabled={busy !== null} onClick={() => void run(permission.id, () => plugins.answerOptional(plugin.id, permission.id, true))}>
            Allow
          </Button>
        ) : null}
      </li>
    );
  };
  return (
    <div className="flex flex-col gap-5">
      <ReviewBanner plugin={plugin} />
      {required.length > 0 && (
        <Section title="It may">
          <ul className="flex flex-col gap-2.5">{required.map(row)}</ul>
        </Section>
      )}
      {optional.length > 0 && (
        <Section title="Only if you allow it">
          <ul className="flex flex-col gap-2.5">{optional.map(row)}</ul>
        </Section>
      )}
      <ErrorNote error={error} />
    </div>
  );
}
