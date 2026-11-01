// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { installOverlayDrag } from "./overlayDrag";

const mouse = (type: string, init: MouseEventInit) => document.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, ...init }));
let off = () => {};
afterEach(() => {
  off();
  vi.unstubAllGlobals();
});

function setup(interactive = true) {
  const frames: (() => void)[] = [];
  vi.stubGlobal("requestAnimationFrame", (cb: () => void) => frames.push(cb));
  const moveBy = vi.fn();
  off = installOverlayDrag({ isInteractive: () => interactive, moveBy });
  return { moveBy, frames };
}

it("batches a left-drag into one move per frame", () => {
  const { moveBy, frames } = setup();
  mouse("mousedown", { button: 0, screenX: 10, screenY: 10 });
  mouse("mousemove", { buttons: 1, screenX: 15, screenY: 12 });
  mouse("mousemove", { buttons: 1, screenX: 20, screenY: 20 });
  expect(moveBy).not.toHaveBeenCalled();
  frames.shift()?.();
  expect(moveBy).toHaveBeenCalledExactlyOnceWith(10, 10);
});

it("does not drag when not interactive", () => {
  const { moveBy, frames } = setup(false);
  mouse("mousedown", { button: 0, screenX: 0, screenY: 0 });
  mouse("mousemove", { buttons: 1, screenX: 9, screenY: 9 });
  expect(frames).toHaveLength(0);
  expect(moveBy).not.toHaveBeenCalled();
});

it("left-drag skips controls, right-drag does not", () => {
  const { frames } = setup();
  const btn = document.body.appendChild(document.createElement("button"));
  btn.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, button: 0 }));
  mouse("mousemove", { buttons: 1, screenX: 5, screenY: 5 });
  expect(frames).toHaveLength(0);
  btn.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, button: 2 }));
  mouse("mousemove", { buttons: 2, screenX: 5, screenY: 5 });
  expect(frames).toHaveLength(1);
  btn.remove();
});

it("stops when the button is released and clears the drag class", () => {
  const { moveBy, frames } = setup();
  mouse("mousedown", { button: 0 });
  expect(document.documentElement.classList.contains("is-overlay-dragging")).toBe(true);
  mouse("mouseup", { button: 0 });
  mouse("mousemove", { buttons: 1, screenX: 5, screenY: 5 });
  expect(frames).toHaveLength(0);
  expect(moveBy).not.toHaveBeenCalled();
  expect(document.documentElement.classList.contains("is-overlay-dragging")).toBe(false);
});
