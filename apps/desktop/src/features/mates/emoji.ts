// Emoji for Evo Mates' message boxes (docs/features/evo-mates-messaging/spec.md,
// FR-12): a small built-in set by group for the picker, and `:shortcode`
// completion. No download, no outside list.

export interface Emoji {
  e: string;
  /** Its shortcode, without colons. */
  n: string;
  /** More words that find it. */
  k?: string[];
  g: "Smileys" | "People" | "Nature" | "Food" | "Activity" | "Objects" | "Symbols";
}

export const EMOJI: Emoji[] = [
  { e: "😀", n: "grinning", k: ["happy", "smile"], g: "Smileys" },
  { e: "😄", n: "smile", k: ["happy", "joy"], g: "Smileys" },
  { e: "😂", n: "joy", k: ["laugh", "lol", "tears"], g: "Smileys" },
  { e: "🤣", n: "rofl", k: ["laugh"], g: "Smileys" },
  { e: "😊", n: "blush", k: ["smile", "happy"], g: "Smileys" },
  { e: "🙂", n: "slight_smile", k: ["smile"], g: "Smileys" },
  { e: "😉", n: "wink", g: "Smileys" },
  { e: "😍", n: "heart_eyes", k: ["love"], g: "Smileys" },
  { e: "🥰", n: "smiling_hearts", k: ["love"], g: "Smileys" },
  { e: "😎", n: "sunglasses", k: ["cool"], g: "Smileys" },
  { e: "🤔", n: "thinking", k: ["hmm", "think"], g: "Smileys" },
  { e: "🤨", n: "raised_eyebrow", k: ["doubt"], g: "Smileys" },
  { e: "😐", n: "neutral", g: "Smileys" },
  { e: "🙄", n: "eye_roll", g: "Smileys" },
  { e: "😴", n: "sleeping", k: ["tired"], g: "Smileys" },
  { e: "😅", n: "sweat_smile", k: ["phew"], g: "Smileys" },
  { e: "😬", n: "grimace", k: ["oops"], g: "Smileys" },
  { e: "😢", n: "cry", k: ["sad"], g: "Smileys" },
  { e: "😭", n: "sob", k: ["sad", "cry"], g: "Smileys" },
  { e: "😡", n: "angry", k: ["mad"], g: "Smileys" },
  { e: "🤯", n: "mind_blown", k: ["wow"], g: "Smileys" },
  { e: "😱", n: "scream", k: ["shock"], g: "Smileys" },
  { e: "🥳", n: "party", k: ["celebrate"], g: "Smileys" },
  { e: "🤗", n: "hug", g: "Smileys" },
  { e: "🫡", n: "salute", k: ["ok", "on it"], g: "Smileys" },
  { e: "🤖", n: "robot", k: ["bot", "mate"], g: "Smileys" },
  { e: "👍", n: "thumbsup", k: ["like", "yes", "ok", "+1"], g: "People" },
  { e: "👎", n: "thumbsdown", k: ["dislike", "no", "-1"], g: "People" },
  { e: "👏", n: "clap", k: ["bravo"], g: "People" },
  { e: "🙌", n: "raised_hands", k: ["yay"], g: "People" },
  { e: "🙏", n: "pray", k: ["thanks", "please"], g: "People" },
  { e: "👋", n: "wave", k: ["hi", "hello", "bye"], g: "People" },
  { e: "🤝", n: "handshake", k: ["deal"], g: "People" },
  { e: "✌️", n: "v", k: ["peace"], g: "People" },
  { e: "👌", n: "ok_hand", k: ["ok", "perfect"], g: "People" },
  { e: "🤞", n: "crossed_fingers", k: ["luck"], g: "People" },
  { e: "💪", n: "muscle", k: ["strong"], g: "People" },
  { e: "👀", n: "eyes", k: ["look", "watching"], g: "People" },
  { e: "🧠", n: "brain", k: ["smart"], g: "People" },
  { e: "🫶", n: "heart_hands", k: ["love"], g: "People" },
  { e: "🐶", n: "dog", g: "Nature" },
  { e: "🐱", n: "cat", g: "Nature" },
  { e: "🦊", n: "fox", g: "Nature" },
  { e: "🐢", n: "turtle", k: ["slow"], g: "Nature" },
  { e: "🌱", n: "seedling", k: ["grow"], g: "Nature" },
  { e: "🌳", n: "tree", g: "Nature" },
  { e: "🌸", n: "blossom", k: ["flower"], g: "Nature" },
  { e: "☀️", n: "sunny", k: ["sun"], g: "Nature" },
  { e: "🌙", n: "moon", k: ["night"], g: "Nature" },
  { e: "⭐", n: "star", g: "Nature" },
  { e: "🌈", n: "rainbow", g: "Nature" },
  { e: "🔥", n: "fire", k: ["lit", "hot"], g: "Nature" },
  { e: "⚡", n: "zap", k: ["fast", "lightning"], g: "Nature" },
  { e: "❄️", n: "snowflake", k: ["cold"], g: "Nature" },
  { e: "🌊", n: "wave_water", k: ["ocean"], g: "Nature" },
  { e: "☕", n: "coffee", g: "Food" },
  { e: "🍵", n: "tea", g: "Food" },
  { e: "🍕", n: "pizza", g: "Food" },
  { e: "🍔", n: "burger", g: "Food" },
  { e: "🍜", n: "ramen", k: ["noodles", "pho"], g: "Food" },
  { e: "🍰", n: "cake", g: "Food" },
  { e: "🍎", n: "apple", g: "Food" },
  { e: "🍺", n: "beer", g: "Food" },
  { e: "🎉", n: "tada", k: ["party", "celebrate", "congrats"], g: "Activity" },
  { e: "🎊", n: "confetti", g: "Activity" },
  { e: "🏆", n: "trophy", k: ["win"], g: "Activity" },
  { e: "🎯", n: "dart", k: ["target", "goal"], g: "Activity" },
  { e: "🎮", n: "game", g: "Activity" },
  { e: "🎵", n: "music", g: "Activity" },
  { e: "⚽", n: "soccer", k: ["football"], g: "Activity" },
  { e: "🚀", n: "rocket", k: ["launch", "ship"], g: "Objects" },
  { e: "💡", n: "bulb", k: ["idea"], g: "Objects" },
  { e: "📌", n: "pushpin", k: ["pin"], g: "Objects" },
  { e: "📎", n: "paperclip", k: ["attach"], g: "Objects" },
  { e: "📝", n: "memo", k: ["note", "write"], g: "Objects" },
  { e: "📅", n: "calendar", k: ["date"], g: "Objects" },
  { e: "⏰", n: "alarm", k: ["clock", "time"], g: "Objects" },
  { e: "⏳", n: "hourglass", k: ["wait"], g: "Objects" },
  { e: "📈", n: "chart_up", k: ["growth"], g: "Objects" },
  { e: "📉", n: "chart_down", g: "Objects" },
  { e: "📊", n: "bar_chart", k: ["stats"], g: "Objects" },
  { e: "💻", n: "laptop", k: ["computer"], g: "Objects" },
  { e: "📱", n: "phone", g: "Objects" },
  { e: "📧", n: "email", k: ["mail"], g: "Objects" },
  { e: "🔒", n: "lock", k: ["secure"], g: "Objects" },
  { e: "🔑", n: "key", g: "Objects" },
  { e: "🔍", n: "mag", k: ["search", "find"], g: "Objects" },
  { e: "🛠️", n: "tools", k: ["fix", "build"], g: "Objects" },
  { e: "🐛", n: "bug", g: "Objects" },
  { e: "📦", n: "package", k: ["box", "ship"], g: "Objects" },
  { e: "💰", n: "moneybag", k: ["money", "cost"], g: "Objects" },
  { e: "❤️", n: "heart", k: ["love"], g: "Symbols" },
  { e: "💙", n: "blue_heart", g: "Symbols" },
  { e: "💚", n: "green_heart", g: "Symbols" },
  { e: "💯", n: "100", k: ["perfect"], g: "Symbols" },
  { e: "✅", n: "white_check_mark", k: ["done", "check", "yes"], g: "Symbols" },
  { e: "✔️", n: "check", k: ["done"], g: "Symbols" },
  { e: "❌", n: "x", k: ["no", "wrong"], g: "Symbols" },
  { e: "⚠️", n: "warning", k: ["careful"], g: "Symbols" },
  { e: "❓", n: "question", k: ["what"], g: "Symbols" },
  { e: "❗", n: "exclamation", k: ["important"], g: "Symbols" },
  { e: "‼️", n: "bangbang", k: ["emphasis"], g: "Symbols" },
  { e: "➡️", n: "arrow_right", k: ["next"], g: "Symbols" },
  { e: "🔁", n: "repeat", k: ["again"], g: "Symbols" },
  { e: "🆗", n: "ok", g: "Symbols" },
  { e: "🆕", n: "new", g: "Symbols" },
  { e: "🔴", n: "red_circle", g: "Symbols" },
  { e: "🟢", n: "green_circle", g: "Symbols" },
  { e: "✨", n: "sparkles", k: ["magic", "new"], g: "Symbols" },
];

export const GROUPS: Emoji["g"][] = ["Smileys", "People", "Nature", "Food", "Activity", "Objects", "Symbols"];

/** Emoji for `query`: shortcodes that start with it first, then any word that does, then contains. */
export function searchEmoji(query: string, limit = 8): Emoji[] {
  const q = query.trim().toLowerCase().replace(/^:/, "");
  if (!q) return EMOJI.slice(0, limit);
  const starts = EMOJI.filter((x) => x.n.startsWith(q));
  const words = EMOJI.filter((x) => !starts.includes(x) && (x.k ?? []).some((k) => k.startsWith(q)));
  const contains = EMOJI.filter((x) => !starts.includes(x) && !words.includes(x) && x.n.includes(q));
  return [...starts, ...words, ...contains].slice(0, limit);
}

/** `:code:` written out in full becomes its emoji. */
export function replaceShortcodes(text: string): string {
  return text.replace(/:([a-z0-9_+-]{2,}):/gi, (all, code: string) => EMOJI.find((x) => x.n === code.toLowerCase())?.e ?? all);
}
