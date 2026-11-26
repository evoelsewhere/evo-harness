// Making or changing a channel (docs/features/evo-mates/spec.md, FR-14,
// FR-15; ux.md 2): its name, its mates, who answers a message that mentions
// nobody, its lead and what it is about.

import * as React from "react";
import { CheckIcon } from "lucide-react";
import { MateFace } from "@/components/mate-face/mate-face";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import type { Channel, ReplyRule } from "@/lib/mates";
import { cn } from "@/lib/utils";
import { useMatesStore } from "@/stores/mates-store";

export const RULES: { id: ReplyRule; label: string; hint: string }[] = [
  { id: "auto", label: "Auto", hint: "Evo picks the best mate for each message." },
  { id: "lead", label: "Lead", hint: "The lead answers, and hands work to others." },
  { id: "everyone", label: "Everyone", hint: "Every mate answers." },
  { id: "mentioned", label: "Only mentioned", hint: "Only mates you @mention answer." },
];

export function ChannelSettings({
  open,
  onOpenChange,
  channel,
}: {
  open: boolean;
  onOpenChange(open: boolean): void;
  /** Change this channel; without it, make a new one. */
  channel?: Channel;
}) {
  const mates = useMatesStore((s) => s.mates);
  const createChannel = useMatesStore((s) => s.createChannel);
  const updateChannel = useMatesStore((s) => s.updateChannel);
  const [name, setName] = React.useState("");
  const [members, setMembers] = React.useState<string[]>([]);
  const [rule, setRule] = React.useState<ReplyRule>("auto");
  const [lead, setLead] = React.useState<string | null>(null);
  const [instructions, setInstructions] = React.useState("");
  const [saving, setSaving] = React.useState(false);

  React.useEffect(() => {
    if (!open) return;
    setName(channel?.name ?? "");
    setMembers(channel?.members.map((m) => m.mateId) ?? []);
    setRule(channel?.replyRule ?? "auto");
    setLead(channel?.leadMateId ?? null);
    setInstructions(channel?.instructions ?? "");
  }, [open, channel]);

  const toggle = (id: string) =>
    setMembers((current) => (current.includes(id) ? current.filter((m) => m !== id) : [...current, id]));
  const leadShown = lead && members.includes(lead) ? lead : (members[0] ?? null);
  const ready = name.trim().length > 0 && members.length > 0 && !saving;

  const save = async () => {
    if (!ready) return;
    setSaving(true);
    const fields = { name: name.trim(), members, replyRule: rule, leadMateId: leadShown, instructions };
    if (channel) await updateChannel(channel.id, fields);
    else await createChannel(fields);
    setSaving(false);
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md rounded-[22px]">
        <DialogHeader>
          <DialogTitle>{channel ? "Channel settings" : "New channel"}</DialogTitle>
          <DialogDescription>A room where you work with several mates at once.</DialogDescription>
        </DialogHeader>
        <div className="flex max-h-[60vh] flex-col gap-4 overflow-y-auto px-0.5 py-1">
          <label className="flex flex-col gap-1.5">
            <span className="text-xs font-medium text-muted-foreground">Name</span>
            <Input
              autoFocus
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="Launch room"
              className="h-9 rounded-2xl border-transparent bg-muted/50 shadow-none focus-visible:bg-background"
            />
          </label>
          <div className="flex flex-col gap-1.5">
            <span className="text-xs font-medium text-muted-foreground">Mates</span>
            {mates.length === 0 ? (
              <p className="text-xs text-muted-foreground">Add a mate first.</p>
            ) : (
              <div className="grid grid-cols-2 gap-1.5">
                {mates.map((mate) => {
                  const on = members.includes(mate.id);
                  return (
                    <button
                      key={mate.id}
                      type="button"
                      role="checkbox"
                      aria-checked={on}
                      onClick={() => toggle(mate.id)}
                      className={cn(
                        "flex items-center gap-2 rounded-2xl bg-muted/40 px-2.5 py-2 text-left transition-colors hover:bg-muted/80",
                        on && "bg-brand/10 shadow-[0_0_0_1.5px_var(--brand)] hover:bg-brand/10",
                      )}
                    >
                      <MateFace avatar={mate.avatar} size={24} plate={false} label={mate.name} />
                      <span className="min-w-0 flex-1 truncate text-[13px] text-foreground">{mate.name}</span>
                      {on && <CheckIcon className="size-3.5 text-brand" />}
                    </button>
                  );
                })}
              </div>
            )}
          </div>
          <div className="flex flex-col gap-1.5" role="radiogroup" aria-label="Who answers">
            <span className="text-xs font-medium text-muted-foreground">Who answers a message that mentions nobody</span>
            {RULES.map((choice) => (
              <button
                key={choice.id}
                type="button"
                role="radio"
                aria-checked={rule === choice.id}
                onClick={() => setRule(choice.id)}
                className={cn(
                  "flex flex-col items-start rounded-2xl bg-muted/40 px-3.5 py-2 text-left transition-colors hover:bg-muted/80",
                  rule === choice.id && "bg-brand/5 shadow-[0_0_0_1.5px_var(--brand)]",
                )}
              >
                <span className="text-[13px] font-medium text-foreground">{choice.label}</span>
                <span className="text-xs text-muted-foreground">{choice.hint}</span>
              </button>
            ))}
          </div>
          {members.length > 1 && (
            <div className="flex flex-col gap-1.5">
              <span className="text-xs font-medium text-muted-foreground">Lead</span>
              <div className="flex flex-wrap gap-1.5">
                {members.map((id) => {
                  const mate = mates.find((m) => m.id === id);
                  if (!mate) return null;
                  return (
                    <button
                      key={id}
                      type="button"
                      onClick={() => setLead(id)}
                      className={cn(
                        "flex items-center gap-1.5 rounded-full bg-muted/50 py-1 pr-3 pl-1 text-[13px] text-foreground hover:bg-muted",
                        leadShown === id && "bg-brand/10 shadow-[0_0_0_1.5px_var(--brand)]",
                      )}
                    >
                      <MateFace avatar={mate.avatar} size={20} plate={false} label={mate.name} />
                      {mate.name}
                    </button>
                  );
                })}
              </div>
            </div>
          )}
          <label className="flex flex-col gap-1.5">
            <span className="text-xs font-medium text-muted-foreground">What this channel is about</span>
            <Textarea
              value={instructions}
              onChange={(event) => setInstructions(event.target.value)}
              rows={3}
              placeholder="Ship the product page by Friday."
              className="rounded-2xl border-transparent bg-muted/50 shadow-none focus-visible:bg-background"
            />
          </label>
        </div>
        <DialogFooter>
          <Button variant="ghost" className="rounded-full" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            disabled={!ready}
            onClick={() => void save()}
            className="rounded-full bg-brand px-4 text-brand-foreground hover:bg-brand/90"
          >
            {channel ? "Save" : "Create channel"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
