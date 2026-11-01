// Expected value of a relic: ducats come from the rewards, platinum from a warframe.market price lookup
// (`price`); without one the platinum value stays null and the card shows a dash.
import { RELIC_QUALITY_MODES, relicDucatonator, type RelicQualityMode } from "../../../config/shared/relicPlannerView";
import { computeSquadEV } from "./relicMath";
import type { RelicGroup, RelicQuality } from "./types";

export interface RowEvData {
  plat: number | null;
  ducat: number | null;
  ratio: number | null;
  cls: "has-value" | "loading" | "no-data";
}

const NO_DATA: RowEvData = { plat: null, ducat: null, ratio: null, cls: "no-data" };

function qualityDucatEv(group: RelicGroup, quality: RelicQuality, squadSize: number): number | null {
  const rewards = group.qualities[quality]?.rewards ?? [];
  const ducats = rewards.map((r) => (r.ducats != null && r.ducats >= 0 ? r.ducats : null));
  if (ducats.every((d) => d == null)) return null;
  return computeSquadEV(rewards, ducats, squadSize);
}

/** Best ducat EV over the refinements the mode covers; `owned` is resolved by the caller. */
export function groupDucatEv(group: RelicGroup, squadSize: number, quality: RelicQuality | "best"): number | null {
  let best: number | null = null;
  for (const q of quality === "best" ? RELIC_QUALITY_MODES : [quality]) {
    const ev = qualityDucatEv(group, q, squadSize);
    if (ev != null && (best == null || ev > best)) best = ev;
  }
  return best;
}

export type PriceLookup = (urlName: string) => number | null;

function qualityPlatEv(group: RelicGroup, quality: RelicQuality, squadSize: number, price: PriceLookup): number | null {
  const rewards = group.qualities[quality]?.rewards ?? [];
  const prices = rewards.map((r) => (r.urlName ? price(r.urlName) : null));
  return prices.every((p) => p == null) ? null : computeSquadEV(rewards, prices, squadSize);
}

/** What the card shows for the refinement a quality mode selects (`null` = nothing owned). */
export function evDataFor(
  group: RelicGroup,
  squadSize: number,
  mode: RelicQualityMode,
  selectedOwned: RelicQuality | null,
  price?: PriceLookup,
): RowEvData {
  const quality = mode === "owned" ? selectedOwned : mode;
  if (!quality) return NO_DATA;
  const ducat = groupDucatEv(group, squadSize, quality);
  const plat = price ? qualityPlatEv(group, quality, squadSize, price) : null;
  return {
    plat,
    ducat,
    ratio: relicDucatonator(plat, ducat),
    cls: plat != null || ducat != null ? "has-value" : "no-data",
  };
}
