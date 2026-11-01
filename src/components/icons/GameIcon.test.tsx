// @vitest-environment jsdom
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { GameIcon, PolarityIcon, RelicIcon } from "./GameIcon";

afterEach(() => vi.unstubAllGlobals());

describe("GameIcon", () => {
  it("tries each URL in order, then a generic glyph with the same label", () => {
    render(<GameIcon name="ducat" label="Ducats" />);
    const srcs: string[] = [];
    for (let i = 0; i < 3; i++) {
      const img = screen.getByRole("img", { name: "Ducats" });
      srcs.push(img.getAttribute("src") ?? "");
      fireEvent.error(img);
    }
    expect(srcs[0]).toContain("wiki.warframe.com");
    expect(srcs[1]).toContain("browse.wf");
    expect(srcs[2]).toContain("cdn.warframestat.us");
    const generic = screen.getByRole("img", { name: "Ducats" });
    expect(generic.tagName.toLowerCase()).toBe("svg");
  });

  it("is hidden from assistive tech without a label", () => {
    const { container } = render(<GameIcon name="platinum" />);
    expect(container.querySelector("img")?.getAttribute("alt")).toBe("");
  });

  it("builds relic URLs from tier and refinement", () => {
    render(<RelicIcon tier="requiem" refinement="radiant" label="r" />);
    const img = screen.getByRole("img", { name: "r" });
    expect(img.getAttribute("src")).toContain("RequiemRelicRadiant.png");
    fireEvent.error(img);
    expect(screen.getByRole("img", { name: "r" }).getAttribute("src")).toContain("RelicImmortalC.png");
  });

  it("masks with the first URL that loads, skipping one that fails", async () => {
    const tried: string[] = [];
    class FakeImage {
      crossOrigin = "";
      onload: (() => void) | null = null;
      onerror: (() => void) | null = null;
      set src(url: string) {
        tried.push(url);
        queueMicrotask(() => (tried.length === 1 ? this.onerror : this.onload)?.());
      }
    }
    vi.stubGlobal("Image", FakeImage);
    render(<GameIcon name="standing" label="Standing" />);
    // standing has one URL, so the failed probe ends on the generic glyph
    await waitFor(() => expect(screen.getByRole("img", { name: "Standing" }).tagName.toLowerCase()).toBe("svg"));
    render(<PolarityIcon polarity="madurai" label="Madurai" />);
    await waitFor(() => expect((screen.getByRole("img", { name: "Madurai" }) as HTMLElement).style.getPropertyValue("--mask")).toContain("Madurai_Pol"));
  });

  it("uses the generic glyph for a polarity without a public icon", () => {
    render(<PolarityIcon polarity="aura" label="Aura" />);
    expect(screen.getByRole("img", { name: "Aura" }).tagName.toLowerCase()).toBe("svg");
  });
});

describe("bundled game art", () => {
  it("keeps src/assets/game out of the tree and out of imports", () => {
    expect(Object.keys(import.meta.glob("/src/assets/game/**"))).toEqual([]);
    const sources = import.meta.glob<string>("/src/**/*.{ts,tsx,css}", { query: "?raw", import: "default", eager: true });
    const offenders = Object.entries(sources).filter(([f, text]) => !f.endsWith("GameIcon.test.tsx") && text.includes("assets/game"));
    expect(offenders.map(([f]) => f)).toEqual([]);
  });
});
