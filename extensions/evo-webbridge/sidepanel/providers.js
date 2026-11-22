// Provider marks as the desktop app draws them (apps/desktop/src/components/provider-icon.tsx):
// a monochrome glyph tinted with the brand colour on a faint tile of the same
// colour; a provider with no mark shows its initials. Keep the table in step
// with the app's.

const BRANDS = {
  anthropic: ["#D4A574", "anthropic"],
  openai: ["#10A37F", "openai"],
  codex: ["#10A37F", "openai"],
  googlegenai: ["#4285F4", "google"],
  vertexai: ["#4285F4", "google"],
  xiaomi: ["#FF6900", "xiaomimimo"],
  deepseek: ["#4D6BFE", "deepseek"],
  openrouter: ["#8B5CF6", "openrouter"],
  qwencloud: ["#6F69F7", "qwen"],
  zai: ["#71717A", "zai"],
  zhipuai: ["#71717A", "zai"],
  kimi: ["#7C3AED", "kimi"],
  moonshot: ["#7C3AED", "kimi"],
  nvidia: ["#76B900", "nvidia"],
  xai: ["#9CA3AF", "xai"],
  copilot: ["#6E40C9", "githubcopilot"],
  ollama: ["#9CA3AF", "ollama"],
  foundry: ["#0078D4", "azureai"],
  bedrock: ["#FF9900", "bedrock"],
};

function initials(label) {
  const words = String(label).replace(/[^\p{L}\p{N} ]/gu, " ").split(/\s+/).filter(Boolean);
  return (words.length > 1 ? words[0][0] + words[1][0] : String(label).slice(0, 2)).toUpperCase();
}

/** A small provider mark (the app's `size="xs"`). */
export function providerIcon(id) {
  const [color, glyph] = BRANDS[String(id || "").toLowerCase()] || ["var(--muted)", null];
  const tile = document.createElement("span");
  tile.className = "provider";
  tile.setAttribute("aria-hidden", "true");
  tile.style.backgroundColor = `color-mix(in srgb, ${color} 14%, transparent)`;
  tile.style.boxShadow = `inset 0 0 0 1px color-mix(in srgb, ${color} 24%, transparent)`;
  if (glyph) {
    const mark = document.createElement("i");
    const url = `url("../icons/providers/${glyph}.svg")`;
    Object.assign(mark.style, { backgroundColor: color, maskImage: url, webkitMaskImage: url });
    tile.append(mark);
  } else {
    tile.append(Object.assign(document.createElement("b"), { textContent: initials(id || "?") }));
    tile.style.color = color;
  }
  return tile;
}
