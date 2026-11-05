// Static sample content shown while overlays are being moved, and by `overlay.html?kind=…&mode=preview`
// (browser previews and screenshots). No live data needed.
import type { PlannerRow } from "../reward/plannerRows";
import type { RelicGroup, RelicPlannerEvent, ArbiLive, RivenOverlayView_Serialize as RivenOverlayView, TradeOverlayView_Serialize as TradeOverlayView } from "../../bindings";

type RivenCard = NonNullable<RivenOverlayView["card"]>;

const sampleStat = (name: string, positive: boolean, value: number, grade: string, quality: number, min: number, max: number): RivenCard["stats"][number] => ({ name, positive, value, multiplier: false, grade, min, max, quality });

const SAMPLE_CURRENT: RivenCard = {
  weapon: "Staticor",
  stats: [sampleStat("Critical Damage", true, 134, "A+", 88, 80, 152), sampleStat("Status Chance", true, 144.3, "A", 79, 70, 159), sampleStat("Reload Speed", false, 31.7, "B", 64, 20, 49)],
  overallGrade: "B+",
  attributeGrade: null,
  best: { positives: ["Critical Damage", "Status Chance"], negatives: ["Reload Speed"] },
  unsure: false,
  mr: 15,
  cardRolls: 5,
  cardKuvaSpent: 5200,
  similarScope: "all",
  analysis: { estimate: { source: "listings", p25: 280, median: 420, p75: 540, n: 14, lowConfidence: false, best: { matchPct: 86, platinum: 480 } }, listings: [] },
};

const SAMPLE_NEW: RivenCard = {
  ...SAMPLE_CURRENT,
  stats: [sampleStat("Cold", true, 109.4, "A+", 86, 60, 119), sampleStat("Punch Through", true, 3.1, "B+", 68, 1.2, 4.4)],
  overallGrade: "A",
  analysis: {
    estimate: { source: "listings", p25: 15, median: 35, p75: 60, n: 20, lowConfidence: false, best: { matchPct: 71, platinum: 65 } },
    listings: [
      { matchPct: 100, platinum: 60, sellerStatus: "ingame", url: "https://warframe.market/auction/sample", stats: [{ key: "cold_damage", positive: true, matched: true }, { key: "punch_through", positive: true, matched: true }] },
      { matchPct: 100, platinum: 45, sellerStatus: "online", url: "https://warframe.market/auction/sample", stats: [{ key: "cold_damage", positive: true, matched: true }, { key: "punch_through", positive: true, matched: true }] },
      { matchPct: 67, platinum: 90, sellerStatus: "ingame", url: "https://warframe.market/auction/sample", stats: [{ key: "cold_damage", positive: true, matched: true }, { key: "punch_through", positive: true, matched: true }, { key: "critical_chance", positive: true, matched: false }] },
      { matchPct: 50, platinum: 30, sellerStatus: "offline", url: "https://warframe.market/auction/sample", stats: [{ key: "cold_damage", positive: true, matched: true }, { key: "multishot", positive: true, matched: false }, { key: "zoom", positive: false, matched: false }] },
      { matchPct: 33, platinum: 20, sellerStatus: "online", url: "https://warframe.market/auction/sample", stats: [{ key: "punch_through", positive: true, matched: true }, { key: "toxin_damage", positive: true, matched: false }, { key: "base_damage_/_melee_damage", positive: true, matched: false }] },
    ],
  },
};

const sampleView = (over: Partial<RivenOverlayView>): RivenOverlayView => ({
  status: "ready",
  token: null,
  layout: "single",
  card: SAMPLE_NEW,
  current: null,
  verdict: null,
  nextKuva: 2000,
  message: null,
  autoCloseMs: 0,
  session: { kuvaPerRoll: 1700, rollCount: 1, totalKuvaSpent: 1700 },
  ...over,
});

export const SAMPLE_RIVEN_COMPARE = sampleView({ layout: "compare", current: SAMPLE_CURRENT, verdict: { kind: "keep", delta: -385, gradeCmp: "better" } });
export const SAMPLE_RIVEN_SINGLE = sampleView({});
/** The new card did not read: only Current is drawn. */
export const SAMPLE_RIVEN_PARTIAL = sampleView({ layout: "compare", current: SAMPLE_CURRENT, card: null });
/** No similar listing: the estimate falls back to the weapon median. */
export const SAMPLE_RIVEN_EMPTY = sampleView({
  card: { ...SAMPLE_NEW, analysis: { estimate: { source: "weaponMedian", p25: 40, median: 40, p75: 40, n: 112, lowConfidence: true, best: null }, listings: [] } },
});

export const SAMPLE_ARBI: ArbiLive = {
  node: "Hydron (Sedna)",
  missionType: "defense",
  missionTypeRaw: null,
  elapsedSec: 1458,
  rotations: 7,
  drones: 936,
  enemies: 2341,
  dpm: 38.5,
  saturationPct: 82,
  currentEnemies: null,
};

export const SAMPLE_TRADE: TradeOverlayView = {
  current: {
    trade: {
      id: "sample",
      date: "2026-01-01T00:00:00Z",
      type: "sale",
      platChange: 92,
      items: [
        { internalName: "", displayName: "Ash Prime Set", count: 1, direction: "given", wfmSlug: "ash_prime_set" },
        { internalName: "", displayName: "Platinum", count: 92, direction: "received" },
      ],
      partner: "Tenno_Kaz",
      schemaVersion: null,
      source: null,
    },
    matches: [{ kind: "order", orderId: "sample", itemName: "Ash Prime Set", itemUrlName: "ash_prime_set", itemThumb: null, quantity: 1, platinum: 92, partner: "Tenno_Kaz", type: "sale", perTrade: null }],
    closed: [],
  },
  queued: 1,
  token: null,
  autoCloseMs: 20000,
};

export const SAMPLE_PLANNER_EVENT: RelicPlannerEvent = { token: 0, phase: "ready", era: "meso", confidence: 1, ocrUnavailable: false };

const plannerRow = (name: string, quality: PlannerRow["quality"], count: number, plat: number, ducat: number): PlannerRow => ({
  group: { key: name } as RelicGroup,
  name,
  tier: "Meso",
  vaulted: false,
  quality,
  count,
  ownedCount: count,
  ownedTotal: count,
  plat,
  ducat,
  ratio: null,
});
export const SAMPLE_PLANNER_ROWS: PlannerRow[] = [
  plannerRow("Meso C2", "radiant", 3, 24.5, 31.2),
  plannerRow("Meso F3", "exceptional", 2, 18.1, 27.8),
  plannerRow("Meso S5", "intact", 5, 11.4, 22.5),
];
