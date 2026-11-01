// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "vitest";
import { translator } from "../../lib/i18n";
import { getOnboarding, overlayPlacementGet, resetMock, setSetupDone } from "../../lib/ipc";
import { startTour } from "../../stores/onboarding";
import Tour from "../Tour";
import { OVERLAY_DEFAULTS } from "../../overlays/common/overlayDefaults";
import OverlayPlacement from "./OverlayPlacement";

const tr = translator("en");

beforeEach(resetMock);
afterEach(cleanup);

test("the tour can be restarted from anywhere and skipping records it as seen", async () => {
  await setSetupDone(true);
  render(<Tour tr={tr} enabled />);
  await waitFor(() => expect(document.querySelector("[data-tour-overlay]")).toBeTruthy());
  fireEvent.click(screen.getByText("Skip tour"));
  await waitFor(async () => expect((await getOnboarding()).tourSeen).toBe(true));
  expect(document.querySelector("[data-tour-overlay]")).toBeNull();
  act(() => startTour());
  expect(await screen.findByText("Feature tour")).toBeTruthy();
});

test("moving an overlay panel with the keyboard saves its placement for the screen size", async () => {
  render(<OverlayPlacement tr={tr} />);
  const panel = await screen.findByRole("button", { name: "Relic reward overlay" });
  fireEvent.keyDown(panel, { key: "ArrowRight" });
  await waitFor(async () => {
    const saved = await overlayPlacementGet(1920, 1080);
    expect(saved.find((e) => e.kind === "reward")?.placement.x_frac).toBeCloseTo(OVERLAY_DEFAULTS.reward.x + 0.01);
  });
});
