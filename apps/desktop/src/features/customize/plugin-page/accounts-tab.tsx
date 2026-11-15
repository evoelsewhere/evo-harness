// A plugin's accounts (docs/features/plugin-settings/spec.md, FR-20 – FR-31):
// each account with its sign-ins — who, state, the access an update asks
// for, default and per-workspace choice, sign out — and Sign in / Log in;
// a key account's Test. Tokens and cookies never reach this page.

import * as React from "react";
import { CheckIcon, KeyRoundIcon, LogInIcon, LogOutIcon, TriangleAlertIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { plugins, type AccountSignIn, type AccountStatus, type InstalledPlugin } from "@/lib/plugins";
import { cn } from "@/lib/utils";
import { useSessionStore } from "@/stores/session-store";
import { Badge } from "../customize-parts";
import { ErrorNote, Section, useBusy } from "./parts";

const KIND: Record<AccountStatus["kind"], string> = {
  oauth: "Signs in in your browser",
  device: "Signs in with a code",
  token: "Uses a key you enter in Settings",
  web: "Logs in on the site in a separate window",
};

function SignInRow({ plugin, account, signIn }: { plugin: InstalledPlugin; account: AccountStatus; signIn: AccountSignIn }) {
  const { busy, error, run } = useBusy();
  const workspaces = useSessionStore((s) => s.workspaces);
  const several = account.signIns.length > 1;
  const needsAgain = signIn.state === "needsSignIn";
  return (
    <li className="flex flex-col gap-1.5 border-t border-border/60 pt-2 first:border-t-0 first:pt-0">
      <div className="flex flex-wrap items-center gap-2 text-[13px]">
        <span className={cn("size-2 shrink-0 rounded-full", needsAgain ? "bg-destructive" : "bg-status-ok")} />
        <span className="min-w-0 truncate font-medium">{signIn.label}</span>
        {signIn.default && several && <Badge>Default</Badge>}
        {needsAgain && <span className="text-[11px] text-destructive">{account.kind === "web" ? "Log in again" : "Sign in again"}</span>}
        <span className="flex-1" />
        {several && !signIn.default && (
          <Button size="xs" variant="ghost" disabled={busy !== null} onClick={() => void run("default", () => plugins.setDefaultAccount(plugin.id, signIn.id))}>
            <CheckIcon /> Make default
          </Button>
        )}
        <Button size="xs" variant="ghost" disabled={busy !== null} onClick={() => void run("out", () => plugins.signOut(plugin.id, signIn.id))}>
          {busy === "out" ? <Spinner className="size-3" /> : <LogOutIcon />} {account.kind === "web" ? "Log out" : "Sign out"}
        </Button>
      </div>
      {signIn.missingScopes.length > 0 && (
        <span className="flex items-center gap-1 text-[11px] text-amber-600 dark:text-amber-400">
          <TriangleAlertIcon className="size-3" />
          Sign in again to grant: {signIn.missingScopes.join(", ")}. Until then it keeps its current access.
        </span>
      )}
      {several && workspaces.length > 0 && (
        <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
          Used in
          {workspaces.map((w) => {
            const mine = signIn.workspaces.includes(w.id);
            return (
              <button
                key={w.id}
                type="button"
                disabled={busy !== null}
                onClick={() => void run("ws", () => plugins.setDefaultAccount(plugin.id, signIn.id, w.id))}
                className={cn("rounded-full border px-2 py-0.5", mine ? "border-brand/40 bg-brand/10 text-foreground" : "border-border/70 hover:bg-muted")}
              >
                {w.name}
              </button>
            );
          })}
        </div>
      )}
      <ErrorNote error={error} />
    </li>
  );
}

function AccountBlock({ plugin, account }: { plugin: InstalledPlugin; account: AccountStatus }) {
  const { busy, error, run } = useBusy();
  const [test, setTest] = React.useState<{ ok: boolean; message: string } | null>(null);
  const signing = account.signingIn;
  const verb = account.kind === "web" ? "Log in" : "Sign in";
  const testable = account.kind === "token" && Boolean(plugin.accounts.find((a) => a.id === account.id)?.method.test);
  return (
    <Section
      title={account.service}
      aside={account.optional ? <span className="text-[11px] text-muted-foreground">Optional</span> : undefined}
    >
      <div className="flex flex-col gap-1">
        {account.reason && <span className="text-[13px]">{account.reason}</span>}
        <span className="text-[11px] text-muted-foreground">{KIND[account.kind]}</span>
      </div>
      {account.kind === "token" ? (
        <div className="flex flex-wrap items-center gap-2 text-[13px]">
          <KeyRoundIcon className="size-3.5 text-muted-foreground" />
          {account.state === "signedIn" ? "Your key is set." : "No key yet: enter it in Settings."}
          <span className="flex-1" />
          {testable && account.state === "signedIn" && (
            <Button size="xs" variant="outline" disabled={busy !== null} onClick={() => void run("test", async () => setTest(await plugins.testAccount(plugin.id, account.id)))}>
              {busy === "test" && <Spinner className="size-3" />} Test
            </Button>
          )}
        </div>
      ) : (
        <>
          {account.signIns.length > 0 ? (
            <ul className="flex flex-col gap-2">
              {account.signIns.map((s) => (
                <SignInRow key={s.id} plugin={plugin} account={account} signIn={s} />
              ))}
            </ul>
          ) : (
            <span className="text-[13px] text-muted-foreground">Not signed in.</span>
          )}
          <div className="flex items-center gap-2">
            {signing ? (
              <>
                <Spinner className="size-3.5" />
                <span className="flex-1 text-xs text-muted-foreground">
                  {signing.flow === "browser" ? "Finish in your browser…" : signing.flow === "window" ? "Log in in the window that opened…" : "Enter the code shown…"}
                </span>
                <Button size="xs" variant="ghost" onClick={() => void plugins.cancelSignIn(plugin.id, account.id)}>
                  Cancel
                </Button>
              </>
            ) : (
              <Button
                size="xs"
                variant={account.signIns.length > 0 ? "outline" : "default"}
                disabled={busy !== null || !plugin.enabled}
                onClick={() => void run("in", () => plugins.signIn(plugin.id, account.id))}
              >
                <LogInIcon /> {account.signIns.length === 0 ? verb : account.state === "needsSignIn" ? `${verb} again` : `${verb} with another account`}
              </Button>
            )}
          </div>
        </>
      )}
      {test && (
        <span className={cn("text-[11px] break-words", test.ok ? "text-status-ok" : "text-destructive")}>
          {test.ok ? "Works" : "Did not work"}
          {test.message ? `: ${test.message}` : ""}
        </span>
      )}
      <ErrorNote error={error} />
    </Section>
  );
}

export function AccountsTab({ plugin }: { plugin: InstalledPlugin }) {
  if (plugin.accountStatus.length === 0) {
    return <p className="text-xs text-muted-foreground">{plugin.name} signs in to no service.</p>;
  }
  const allowed = plugin.approvedPermissions.includes("secrets");
  return (
    <div className="flex flex-col gap-5">
      {!allowed && <p className="text-xs text-muted-foreground">Allow its permissions first: it may keep accounts only with “Use the keys and accounts you give it”.</p>}
      {plugin.accountStatus.map((account) => (
        <AccountBlock key={account.id} plugin={plugin} account={account} />
      ))}
    </div>
  );
}
