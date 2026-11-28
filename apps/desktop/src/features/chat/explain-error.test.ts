// docs/features/ux-polish/spec.md (FR-1).
import { describe, expect, test } from "bun:test";
import { explainError } from "./explain-error";

const anthropic401 =
  'http error 401: {"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"},"request_id":"req_1"}';

describe("explaining a failed turn", () => {
  test("a rejected key names the provider and asks for Providers", () => {
    const e = explainError(anthropic401, "Anthropic");
    expect(e.kind).toBe("auth");
    expect(e.title).toBe("Anthropic rejected the API key.");
    expect(e.detail).toBe(anthropic401);
  });
  test("without a provider name it still reads as a sentence", () => {
    expect(explainError("http error 403: forbidden").title).toBe("The provider rejected the API key.");
  });
  // evo-mates US-3 #3 (qa-2026-11-28, F2): with no provider, the line offers Providers.
  test("no provider connected asks for Providers", () => {
    const e = explainError("No model provider is connected. Open Settings → Providers to add an API key, then send your message again.");
    expect(e.kind).toBe("setup");
    expect(e.title).toBe("Connect a model provider first, then send your message again.");
  });
  test("rate limits, overload, network and context", () => {
    expect(explainError("http error 429: rate_limit_error").kind).toBe("rate");
    expect(explainError('http error 529: {"type":"overloaded_error"}').kind).toBe("unavailable");
    expect(explainError("http error 503: Service Unavailable").kind).toBe("unavailable");
    expect(explainError("http error: http transport error: error sending request for url (https://api.anthropic.com/v1/messages)").kind).toBe("network");
    expect(explainError("prompt is too long: 210000 tokens > 200000 maximum").kind).toBe("context");
  });
  test("anything else keeps its first line, shortened", () => {
    const e = explainError(`${"x".repeat(300)}\nsecond line`);
    expect(e.kind).toBe("other");
    expect(e.title.length).toBeLessThanOrEqual(161);
    expect(e.title.endsWith("…")).toBe(true);
    expect(explainError("Engine exited with code 1").title).toBe("Engine exited with code 1");
  });
});
