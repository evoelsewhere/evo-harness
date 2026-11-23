// Run with `bun test` from apps/desktop. The bytes come from `frames::packet`
// (src-tauri/src/frames.rs). Spec: docs/features/floating-cards (FR-9).
import { describe, expect, test } from "bun:test";
import { jpegBlob, readPacket } from "./stage-stream";

/** The bytes the shell sends for `header` and `image`. */
function packet(header: unknown, image: number[]): ArrayBuffer {
  const json = new TextEncoder().encode(JSON.stringify(header));
  const bytes = new Uint8Array(4 + json.length + image.length);
  new DataView(bytes.buffer).setUint32(0, json.length, true);
  bytes.set(json, 4);
  bytes.set(image, 4 + json.length);
  return bytes.buffer;
}

describe("readPacket", () => {
  test("reads the header and the picture", async () => {
    const read = readPacket(packet({ kind: "browser", tab: "browser-1", seq: 4, width: 1280, height: 800 }, [0xff, 0xd8, 0xff]));
    expect(read?.header).toEqual({ kind: "browser", tab: "browser-1", seq: 4, width: 1280, height: 800 });
    expect(read?.image.type).toBe("image/jpeg");
    expect([...new Uint8Array(await read!.image.arrayBuffer())]).toEqual([0xff, 0xd8, 0xff]);
  });

  test("refuses bytes that are cut short or not JSON", () => {
    expect(readPacket(new ArrayBuffer(2))).toBeNull();
    const short = new Uint8Array(packet({ tab: "t" }, []));
    new DataView(short.buffer).setUint32(0, 999, true);
    expect(readPacket(short.buffer)).toBeNull();
    const text = new TextEncoder().encode("not json");
    const bad = new Uint8Array(4 + text.length);
    new DataView(bad.buffer).setUint32(0, text.length, true);
    bad.set(text, 4);
    expect(readPacket(bad.buffer)).toBeNull();
  });
});

describe("jpegBlob", () => {
  test("turns an event's base64 into the JPEG's bytes", async () => {
    const blob = jpegBlob("/9j/");
    expect(blob.type).toBe("image/jpeg");
    expect([...new Uint8Array(await blob.arrayBuffer())]).toEqual([0xff, 0xd8, 0xff]);
  });
});
