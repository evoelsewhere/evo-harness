// Run with `bun test` from apps/desktop. Spec: docs/features/floating-cards (FR-7, FR-9).
import { describe, expect, test } from "bun:test";
import { keyEvents, modifiers, mouseParams, toPagePoint, wheelParams } from "./input";

const page = { w: 1280, h: 800 };
const none = { shiftKey: false, ctrlKey: false, altKey: false, metaKey: false };

describe("a point on the picture", () => {
  test("a box of the page's own size maps one to one", () => {
    expect(toPagePoint(110, 220, { left: 10, top: 20, width: 1280, height: 800 }, page)).toEqual({ x: 100, y: 200 });
  });

  test("a smaller box scales the point back to page pixels", () => {
    const box = { left: 100, top: 50, width: 360, height: 225 };
    const point = toPagePoint(100 + 180, 50 + 112.5, box, page)!;
    expect(point.x).toBeCloseTo(640, 0);
    expect(point.y).toBeCloseTo(400, 0);
  });

  test("a wider box leaves margins at the sides, which are not the page", () => {
    const box = { left: 0, top: 0, width: 1000, height: 400 }; // the picture is 640x400 centred
    expect(toPagePoint(10, 200, box, page)).toBeNull();
    const centre = toPagePoint(500, 200, box, page)!;
    expect(centre.x).toBeCloseTo(640, 0);
    expect(centre.y).toBeCloseTo(400, 0);
    expect(toPagePoint(180, 0, box, page)).toEqual({ x: 0, y: 0 });
  });

  test("an empty box maps nothing", () => {
    expect(toPagePoint(1, 1, { left: 0, top: 0, width: 0, height: 0 }, page)).toBeNull();
  });
});

describe("mouse and wheel", () => {
  test("a press names its button and click count; a move names the held button", () => {
    const press = mouseParams("mousePressed", { x: 5, y: 6 }, { button: 2, buttons: 2, detail: 1, ...none });
    expect(press).toMatchObject({ type: "mousePressed", x: 5, y: 6, button: "right", buttons: 2, clickCount: 1 });
    const double = mouseParams("mousePressed", { x: 5, y: 6 }, { button: 0, buttons: 1, detail: 2, ...none });
    expect(double.clickCount).toBe(2);
    const drag = mouseParams("mouseMoved", { x: 7, y: 8 }, { button: 0, buttons: 1, detail: 0, ...none });
    expect(drag).toMatchObject({ button: "left", clickCount: 0 });
    expect(mouseParams("mouseMoved", { x: 7, y: 8 }, { button: 0, buttons: 0, detail: 0, ...none }).button).toBe("none");
  });

  test("modifier bits follow DevTools", () => {
    expect(modifiers({ ...none, altKey: true, shiftKey: true })).toBe(9);
    expect(modifiers({ ...none, ctrlKey: true, metaKey: true })).toBe(6);
    expect(modifiers(none)).toBe(0);
  });

  test("a wheel turn is scaled to the page and lines become pixels", () => {
    expect(wheelParams({ x: 1, y: 2 }, { deltaX: 0, deltaY: 100, deltaMode: 0 }, 2)).toEqual({ type: "mouseWheel", x: 1, y: 2, deltaX: 0, deltaY: 200 });
    expect(wheelParams({ x: 1, y: 2 }, { deltaX: 0, deltaY: 3, deltaMode: 1 }, 1).deltaY).toBe(48);
  });
});

describe("keys", () => {
  test("a letter types its text", () => {
    const keys = keyEvents({ key: "a", code: "KeyA", ...none })!;
    expect(keys.down).toMatchObject({ type: "keyDown", key: "a", text: "a", windowsVirtualKeyCode: 65 });
    expect(keys.up).toMatchObject({ type: "keyUp", key: "a" });
  });

  test("Enter types a carriage return and Backspace is a raw key", () => {
    expect(keyEvents({ key: "Enter", code: "Enter", ...none })!.down).toMatchObject({ type: "keyDown", text: "\r", windowsVirtualKeyCode: 13 });
    expect(keyEvents({ key: "Backspace", code: "Backspace", ...none })!.down).toMatchObject({ type: "rawKeyDown", windowsVirtualKeyCode: 8 });
    expect(keyEvents({ key: "ArrowLeft", code: "ArrowLeft", ...none })!.down.windowsVirtualKeyCode).toBe(37);
  });

  test("shortcuts and lone modifiers stay with the app", () => {
    expect(keyEvents({ key: "c", code: "KeyC", ...none, ctrlKey: true })).toBeNull();
    expect(keyEvents({ key: "p", code: "KeyP", ...none, metaKey: true })).toBeNull();
    expect(keyEvents({ key: "Shift", code: "ShiftLeft", ...none, shiftKey: true })).toBeNull();
  });

  test("shift is carried as a modifier", () => {
    expect(keyEvents({ key: "A", code: "KeyA", ...none, shiftKey: true })!.down.modifiers).toBe(8);
  });
});
