// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { installExternalLinks } from "./ipc";

const open = vi.fn();
let off = () => {};
afterEach(() => { off(); open.mockReset(); document.body.innerHTML = ""; });

const click = (href: string, type = "click") => {
  document.body.innerHTML = `<a id="a" href="${href}" target="_blank"><span id="s">x</span></a>`;
  const ev = new MouseEvent(type, { bubbles: true, cancelable: true, button: type === "auxclick" ? 1 : 0 });
  document.getElementById("s")!.dispatchEvent(ev);
  return ev;
};

describe("installExternalLinks", () => {
  it("opens external http(s) anchors in the browser", () => {
    off = installExternalLinks(open, true);
    expect(click("https://warframe.market/items/x").defaultPrevented).toBe(true);
    expect(click("https://wiki.example/a", "auxclick").defaultPrevented).toBe(true);
    expect(open.mock.calls.map((c) => c[0])).toEqual(["https://warframe.market/items/x", "https://wiki.example/a"]);
  });
  it("ignores internal, hash and non-http links", () => {
    off = installExternalLinks(open, true);
    for (const h of ["#top", "/local", "javascript:alert(1)", "file:///c:/x", "mailto:a@b.c"]) expect(click(h).defaultPrevented).toBe(false);
    expect(open).not.toHaveBeenCalled();
  });
  it("patches window.open for http(s) only", () => {
    off = installExternalLinks(open, true);
    window.open("https://a.example/");
    window.open("javascript:alert(1)");
    expect(open.mock.calls).toEqual([["https://a.example/"]]);
  });
  it("does nothing outside Tauri", () => {
    off = installExternalLinks(open, false);
    expect(click("https://a.example/").defaultPrevented).toBe(false);
  });
});
