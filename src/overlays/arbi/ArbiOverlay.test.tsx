// @vitest-environment jsdom
import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import type { ArbiLive } from "../../bindings";
import { translator } from "../../lib/i18n";
import type { OverlayIpc } from "../common/overlayIpc";
import { ARBI_LIVE_EVENT, ArbiOverlay, formatClock } from "./ArbiOverlay";

afterEach(cleanup);

const live = (over: Partial<ArbiLive> = {}): ArbiLive => ({
  node: "Casta (Ceres)", missionType: "defense", missionTypeRaw: null, elapsedSec: 754, rotations: 3,
  drones: 21, enemies: 480, dpm: 1.67, saturationPct: 62.4, currentEnemies: 17, ...over,
});

function mount() {
  let push: (p: ArbiLive | null) => void = () => {};
  const ipc: OverlayIpc = {
    settings: async () => ({}) as never,
    placement: async () => [],
    layout: { get: async () => null, save: async () => {}, reset: async () => {} },
    on: (e, cb) => {
      if (e === ARBI_LIVE_EVENT) push = cb as typeof push;
      return () => {};
    },
  };
  const { container } = render(<ArbiOverlay tr={translator("en")} ipc={ipc} interactive={false} />);
  return { container, emit: (p: ArbiLive | null) => act(async () => push(p)) };
}

it("formats the clock", () => {
  expect([formatClock(0), formatClock(65), formatClock(3725)]).toEqual(["0:00", "1:05", "1:02:05"]);
});

it("is blank until a run reports", () => {
  expect(mount().container.textContent).toBe("");
});

it("shows the counters, follows updates and clears when the run closes", async () => {
  const { container, emit } = mount();
  await emit(live());
  expect(screen.getByText("Casta (Ceres)")).toBeTruthy();
  expect(container.textContent).toContain("12:34");
  expect(container.textContent).toContain("62%");
  expect(container.textContent).toContain("480");
  await emit(live({ drones: 22, elapsedSec: 800 }));
  expect(container.textContent).toContain("13:20");
  await emit(null);
  expect(container.textContent).toBe("");
});

it("labels an unclassified mission from the game's own enum", async () => {
  const { container, emit } = mount();
  await emit(live({ missionType: "other", missionTypeRaw: "MT_EXCAVATE" }));
  expect(container.textContent).toContain("Excavate");
});
