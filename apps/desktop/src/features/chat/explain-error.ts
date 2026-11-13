// A failed turn's error in one sentence: what went wrong and what to do,
// with the raw text kept for Details (docs/features/ux-polish/spec.md, FR-1).
// Read from the text the provider or engine already sends; nothing is
// guessed beyond the status code and a few well-known words.

export type ErrorKind = "auth" | "rate" | "unavailable" | "network" | "context" | "other";

export interface ExplainedError {
  kind: ErrorKind;
  /** One sentence for the person. */
  title: string;
  /** The text as it came, for Details. */
  detail: string;
}

const MAX_TITLE = 160;

const status = (text: string): number | null => {
  const m = /\bhttp error (\d{3})\b/i.exec(text) ?? /\bstatus(?: code)?:? (\d{3})\b/i.exec(text);
  return m ? Number(m[1]) : null;
};

export function explainError(text: string, provider?: string): ExplainedError {
  const detail = text;
  const who = provider?.trim() || "The provider";
  const code = status(text);
  const lower = text.toLowerCase();
  if (code === 401 || code === 403 || /authentication_error|invalid x-api-key|invalid api key|permission_error/.test(lower)) {
    return { kind: "auth", title: `${who} rejected the API key.`, detail };
  }
  if (code === 429 || /rate.?limit/.test(lower)) {
    return { kind: "rate", title: `${who} is limiting requests right now. Try again in a moment.`, detail };
  }
  if ((code !== null && (code >= 500 || code === 529)) || /overloaded/.test(lower)) {
    return { kind: "unavailable", title: `${who} is unavailable or overloaded. Try again shortly.`, detail };
  }
  if (/transport error|error sending request|connection (refused|reset|closed)|timed out|dns|network/.test(lower)) {
    return { kind: "network", title: `Could not reach ${provider?.trim() || "the provider"}. Check the connection and try again.`, detail };
  }
  if (/prompt is too long|context.{0,20}(length|window)|maximum context|too many tokens/.test(lower)) {
    return { kind: "context", title: "The conversation is longer than the model can read. Compact it, or start a new chat.", detail };
  }
  const first = text.split("\n")[0].trim();
  const title = first.length > MAX_TITLE ? `${first.slice(0, MAX_TITLE)}…` : first;
  return { kind: "other", title, detail };
}
