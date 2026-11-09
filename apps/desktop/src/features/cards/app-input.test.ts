// Run with `bun test` from apps/desktop. Spec: docs/features/computer-app-control (FR-13, T29).
import { describe, expect, test } from "bun:test";
import { appKeyInput, toFraction } from "./app-input";

const none = { shiftKey: false, ctrlKey: false, altKey: false, metaKey: false };

describe("the person's keys in an app card", () => {
  test("a printable character is text, Vietnamese included", () => {
    expect(appKeyInput({ ...none, key: "a" })).toEqual({ kind: "text", text: "a" });
    expect(appKeyInput({ ...none, key: "ệ" })).toEqual({ kind: "text", text: "ệ" });
    expect(appKeyInput({ ...none, shiftKey: true, key: "A" })).toEqual({ kind: "text", text: "A" });
  });

  test("named keys and shortcuts are key names", () => {
    expect(appKeyInput({ ...none, key: "Enter" })).toEqual({ kind: "key", key: "enter" });
    expect(appKeyInput({ ...none, shiftKey: true, key: "Tab" })).toEqual({ kind: "key", key: "shift+tab" });
    expect(appKeyInput({ ...none, ctrlKey: true, key: "s" })).toEqual({ kind: "key", key: "ctrl+s" });
    expect(appKeyInput({ ...none, ctrlKey: true, shiftKey: true, key: "S" })).toEqual({ kind: "key", key: "ctrl+shift+s" });
    expect(appKeyInput({ ...none, key: "F5" })).toEqual({ kind: "key", key: "f5" });
    expect(appKeyInput({ ...none, key: "ArrowDown" })).toEqual({ kind: "key", key: "down" });
  });

  test("a lone modifier sends nothing", () => {
    expect(appKeyInput({ ...none, ctrlKey: true, key: "Control" })).toBeNull();
    expect(appKeyInput({ ...none, key: "Unidentified" })).toBeNull();
  });

  test("a point is a fraction of the picture", () => {
    expect(toFraction({ x: 640, y: 200 }, { w: 1280, h: 800 })).toEqual({ x: 0.5, y: 0.25 });
    expect(toFraction(null, { w: 1280, h: 800 })).toBeNull();
  });
});
