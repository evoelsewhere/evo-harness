import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { AppNotification } from "@/stores/notification-store";
import { runNotificationAction, runnableActions } from "./open-target";

/** A notification's buttons, under its text on a toast or a bell row (FR-14). */
export function NotificationActions({ notification, className }: { notification: AppNotification; className?: string }) {
  const actions = runnableActions(notification);
  if (actions.length === 0) return null;
  return (
    <div className={cn("flex flex-wrap gap-1.5", className)}>
      {actions.map((action, index) => (
        <Button
          key={action.command}
          size="xs"
          variant={index === 0 ? "default" : "outline"}
          onClick={() => runNotificationAction(notification, action)}
        >
          {action.label}
        </Button>
      ))}
    </div>
  );
}
