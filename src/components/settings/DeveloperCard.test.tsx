// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import DeveloperCard from "./DeveloperCard";

const ipc = vi.hoisted(() => ({ devTradeSample: vi.fn() }));
vi.mock("../../lib/ipc", () => ipc);

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  vi.unstubAllEnvs();
});

describe("DeveloperCard", () => {
  it("has one button per sample and calls the ipc wrapper with it", () => {
    render(<DeveloperCard />);
    const buttons: Array<[string, string]> = [
      ["One listing", "OneListing"],
      ["Two listings", "TwoListings"],
      ["Closed + message copied", "Closed"],
      ["Purchase", "Purchase"],
      ["No listing", "NoListing"],
      ["Plain trade", "PlainTrade"],
    ];
    for (const [label] of buttons) fireEvent.click(screen.getByRole("button", { name: label }));
    expect(ipc.devTradeSample.mock.calls.map((c) => c[0])).toEqual(buttons.map(([, s]) => s));
    expect(screen.getByText(/Nothing is sent to warframe.market/)).toBeTruthy();
  });

  it("renders nothing outside development builds", () => {
    vi.stubEnv("DEV", false);
    const { container } = render(<DeveloperCard />);
    expect(container.firstChild).toBeNull();
  });
});
