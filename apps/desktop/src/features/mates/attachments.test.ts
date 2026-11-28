import { describe, expect, test } from "bun:test";
import type { Turn } from "@/features/chat";
import { splitPrompt } from "@/features/chat/prompt-text";
import { channelMessage, isPicture, mentionedFiles, picturesOf, sentFilesOf, splitMessage } from "./attachments";
import { threadRows } from "./thread-rows";

// docs/features/evo-mates/spec.md (FR-81 - FR-83).
const tool = (name: string, over: Record<string, unknown> = {}) => ({
  kind: "tool" as const,
  key: `k-${name}`,
  callId: "c",
  name,
  args: {},
  argsText: "",
  phase: "done" as const,
  output: "",
  startedAt: 0,
  ...over,
});
const sent = (files: { path: string; name: string; size: number }[], caption?: string, isError = false) =>
  tool("send_file", { details: { externalToolResult: { isError, structuredContent: { files, caption } } } });
const turn = (over: Partial<Turn>): Turn => ({ id: "t", prompt: "hi", parts: [], status: "done", startedAt: 1, meta: {}, ...over }) as Turn;

describe("attachments", () => {
  test("pictures are known by their extension", () => {
    expect(["a.PNG", "b.jpeg", "c.webp", "d.gif", "C:/x/e.jpg"].every(isPicture)).toBe(true);
    expect(["a.pdf", "b.docx", "png", "a.png.txt"].some(isPicture)).toBe(false);
  });

  test("a turn's sent files are its send_file results, failed ones left out", () => {
    const t = turn({
      parts: [
        sent([{ path: "C:/w/chart.png", name: "chart.png", size: 10 }], "The chart"),
        sent([{ path: "C:/w/bad.txt", name: "bad.txt", size: 1 }], undefined, true),
        sent([{ path: "C:/w/report.docx", name: "report.docx", size: 20 }]),
      ] as Turn["parts"],
    });
    expect(sentFilesOf(t).map((f) => f.name)).toEqual(["chart.png", "report.docx"]);
  });

  test("a turn's pictures are its tools' images", () => {
    const t = turn({ parts: [tool("browser_screenshot", { images: ["data:image/png;base64,AA"] }), tool("read")] as Turn["parts"] });
    expect(picturesOf(t)).toEqual(["data:image/png;base64,AA"]);
  });

  test("a reply's named files are its local file references, once each", () => {
    expect(mentionedFiles("Done: see `out/report.pdf` and chart.png, and report.pdf again at https://x.io/a.pdf")).toEqual([
      "out/report.pdf",
      "chart.png",
      "report.pdf",
    ]);
  });

  test("the person's bubble keeps files and pasted text apart from the words", () => {
    const rows = threadRows([turn({ prompt: 'look\n<pasted-text lines="2">\na\nb\n</pasted-text>\n@C:/in/notes.pdf' })], false);
    const me = rows.find((r) => r.kind === "bubble" && r.bubble.side === "me");
    expect(me?.kind === "bubble" && [me.bubble.text, me.bubble.files, me.bubble.pasted?.length]).toEqual(["look", ["C:/in/notes.pdf"], 1]);
  });

  test("a turn that sent or showed something gets a files row on the mate's side", () => {
    const rows = threadRows(
      [turn({ parts: [{ kind: "text", key: "x", text: "Here it is", streaming: false }, sent([{ path: "C:/w/a.pdf", name: "a.pdf", size: 3 }])] as Turn["parts"] })],
      false,
    );
    const files = rows.find((r) => r.kind === "files");
    expect(files?.kind === "files" && files.files.map((f) => f.name)).toEqual(["a.pdf"]);
  });
});

describe("channel attachments", () => {
  test("a channel message carries every attachment by path, saving pictures that have none", async () => {
    const saved: string[] = [];
    const text = await channelMessage(
      "see these",
      [
        { kind: "image", name: "Pasted image 1.png", blob: new Blob(["x"]) },
        { kind: "file", name: "a.pdf", path: "C:/in/a.pdf" },
        { kind: "text", name: "pasted", text: "one\ntwo" },
      ],
      async (_blob, name) => {
        saved.push(name);
        return `C:/cache/${name}`;
      },
    );
    expect(saved).toEqual(["Pasted image 1.png"]);
    expect(splitPrompt(text)).toEqual({
      text: "see these",
      pasted: [{ lines: 2, text: "one\ntwo" }],
      files: ["C:/cache/Pasted image 1.png", "C:/in/a.pdf"],
    });
  });
});

describe("splitting a sent message", () => {
  test("an @mention line is words, not an attached file", () => {
    expect(splitMessage("@Writer read the attached picture\n@C:/cache/Pasted image 1.png")).toEqual({
      text: "@Writer read the attached picture",
      pasted: [],
      files: ["C:/cache/Pasted image 1.png"],
    });
    expect(splitMessage("@everyone hi").text).toBe("@everyone hi");
    expect(splitMessage("look\n@notes.pdf").files).toEqual(["notes.pdf"]);
  });
});
