// Files named in a reply become links (docs/features/rich-replies/spec.md,
// FR-37): a relative markdown link, a path in backticks, or a file name with
// a known extension in the text. Code blocks, web links and web addresses are
// left as they are. The link's target is the reference as written (a path
// relative to the chat's folder, perhaps with `:line`); the reply's view
// resolves and opens it.

/** Links to local files ride through link hardening under this origin. */
export const FILE_LINK = "https://local.link/";
/** A board card's link (`KEY-n`, docs/features/evo-mates-threads-board/spec.md, FR-14). */
export const CARD_LINK = "https://local.card/";
/** A place or a message in Evo Mates (`channel/<id>`, `mate/<id>`, `message/<chat>/<line>`; evo-mates-threads-board FR-17). */
export const APP_LINK = "https://local.app/";

// Extensions a bare file name in prose must end with. Single letters (`.c`)
// are left to paths with a folder, where they cannot be a sentence's end.
const BARE_EXT =
  "md|mdx|markdown|txt|csv|tsv|json|jsonl|ya?ml|toml|ini|cfg|conf|xml|html?|css|scss|less|jsx?|tsx?|mjs|cjs|rs|py|ipynb|go|java|kts?|swift|cc|cpp|hpp|cs|rb|php|sh|bash|zsh|ps1|bat|sql|log|pdf|docx?|xlsx?|xlsm|pptx?|odt|ods|odp|rtf|tex|png|jpe?g|gif|svg|webp|ico|zip|lock|env|vue|svelte|dart|lua|epub|mp3|mp4|wav|mov";
const PREFIX = String.raw`(?:[A-Za-z]:[\\/]|\.{1,2}[\\/]|~[\\/])?`;
const LINE = String.raw`(?::\d+(?::\d+)?)?`;
const SEGMENT = String.raw`[\w@-][\w@.-]*`;
const BARE = new RegExp(
  String.raw`(?<![\w@/\\.:-])(${PREFIX}(?:${SEGMENT}[\\/])*${SEGMENT}\.(?:${BARE_EXT})${LINE})(?![\w/\\-]|\.\w)`,
  "gi",
);
// Library names written like files ("Node.js", "Next.js") are not files.
const LIBRARY = /^[A-Z][a-z]+\.js$/;
// A whole code span that is a path: a folder and a file with an extension, or
// a bare file name with a known one.
const CODE_PATH = new RegExp(String.raw`^${PREFIX}(?:(?:${SEGMENT}[\\/])+${SEGMENT}\.[A-Za-z0-9]{1,8}|${SEGMENT}\.(?:${BARE_EXT}))${LINE}$`, "i");
const SCHEME = /^(?:[a-z][a-z0-9+.-]*:|#|\/\/)/i;

const toLink = (label: string, target: string) => `[${label}](${FILE_LINK}${encodeURIComponent(target)})`;

/** One stretch of prose: links, code spans and web addresses kept, file names linked. */
function linkProse(text: string): string {
  const token = /(\[[^\]\n]*\]\([^)\s]*\))|(`[^`\n]+`)|(<[^>\s]+>)|((?:https?:\/\/|www\.)[^\s)<]+)/g;
  let out = "";
  let last = 0;
  for (const match of text.matchAll(token)) {
    out += linkBare(text.slice(last, match.index));
    const [whole, mdLink, code] = match;
    if (mdLink) {
      const at = mdLink.lastIndexOf("](");
      const href = mdLink.slice(at + 2, -1);
      out += href && !SCHEME.test(href) ? toLink(mdLink.slice(1, at), href) : mdLink;
    } else if (code) {
      const inner = code.slice(1, -1).trim();
      out += CODE_PATH.test(inner) && !LIBRARY.test(inner) ? toLink(code, inner) : code;
    } else {
      out += whole;
    }
    last = (match.index ?? 0) + whole.length;
  }
  return out + linkBare(text.slice(last));
}

function linkBare(text: string): string {
  return text.replace(BARE, (name: string) => (LIBRARY.test(name) ? name : toLink(name, name)));
}

/** The reply's markdown with the files it names as links; fenced code untouched. */
export function linkFiles(markdown: string): string {
  const lines = markdown.split("\n");
  const out: string[] = [];
  let prose: string[] = [];
  let fence: string | null = null;
  const flush = () => {
    if (prose.length) out.push(linkProse(prose.join("\n")));
    prose = [];
  };
  for (const line of lines) {
    const marker = /^\s*(`{3,}|~{3,})/.exec(line)?.[1];
    if (fence) {
      out.push(line);
      if (marker && marker[0] === fence[0] && marker.length >= fence.length) fence = null;
    } else if (marker) {
      flush();
      fence = marker;
      out.push(line);
    } else {
      prose.push(line);
    }
  }
  flush();
  return out.join("\n");
}

/** `src/app.ts:42` → the path and the line; a Windows drive letter is kept. */
export function parseFileRef(reference: string): { path: string; line?: number } {
  const match = /^(.*?):(\d+)(?::\d+)?$/.exec(reference);
  if (match && match[1] && !/^[A-Za-z]$/.test(match[1])) return { path: match[1], line: Number(match[2]) };
  return { path: reference };
}
