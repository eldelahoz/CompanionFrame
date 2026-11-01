// Hand-written shapes the stats libs read; the bindings type every f64 as number | null, the ledger never stores one.
import type { ItemDbEntry } from "./inventory";

/** Classification of a trade event. */
export type TradeType = "sale" | "purchase" | "trade";
/** Direction of an item in a trade. */
type TradeDirection = "given" | "received";

export interface TradeItem {
  internalName: string;
  displayName: string;
  count: number;
  direction: TradeDirection;
  wfmSlug?: string;
  wfmThumb?: string;
}

/** Where a ledger row came from; absent on legacy rows and means "live". */
type TradeEventSource = "live" | "gdpr";
export interface TradeEvent {
  id: string;
  date: string; // ISO datetime
  type: TradeType;
  platChange: number; // always positive (0 for pure item swaps)
  items: TradeItem[];
  partner?: string; // trading partner username (best-effort from EE.log)
  wfmClosed?: boolean; // true when a WFM order was auto-closed for this trade
  schemaVersion?: number; // absent = v1 legacy row
  source?: TradeEventSource;
  /** Deterministic id of the source record so re-imports stay idempotent. */
  sourceRecordId?: string;
  importBatchId?: string;
  credits?: number; // credits moved in the trade, when known
  tradeTax?: number;
  editedAt?: string; // ISO, set when the user fixes a row by hand
}
interface StatResourceDay {
  delta: number;
  abs?: number;
}
export interface DailyStatEntry {
  date: string; // "YYYY-MM-DD"
  platDelta: number; // net plat change this session/day
  creditsDelta: number;
  endoDelta: number;
  ducatsDelta: number; // net Void Ducat change (MiscItems/PrimeBucks)
  ayaDelta: number; // net Aya (MiscItems/SchismKey) change
  vitusDelta: number; // net Vitus Essence change (MiscItems/Elitium)
  relicsOpened: number; // relics consumed (LevelKeys net decrease, >=0)
  daysPlayed: number; // 1 = played; 0 = no inventory data (imported gap)
  dailyTrades: number; // number of trades detected or imported for this day
  absPlat?: number; // absolute platinum balance at end of day
  absCredits?: number; // absolute credits balance at end of day
  absEndo?: number; // absolute endo balance at end of day
  absDucats?: number; // absolute ducats balance at end of day
  absAya?: number; // absolute aya balance at end of day
  absVitus?: number; // absolute vitus essence balance at end of day
  resourcesVersion?: number; // schema of `resources`; absent on pre-map entries
  // Every tracked resource, keyed by catalog id. The six fields above stay
  // written so a downgraded build still reads its own history.
  resources?: Record<string, StatResourceDay>;
}


export type ItemDbLookup = Record<string, ItemDbEntry>;
export type WfmItemsLookup = Record<
  string,
  {
    url_name: string;
    item_name?: string;
    thumb?: string | null;
    icon?: string | null;
    maxRank?: number | null;
    gameRef?: string | null;
  }
>;
