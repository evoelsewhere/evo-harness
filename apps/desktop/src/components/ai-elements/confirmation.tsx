import { ShieldQuestionIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Item, ItemActions, ItemContent, ItemDescription, ItemMedia, ItemTitle } from "@/components/ui/item";

export interface ConfirmationOption {
  title: string;
  permission?: string;
}

/** Approve or deny a tool before it runs (AI Elements · Confirmation). */
export function Confirmation({
  title,
  description,
  mono = false,
  options,
  onAnswer,
}: {
  title: string;
  description?: string;
  /** Show the description as code (a shell command). */
  mono?: boolean;
  options: ConfirmationOption[];
  onAnswer(option: ConfirmationOption, index: number): void;
}) {
  return (
    <Item variant="outline" className="items-start shadow-hairline">
      <ItemMedia variant="icon">
        <ShieldQuestionIcon />
      </ItemMedia>
      <ItemContent className="min-w-64">
        <ItemTitle className="line-clamp-2 w-auto break-words">{title}</ItemTitle>
        {description && (
          <ItemDescription
            data-selectable
            className={mono ? "line-clamp-none max-h-32 overflow-auto font-mono text-xs break-all whitespace-pre-wrap" : undefined}
          >
            {description}
          </ItemDescription>
        )}
      </ItemContent>
      <ItemActions className="self-center">
        {options.map((option, index) => {
          const rejects = option.permission?.startsWith("reject") || /^(deny|reject|no)\b/i.test(option.title);
          return (
            <Button
              key={`${option.title}-${index}`}
              size="sm"
              variant={rejects ? "outline" : index === 0 ? "default" : "secondary"}
              onClick={() => onAnswer(option, index)}
            >
              {option.title}
            </Button>
          );
        })}
      </ItemActions>
    </Item>
  );
}
