// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import MasteryMark from "./MasteryMark";

const tr = ((k: string, p?: Record<string, unknown>) => `${k}:${p?.rank}/${p?.max}`) as never;

afterEach(cleanup);

describe("MasteryMark", () => {
  it("labels each state with the mastery icon", () => {
    for (const status of ["mastered", "progress", "missing"] as const) {
      const { container } = render(<MasteryMark mark={{ status, rank: 12, maxRank: 30 }} tr={tr} />);
      expect(screen.getByRole("img", { name: `inventory.mastery.${status}:12/30` })).toBeTruthy();
      expect(container.querySelector("img.game-icon")).toBeTruthy();
      cleanup();
    }
  });
  it("shows the rank only while in progress", () => {
    const { container, rerender } = render(<MasteryMark mark={{ status: "progress", rank: 12, maxRank: 30 }} tr={tr} />);
    expect(container.textContent).toBe("12/30");
    rerender(<MasteryMark mark={{ status: "mastered", rank: 30, maxRank: 30 }} tr={tr} />);
    expect(container.textContent).toBe("");
  });
  it("renders nothing without a mark", () => {
    const { container } = render(<MasteryMark mark={null} tr={tr} />);
    expect(container.firstChild).toBeNull();
  });
});
