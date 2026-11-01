// @vitest-environment jsdom
import { act, cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import type { RivenCardView_Serialize as Card, RivenOverlayView_Serialize as View } from "../../bindings";
import { translator } from "../../lib/i18n";
import type { OverlayIpc } from "../common/overlayIpc";
import { SAMPLE_RIVEN_COMPARE } from "../common/moveSamples";
import { RIVEN_OVERLAY_EVENT, RivenOverlay } from "./RivenOverlay";

afterEach(cleanup);

const card = (over: Partial<Card> = {}): Card => ({
  weapon: "Rubico", overallGrade: "A", attributeGrade: "Good", unsure: false, mr: null, cardRolls: null, cardKuvaSpent: null, similarScope: null, analysis: null,
  stats: [
    { name: "Critical Damage", positive: true, value: 90, multiplier: false, grade: "S", min: 60, max: 100, quality: 75 },
    { name: "Zoom", positive: false, value: 30, multiplier: false, grade: "B", min: null, max: null, quality: null },
  ],
  best: { positives: ["Critical Damage"], negatives: ["Zoom"] }, ...over,
});

const view = (over: Partial<View> = {}): View => ({
  status: "ready", token: 1, layout: "single", card: card(), current: null, verdict: null, nextKuva: null, message: null, autoCloseMs: 60000, session: null, ...over,
});

const listing = (platinum: number, matchPct = 100) => ({ matchPct, platinum, sellerStatus: "online", url: `https://warframe.market/auction/${platinum}`, stats: [{ key: "critical_damage", positive: true, matched: true }] });
const estimate = { source: "listings" as const, p25: 100, median: 120, p75: 150, n: 7, lowConfidence: false, best: { matchPct: 100, platinum: 100 } };

function mount() {
  let push: (p: View) => void = () => {};
  const ipc: OverlayIpc = {
    settings: async () => ({}) as never,
    placement: async () => [],
    layout: { get: async () => null, save: async () => {}, reset: async () => {} },
    on: (e, cb) => {
      if (e === RIVEN_OVERLAY_EVENT) push = cb as typeof push;
      return () => {};
    },
  };
  const { container } = render(<RivenOverlay tr={translator("en")} ipc={ipc} interactive={false} />);
  return { container, emit: (p: View) => act(async () => push(p)) };
}

it("is blank until a scan arrives", () => {
  expect(mount().container.textContent).toBe("");
});

it("shows weapon, grades, the roll range and best attributes, then the estimate and listings as they arrive", async () => {
  const { emit, container } = mount();
  await emit(view());
  expect(screen.getByText(/Rubico/)).toBeTruthy();
  expect(screen.getByText("Critical Damage")).toBeTruthy();
  expect(screen.getByText("75% of range")).toBeTruthy();
  expect(screen.getByText(/Best: Critical Damage/)).toBeTruthy();
  expect(screen.queryByText("Similar listings")).toBeNull();
  await emit(view({ card: card({ analysis: { estimate, listings: [listing(120), listing(140, 60)] } }) }));
  expect(screen.getAllByText("120").length).toBe(2);
  expect(screen.getByText("7 listings, weighted by match")).toBeTruthy();
  const rows = [...container.querySelectorAll<HTMLAnchorElement>("a.rv-li")];
  expect(rows.map((r) => r.getAttribute("href"))).toEqual(["https://warframe.market/auction/120", "https://warframe.market/auction/140"]);
  expect(within(rows[0]).getByText("CD")).toBeTruthy();
});

it("falls back to the weapon median and says it is a floor", async () => {
  const { emit } = mount();
  const median = { ...estimate, source: "weaponMedian" as const, p25: 40, median: 40, p75: 40, n: 112, lowConfidence: true, best: null };
  await emit(view({ card: card({ analysis: { estimate: median, listings: [] } }) }));
  expect(screen.getByText("~40")).toBeTruthy();
  expect(screen.getByText("low confidence")).toBeTruthy();
  expect(screen.getByText(/weapon median \(112 listings\)/)).toBeTruthy();
  expect(screen.getByRole("link", { name: /Search warframe.market/ })).toBeTruthy();
});

it("shows MR, the card's rolls, Kuva and the next cycle", async () => {
  const { emit } = mount();
  await emit(view({ nextKuva: 2000, card: card({ mr: 10, cardRolls: 39, cardKuvaSpent: 121800 }) }));
  expect(screen.getAllByText("MR 10").length).toBeGreaterThan(0);
  expect(screen.getAllByText("Rolls: 39").length).toBeGreaterThan(0);
  expect(screen.getByText(/121.800 Kuva spent|121,800 Kuva spent/)).toBeTruthy();
  expect(screen.getByText(/Next cycle 2.000 Kuva|Next cycle 2,000 Kuva/)).toBeTruthy();
  expect(screen.queryByText(/too small/)).toBeNull();
});

it("labels the EE.log tally as the session's when the card's own count is unknown", async () => {
  const { emit } = mount();
  await emit(view({ session: { kuvaPerRoll: 2700, rollCount: 18, totalKuvaSpent: 48300 } }));
  expect(screen.getByText("Session rolls: 18")).toBeTruthy();
});

it("compares Current with the New roll under the verdict", async () => {
  const { emit } = mount();
  await emit(SAMPLE_RIVEN_COMPARE);
  expect(screen.getByRole("status").textContent).toMatch(/Keep current/);
  expect(screen.getByRole("status").textContent).toMatch(/−385p/);
  expect(screen.getByText("Current")).toBeTruthy();
  expect(screen.getByText("New roll")).toBeTruthy();
  expect(screen.getByText("Critical Damage")).toBeTruthy();
  expect(screen.getByText("Cold")).toBeTruthy();
});

it("waits for the verdict while an estimate is missing, and draws a partial read", async () => {
  const { emit } = mount();
  await emit({ ...SAMPLE_RIVEN_COMPARE, verdict: null });
  expect(screen.getByRole("status").textContent).toMatch(/Waiting for the new card/);
  await emit({ ...SAMPLE_RIVEN_COMPARE, card: null });
  expect(screen.getByText("New card not read")).toBeTruthy();
  expect(screen.getByRole("button", { name: "Read again" })).toBeTruthy();
});

it("draws a skeleton only when a compare read has nothing before it, else keeps the last view under a chip, and clears on close", async () => {
  const { emit, container } = mount();
  await emit({ ...view(), status: "scanning", layout: "compare", card: null });
  expect(screen.getByText("Reading cards…")).toBeTruthy();
  await emit(view());
  await emit({ ...view(), status: "scanning", card: null });
  expect(screen.getByText("Scanning...")).toBeTruthy();
  expect(screen.getByText("Critical Damage")).toBeTruthy();
  await emit({ ...view(), status: "scanning", layout: "compare", card: null });
  expect(screen.getByText("Scanning...")).toBeTruthy();
  expect(screen.getByText("Critical Damage")).toBeTruthy();
  expect(screen.queryByText("Reading cards…")).toBeNull();
  await emit({ ...view(), status: "closed" });
  expect(container.textContent).toBe("");
});

it("says so when the models are missing instead of failing", async () => {
  const { emit } = mount();
  await emit(view({ status: "unavailable", card: null }));
  expect(screen.getByRole("alert").textContent).toMatch(/unavailable/);
});

it("grades nothing and says why when the weapon was not read", async () => {
  const { emit } = mount();
  await emit(view({ card: card({ weapon: null, overallGrade: null, stats: [{ name: "Zoom", positive: false, value: 30, multiplier: false, grade: null, min: null, max: null, quality: null }] }) }));
  expect(screen.getByText(/Couldn't read the weapon name/)).toBeTruthy();
});

it("shows a recoil curse with the sign the card prints, still marked as a curse", async () => {
  const { emit } = mount();
  await emit(view({ card: card({ stats: [{ name: "Weapon Recoil", positive: false, printedPositive: true, value: 51.2, multiplier: false, grade: "A-", min: 48.1, max: 58.8, quality: 71 }] }) }));
  expect(screen.getByText("+51.2%")).toBeTruthy();
  expect(screen.getByText("Weapon Recoil").closest("li")?.className).toContain("riven-neg");
});
