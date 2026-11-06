// The only module that talks to the generated bindings. Outside Tauri (plain Vite, tests) an in-memory mock answers.
import { logTiming, msSince } from "./timing";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { openUrl } from "@tauri-apps/plugin-opener";
import pkg from "../../package.json";
import { MOCK_RELIC_DB, MOCK_RELIC_INVENTORY } from "./relic/mockDb";
import {
  commands,
  type AppInfo,
  type CatalogItem_Serialize as CatalogItem,
  type RivenRollSession,
  type OcrInstallOutcome,
  type OcrLanguageStatus,
  type DecodedRivens_Serialize as DecodedRivensBinding,
  type RivenBestAttributes,
  type RivenGoodRollsInfo,
  type Item_Serialize as WfmItem,
  type RivenWeapon_Serialize as RivenWeapon,
  type PriceEntry_Serialize as PriceEntry,
  type MarketStatPoint_Serialize as MarketStatPoint,
  type SessionStatus_Serialize,
  type SetLookup,
  type DropRow_Serialize as DropRow,
  type DropSearchMode,
  type DropSearchResult,
  type ItemDbEntry_Serialize as ItemDbEntry,
  type MasteryProgress,
  type RelicDataInfo,
  type RelicDatabase,
  type Settings_Serialize,
  type ExportResult_Serialize as ExportResult,
  type HelperDownloadProgress_Serialize as HelperDownloadProgress,
  type HelperStatus_Serialize as HelperStatus,
  type InventorySource,
  type InventoryStatus_Serialize as InventoryStatus,
  type SpawnNode,
  type MyOrder_Serialize as MyOrder,
  type MyOrders_Serialize as MyOrders,
  type BookOrder_Serialize as BookOrder,
  type CreateOrder_Deserialize as CreateOrder,
  type OrderUpdates_Deserialize as OrderUpdates,
  type TrackersStatus_Serialize as TrackersStatus, type PresenceOptions, type PresenceState_Deserialize, type Status as WfmStatus, type WsStatus,
  type WorldState,
  type ChannelState, type NotificationEntry, type NotificationSource, type SoundAsset, type SoundUpload, type SourceToggles, type WebhookChannel, type WebhookTestError, type WebhookUrlError, type DiscordPingError, type WorldNotifyPrefs_Serialize as WorldNotifyPrefs, type WorldFissureAlert, type ArbiRunRecord_Serialize as ArbiRunRecord, type ArbiRunsPayload_Serialize as ArbiRunsPayload, type ArbiImportResult_Serialize as ArbiImportResult, type PtRunRecord_Serialize, type PtPhase as PtPhaseBinding, type WfmContract_Serialize as WfmContract, type WfmContractsPage_Serialize as WfmContractsPage, type RivenSearchOptions_Deserialize as RivenSearchOptions, type RivenAuctionCreate_Deserialize as RivenAuctionCreate, type RivenAuctionUpdate_Deserialize as RivenAuctionUpdate, type SendRepResult, type ArbiScheduleEntry as ArbiScheduleEntryBinding, type ArbiScheduleAlerts as ArbiScheduleAlertsBinding, type TradeDetected_Serialize as TradeDetected, type LedgerBatch_Serialize as LedgerBatchBinding, type LedgerQuery_Deserialize as LedgerQuery, type LedgerPatch_Deserialize as LedgerPatch, type ImportPreview_Serialize as ImportPreview, type ApplyResult, type EeLogStatus_Serialize as EeLogStatus, type OnnxStatus_Serialize as OnnxStatus, type GameWindowInfo_Serialize as GameWindowInfo, type ScanDebug, type DevTradeSample,
} from "../bindings";
import type { TradeEvent } from "../types/stats";
import { localDayKey } from "../../config/shared/dayKey";
import type { ArbiRunStats } from "../../config/shared/arbiTypes";
import { MOCK_ARBI_RUNS, MOCK_PT_RUNS } from "./arbi/mockRuns";
import { MOCK_MASTERY } from "./mockMastery";

import type { RawInventoryData } from "../types/inventory";
import { mockWorldState } from "./world/fixture";
import { mockBook, mockHistory } from "./wfm/mockMarket";
import { DEFAULT_SOURCE_CHANNELS, NOTIFICATION_LOG_MAX_ENTRIES } from "../../config/shared/notifications";

export type {
  AppInfo, DropRow, DropSearchMode, DropSearchResult, ItemDbEntry, MasteryProgress, RelicDataInfo, RelicDatabase, SpawnNode, WorldState, TrackersStatus, ExportResult, HelperDownloadProgress, HelperStatus, InventorySource, InventoryStatus, CatalogItem, RivenWeapon, SetLookup, WfmItem, PriceEntry, MarketStatPoint, ChannelState, NotificationEntry, NotificationSource, SoundAsset, SoundUpload, SourceToggles, WebhookChannel, ArbiRunRecord, ArbiRunsPayload, ArbiImportResult, WorldNotifyPrefs, WorldFissureAlert, OcrInstallOutcome, OcrLanguageStatus,
};
export type Language = "en" | "de" | "zh" | "fr" | "es";
export type Theme = "dark" | "light";
// The backend types language/theme as plain strings; the UI narrows them to the values it supports.
export type Settings = Omit<Settings_Serialize, "language" | "theme" | "ui_scale"> & { language: Language; theme: Theme; ui_scale: number };

// The bindings type every f64 as number | null (NaN serializes to null); the backend never stores one.
type Finite<T> = [T] extends [number | null]
  ? [T] extends [null] ? null : number
  : T extends (infer U)[] ? Finite<U>[]
  : T extends object ? { [K in keyof T]: Finite<T[K]> }
  : T;
export type PtRunRecord = Finite<PtRunRecord_Serialize>;
export type PtPhase = Finite<PtPhaseBinding>;
export type { ArbiRunStats } from "../../config/shared/arbiTypes";
type ArbiRunBase = Finite<ArbiRunRecord>;
type ArbiRunOptional = "tags" | "players" | "notes" | "duplicateOf" | "missionTypeRaw" | "solNode";
// Stats follow the hand-written shape the libs use; the backend always sends the optional fields.
export type ArbiRun = Omit<ArbiRunBase, "stats" | "vitusActual" | ArbiRunOptional> &
  Partial<Pick<ArbiRunBase, ArbiRunOptional>> & { stats: ArbiRunStats | null; vitusActual: number | null };
export type PtRunsPayload = { runs: PtRunRecord[]; diskUsageBytes: number };
export type PtImportResult = { imported: PtRunRecord[]; skipped: number };

export const inTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const DEFAULT_SETTINGS: Settings = { language: "en", game_language: "auto", theme: "dark", reward_scan_hotkey: "F8", overlay_move_hotkey: "F7", riven_rescan_hotkey: "F6", eelog_path: null, overlay_layouts: {}, overlay_enabled: {}, overlay_opacity: {}, message_notifications_enabled: true, message_notifications_while_focused: false, riven_similar_auctions_shown: true, theme_settings: "", ui_scale: 1, keep_running_on_close: false, data_source_mode: "passive", memory_consent_at: null, auto_inventory_sync: true, save_scan_captures: false, auto_update_check: true, auto_close_trade_listings: false, trade_thanks_enabled: false, trade_thanks_message: "Thank you for your purchase. I appreciate the +rep <3" };
let mockSettings: Settings = { ...DEFAULT_SETTINGS };
const mockInfo: AppInfo = {
  name: "CompanionFrame",
  version: pkg.version,
  data_dir: "(mock)",
  log_dir: "(mock)",
};

// Unwraps tauri-specta's Result shape, throwing on error.
const unwrap = <T>(r: { status: "ok"; data: T } | { status: "error"; error: unknown }): T => {
  if (r.status === "error") throw new Error(String(r.error));
  return r.data;
};

export const getSettings = async (): Promise<Settings> =>
  inTauri() ? ((await commands.getSettings()) as Settings) : { ...mockSettings };

export const setSettings = async (settings: Settings): Promise<void> => {
  if (inTauri()) {
    unwrap(await commands.setSettings(settings));
    return;
  }
  if (settings.data_source_mode === "memory" && settings.memory_consent_at == null) throw new Error("memory mode needs the disclaimer accepted");
  mockSettings = { ...settings };
};

/** WebView2 swallows external links; send http(s) ones to the default browser (anchors, middle-click, window.open). Returns the uninstaller. */
export const installExternalLinks = (open: (url: string) => unknown = openUrl, force = inTauri()): (() => void) => {
  if (!force) return () => {};
  const external = (href: string) => {
    try {
      const u = new URL(href, location.href);
      return /^https?:$/.test(u.protocol) && u.origin !== location.origin ? u.href : null;
    } catch {
      return null;
    }
  };
  const onClick = (e: MouseEvent) => {
    if (e.button === 2) return;
    const url = e.target instanceof Element && external(e.target.closest("a")?.href ?? "");
    if (!url) return;
    e.preventDefault();
    void open(url);
  };
  const nativeOpen = window.open;
  window.open = ((url?: string | URL) => {
    const ext = url ? external(String(url)) : null;
    if (!ext) return null;
    void open(ext);
    return null;
  }) as typeof window.open;
  document.addEventListener("click", onClick, true);
  document.addEventListener("auxclick", onClick, true);
  return () => {
    window.open = nativeOpen;
    document.removeEventListener("click", onClick, true);
    document.removeEventListener("auxclick", onClick, true);
  };
};

/** Zoom of the main window: the display-derived base times `scale` (0.75-1.5). */
export const setZoom = async (scale: number): Promise<void> => {
  if (inTauri()) unwrap(await commands.setZoom(scale));
};

export const appInfo = async (): Promise<AppInfo> =>
  inTauri() ? unwrap(await commands.appInfo()) : { ...mockInfo };

export type ItemDatabase = Record<string, ItemDbEntry>;

const svg = (fill: string) =>
  `data:image/svg+xml,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><circle cx="32" cy="32" r="28" fill="${fill}"/></svg>`)}`;
/** Public art by file name, as the app gets it in production (@wfcd/items via cdn.warframestat.us). */
const art = (file: string) => `https://cdn.warframestat.us/img/${file}`;
const mockEntry = (e: Partial<ItemDbEntry> & Pick<ItemDbEntry, "name">): ItemDbEntry => ({
  category: "Misc",
  imageUrl: null,
  isPrime: false,
  masteryReq: 0,
  vaulted: false,
  exalted: false,
  type: "",
  isBuildComponent: false,
  description: "",
  productCategory: null,
  ducats: null,
  components: [],
  drops: [],
  wikiaUrl: null,
  tradable: true,
  ...e,
});
// Rhino Prime parts as in @wfcd/items: [name, uniqueName suffix, art, ducats].
const PARTS = [["Chassis", "Chassis", "GenericWarframePrimeChassis.png", 65], ["Neuroptics", "Helmet", "GenericWarframePrimeHelmet.png", 45], ["Systems", "Systems", "GenericWarframePrimeSystem.png", 15]] as const;
const MOCK_ITEMS: ItemDatabase = {
  "/Lotus/Powersuits/Rhino/RhinoPrime": mockEntry({
    name: "Rhino Prime",
    imageUrl: art("RhinoPrime.png"),
    category: "Warframe",
    isPrime: true,
    description: "The Prime form of the unstoppable tank Warframe.",
    components: PARTS.map(([name, suffix]) => ({ name, uniqueName: `/Lotus/Types/Recipes/WarframeRecipes/RhinoPrime${suffix}Component`, itemCount: 1, tradable: true, drops: [] })),
    recipe: { buildPrice: 25000, buildTime: 259200, num: 1, blueprintUniqueName: "/Lotus/Types/Recipes/WarframeRecipes/RhinoPrimeBlueprint", ingredients: PARTS.map(([, suffix]) => ({ uniqueName: `/Lotus/Types/Recipes/WarframeRecipes/RhinoPrime${suffix}Component`, count: 1 })) },
  }),
  ...Object.fromEntries(
    PARTS.map(([name, suffix, file, ducats]) => [
      `/Lotus/Types/Recipes/WarframeRecipes/RhinoPrime${suffix}Component`,
      mockEntry({
        name,
        imageUrl: art(file),
        ducats,
        isBuildComponent: true,
        componentOf: "/Lotus/Powersuits/Rhino/RhinoPrime",
        tradable: true,
        drops: [{ location: "Lith A1 Relic", type: name, rarity: "Rare", chance: 2 }],
        recipe: { buildPrice: 15000, buildTime: 43200, num: 1, blueprintUniqueName: "", ingredients: [{ uniqueName: "/Lotus/Types/Items/MiscItems/Ferrite", count: 1000 }, { uniqueName: "/Lotus/Types/Items/MiscItems/OrokinCell", count: 1 }] },
      }),
    ]),
  ),
  "/Lotus/Weapons/Tenno/Rifle/BoltoPrime": mockEntry({ name: "Boltor Prime", displayName: "Boltor Prime", imageUrl: art("BoltorPrime.png"), category: "Weapon", isPrime: true }),
  "/Lotus/Types/Items/MiscItems/Forma": mockEntry({ name: "Forma", imageUrl: art("GenericComponent.png") }),
  "/Lotus/Types/Items/MiscItems/NoArt": mockEntry({ name: "Item Without Art" }),
  "/Lotus/Powersuits/Volt/Volt": mockEntry({ name: "Volt", imageUrl: art("Volt.png"), category: "Warframe" }),
  "/Lotus/Weapons/Tenno/Rifle/BratonPrime": mockEntry({ name: "Braton Prime", imageUrl: art("BratonPrime.png"), category: "Weapon", isPrime: true, vaulted: true }),
  "/Lotus/Weapons/Tenno/Rifle/Acceltra": mockEntry({ name: "Acceltra", imageUrl: art("SapientPrimaryWeapon.png"), category: "Weapon", vaulted: false }),
  "/Lotus/Weapons/Tenno/Pistols/PrimeLex/PrimeLex": mockEntry({ name: "Lex Prime", imageUrl: art("LexPrime.png"), category: "Weapon", isPrime: true }),
  "/Lotus/Types/Items/MiscItems/Ferrite": mockEntry({ name: "Ferrite", imageUrl: art("ComponentFerrite.png"), category: "Resource" }),
  "/Lotus/Types/Items/MiscItems/Plastids": mockEntry({ name: "Plastids", imageUrl: art("ComponentPlastids.png"), category: "Resource" }),
  "/Lotus/Types/Items/MiscItems/OrokinCell": mockEntry({ name: "Orokin Cell", imageUrl: art("ComponentCell.png"), category: "Resource" }),
  "/Lotus/Types/Items/MiscItems/Neurodes": mockEntry({ name: "Neurodes", imageUrl: art("ComponentNeurode.png"), category: "Resource" }),
  "/Lotus/Upgrades/Mods/Warframe/AvatarHealthMaxMod": mockEntry({ name: "Vitality", imageUrl: art("HealthMaxMod.jpg"), category: "Mods", type: "Warframe Mod" }),
  "/Lotus/Upgrades/Mods/Rifle/WeaponCritChanceMod": mockEntry({ name: "Point Strike", imageUrl: art("RifleCritChanceMod.jpg"), category: "Mods", type: "Rifle Mod" }),
  "/Lotus/Upgrades/Mods/Rifle/WeaponDamageAmountMod": mockEntry({ name: "Serration", imageUrl: art("RifleDamageAmountMod.jpg"), category: "Mods", type: "Rifle Mod" }),
  "/Lotus/Types/Game/Projections/AxiS3Bronze": mockEntry({ name: "Axi S3 Intact", imageUrl: art("RelicAxiD.png"), category: "Relics", type: "Relic" }),
  "/Lotus/Types/Game/Projections/AxiS3Platinum": mockEntry({ name: "Axi S3 Radiant", imageUrl: art("RelicAxiC.png"), category: "Relics", type: "Relic" }),
  "/Lotus/Types/Game/CatbrowPet/MirrorCatbrowPetPowerSuit": mockEntry({ name: "Adarza Kavat", imageUrl: art("KavatBreedAdarza.png"), category: "Pets" }),
  "/Lotus/Types/Recipes/WarframeRecipes/RhinoPrimeBlueprint": mockEntry({ name: "Rhino Prime Blueprint", imageUrl: art("blueprint.png"), ducats: 100 }),
  "/Lotus/Types/Game/Projections/T1VoidProjectionAxiA1": mockEntry({ name: "Axi A1 Relic", imageUrl: art("RelicAxiD.png"), category: "Relics", type: "Relic" }),
};

/** unique name → entry. Outside Tauri a small in-memory database answers. */
export const getItemDatabase = async (): Promise<ItemDatabase> =>
  inTauri()
    ? ((await commands.getItemDatabase()) as ItemDatabase)
    : { ...MOCK_ITEMS, ...Object.fromEntries(Array.from({ length: bigInv() }, (_, i) => [bigPath(i), mockEntry({ name: bigName(i), category: "Mods", tradable: true, imageUrl: svg("#7a8aa8") })])) };

/** Tells the backend which game language names to project (en/de/zh/fr/es and the other DE locales). */
export const setGameLocale = async (code: string): Promise<void> => {
  if (inTauri()) unwrap(await commands.setGameLocale(code));
};

/** The game language EE.log's `-language:` reported, or null before the game has run. */
export const gameLanguageDetected = async (): Promise<string | null> => (inTauri() ? commands.gameLanguageDetected() : null);

/** Vite-only mock switches from the page URL (`?noinv`, `?ocrmissing=fr`), for screenshots and manual checks. */
const mockParam = (key: string): string | null => (typeof location === "undefined" ? null : new URLSearchParams(location.search).get(key));
let mockOcrInstalled = false;

export type { EeLogStatus, OnnxStatus, GameWindowInfo, ScanDebug };
export const eelogStatus = async (): Promise<EeLogStatus> => (inTauri() ? commands.eelogStatus() : { path: "C:/Users/Tenno/AppData/Local/Warframe/EE.log", active: true, bytes_read: 1048576, dbwin_active: false });
export const ocrOnnxStatus = async (): Promise<OnnxStatus> =>
  inTauri() ? unwrap(await commands.ocrOnnxStatus()) : { runtimePresent: true, paddlePresent: true, detectorPresent: true, ready: true, error: null };
export const captureGameWindowInfo = async (): Promise<GameWindowInfo> =>
  inTauri() ? unwrap(await commands.captureGameWindowInfo()) : { found: true, x: 0, y: 0, width: 1920, height: 1080, dpi: 96, source: "window", black: false, error: null };
/** What the last reward scan saw; null before any scan. */
export const rewardScanDebug = async (): Promise<ScanDebug | null> => (inTauri() ? unwrap(await commands.rewardScanDebug()) : { cards: 4, slots: [["Forma Blueprint"], ["Ash Prime Systems"], ["50 Endo"], ["Ivara Prime Chassis"]] });

export type { DevTradeSample };
/** Developer panel (debug builds only): shows the trade popup with sample data; nothing is recorded or sent to warframe.market. */
export const devTradeSample = async (sample: DevTradeSample): Promise<void> => {
  if (inTauri()) unwrap(await commands.devTradeSample(sample));
};

/** Whether Windows can read reward cards in the game language (its OCR pack is installed). */
export const ocrLanguageStatus = async (): Promise<OcrLanguageStatus> => {
  if (inTauri()) return commands.ocrLanguageStatus();
  const missing = mockParam("ocrmissing");
  return missing && !mockOcrInstalled ? { gameLanguage: missing, installed: false, installable: true } : { gameLanguage: missing ?? "en", installed: true, installable: true };
};

/** Installs the game language's Windows OCR pack: one UAC prompt, then minutes of download. */
export const ocrLanguageInstall = async (): Promise<OcrInstallOutcome> => {
  if (inTauri()) return unwrap(await commands.ocrLanguageInstall());
  await new Promise((r) => setTimeout(r, Number(mockParam("ocrdelay") ?? 1500)));
  const outcome = (mockParam("ocrinstall") ?? "installed") as OcrInstallOutcome;
  mockOcrInstalled = outcome === "installed";
  return outcome;
};

/** Fires when the item DB is refreshed or the game language changes; the caller re-fetches. */
export const onItemDbUpdated = async (cb: () => void): Promise<UnlistenFn> => {
  if (!inTauri()) return () => {};
  const offs = await Promise.all(["item-db-updated", "game-locale-updated"].map((e) => listen(e, cb)));
  return () => offs.forEach((off) => off());
};

/** Background-refreshed prices landing for slugs that were served from an older cache entry. */
export const onPricesRefreshed = async (cb: (entries: PriceEntry[]) => void): Promise<UnlistenFn> =>
  inTauri() ? listen<PriceEntry[]>("wfm-prices-refreshed", (e) => cb(e.payload)) : () => {};

/** A listing lost `quantity` units because a trade match closed it (popup or Workbench). */
export const onWfmOrderClosed = async (cb: (e: { order_id: string; quantity: number }) => void): Promise<UnlistenFn> =>
  inTauri() ? listen<{ order_id: string; quantity: number }>("wfm-order-closed", (e) => cb(e.payload)) : () => {};

/** Mastery progress from the inventory snapshot, else from the public profile (`fromProfile`), else null. */
export const getMasteryProgress = async (): Promise<MasteryProgress | null> => {
  if (inTauri()) return unwrap(await commands.getMasteryProgress());
  if (mockSource !== "none") return MOCK_MASTERY;
  return mockParam("profile") === null ? null : { ...MOCK_MASTERY, fromProfile: true };
};

/** Fires when a public-profile fetch refreshed the profile-derived mastery data. */
export const onProfileInventoryUpdated = async (cb: () => void): Promise<UnlistenFn> => (inTauri() ? listen("profile-inventory-updated", cb) : () => {});
// --- Inventory. The backend sends the inventory as JSON text (or null) so the bindings need not model it.
const MOCK_INVENTORY: RawInventoryData = {
  Suits: [
    { ItemType: "/Lotus/Powersuits/Rhino/RhinoPrime", XP: 900000 },
    { ItemType: "/Lotus/Powersuits/Volt/Volt", XP: 450000 },
  ],
  LongGuns: [{ ItemType: "/Lotus/Weapons/Tenno/Rifle/BoltoPrime", XP: 450000 }],
  Pistols: [{ ItemType: "/Lotus/Weapons/Tenno/Pistols/PrimeLex/PrimeLex", XP: 0 }],
  MiscItems: [
    { ItemType: "/Lotus/Types/Items/MiscItems/Ferrite", ItemCount: 482113 },
    { ItemType: "/Lotus/Types/Items/MiscItems/Plastids", ItemCount: 12040 },
    { ItemType: "/Lotus/Types/Items/MiscItems/OrokinCell", ItemCount: 37 },
    { ItemType: "/Lotus/Types/Items/MiscItems/Neurodes", ItemCount: 215 },
    { ItemType: "/Lotus/Types/Items/MiscItems/Forma", ItemCount: 9 },
    { ItemType: "/Lotus/Types/Game/Projections/AxiS3Bronze", ItemCount: 4 },
    { ItemType: "/Lotus/Types/Game/Projections/AxiS3Platinum", ItemCount: 1 },
  ],
  RawUpgrades: [
    { ItemType: "/Lotus/Upgrades/Mods/Warframe/AvatarHealthMaxMod", ItemCount: 3 },
    { ItemType: "/Lotus/Upgrades/Mods/Rifle/WeaponCritChanceMod", ItemCount: 2 },
  ],
  // The same mod at three ranks: the Inventory splits them and prices the maxed copy at the max-rank price.
  Upgrades: [
    { ItemType: "/Lotus/Upgrades/Mods/Rifle/WeaponDamageAmountMod", ItemCount: 4, UpgradeFingerprint: '{"lvl":0}' },
    { ItemType: "/Lotus/Upgrades/Mods/Rifle/WeaponDamageAmountMod", ItemCount: 1, UpgradeFingerprint: '{"lvl":4}' },
    { ItemType: "/Lotus/Upgrades/Mods/Rifle/WeaponDamageAmountMod", ItemCount: 1, UpgradeFingerprint: '{"lvl":10}' },
  ],
  LevelKeys: [{ ItemType: "/Lotus/Types/Game/Projections/T1VoidProjectionAxiA1", ItemCount: 6 }],
  // Companions: one Kavat with genes, one stored imprint (Pets tab).
  KubrowPets: [{ ItemType: "/Lotus/Types/Game/CatbrowPet/MirrorCatbrowPetPowerSuit", Details: { Name: "Mochi", IsMale: false, Size: 1.12, Status: "STATUS_AVAILABLE", PrintsRemaining: 2, DominantTraits: { BaseColor: "/Lotus/Types/Game/CatbrowPet/Colors/CatbrowPetColorAccentsA", SecondaryColor: "/Lotus/Types/Game/CatbrowPet/Colors/CatbrowPetColorAccentsB", AccentColor: "/Lotus/Types/Game/CatbrowPet/Colors/CatbrowPetColorAccentsA", FurPattern: "/Lotus/Types/Game/CatbrowPet/Patterns/CatbrowPetPatternA" }, RecessiveTraits: { BaseColor: "/Lotus/Types/Game/CatbrowPet/Colors/CatbrowPetColorAccentsB", SecondaryColor: "/Lotus/Types/Game/CatbrowPet/Colors/CatbrowPetColorAccentsA", AccentColor: "/Lotus/Types/Game/CatbrowPet/Colors/CatbrowPetColorAccentsB", FurPattern: "/Lotus/Types/Game/CatbrowPet/Patterns/CatbrowPetPatternA" } } }],
  KubrowPetPrints: [{ ItemType: "/Lotus/Types/Game/KubrowPetPrint", Name: "Mochi", IsMale: false, Size: 1.12, DominantTraits: { Personality: "/Lotus/Types/Game/CatbrowPet/MirrorCatbrowPetPowerSuit", BaseColor: "/Lotus/Types/Game/CatbrowPet/Colors/CatbrowPetColorAccentsA" }, RecessiveTraits: { Personality: "/Lotus/Types/Game/CatbrowPet/MirrorCatbrowPetPowerSuit", BaseColor: "/Lotus/Types/Game/CatbrowPet/Colors/CatbrowPetColorAccentsB" } }],
  // Foundry: one blueprint per state (not ready, ready to build, claimable, still building).
  Recipes: [
    { ItemType: "/Lotus/Types/Recipes/WarframeRecipes/RhinoPrimeBlueprint", ItemCount: 1 },
    { ItemType: "/Lotus/Types/Recipes/WarframeRecipes/RhinoPrimeChassisComponent", ItemCount: 2 },
  ],
  // Syndicates: standing, rank and today's remaining pools, so the Syndicates view has something to plan from.
  PlayerLevel: 22,
  RegularCredits: 2_450_000,
  DailyAffiliation: 12_000,
  DailyAffiliationCetus: 9_000,
  Affiliations: [
    { Tag: "CetusSyndicate", Title: 2, Standing: 33_000 },
    { Tag: "ArbitersSyndicate", Title: 1, Standing: 14_000, Initiated: true },
    { Tag: "NewLokaSyndicate", Title: 0, Standing: 2_000 },
  ],
  PendingRecipes: [
    { ItemType: "/Lotus/Types/Recipes/WarframeRecipes/RhinoPrimeSystemsComponent", CompletionDate: "2020-01-01T00:00:00Z" },
    { ItemType: "/Lotus/Types/Recipes/WarframeRecipes/RhinoPrimeHelmetComponent", CompletionDate: "2999-01-01T00:00:00Z" },
  ],
} as RawInventoryData;

let mockSource: InventorySource = mockParam("noinv") === null ? "manual" : "none";
let mockImportedAt = Date.now() - 3 * 86_400_000;
const mockListeners = new Set<(data: RawInventoryData | null) => void>();
/** Outside Tauri the backend's `inventory-updated` push is simulated, so the views react to source changes. */
const setMockSource = (source: InventorySource): RawInventoryData | null => {
  mockSource = source;
  const data = source === "none" ? null : MOCK_INVENTORY;
  mockListeners.forEach((cb) => cb(data));
  return data;
};
const mockStatus = (): InventoryStatus => ({
  path: mockSource === "none" ? null : "C:/Users/Tenno/Documents/inventory.json",
  found: mockSource !== "none",
  source: mockSource,
  modifiedAt: mockImportedAt - 3_600_000,
  lastError: null,
  importedAt: mockSource === "none" ? null : mockImportedAt,
  tradesApplied: mockSource === "none" ? 0 : 5,
  picksApplied: mockSource === "none" ? 0 : 2,
  gameSyncedSinceImport: false,
});
const parseInv = (text: string | null): RawInventoryData | null => {
  if (!text) return null;
  const t0 = performance.now();
  const data = JSON.parse(text) as RawInventoryData;
  logTiming("json_parse_inventory", { bytes: text.length, ms: msSince(t0) });
  return data;
};

/** `?bigInv=2617` fills the mock with that many tradable mods, to check the Inventory view at a real account's size. */
const bigInv = (): number => Math.min(10_000, Number(mockParam("bigInv")) || 0);
const bigPath = (i: number) => `/Lotus/Upgrades/Mods/Bulk/Bulk${i}Mod`;
const bigName = (i: number) => `Bulk Mod ${i}`;
const bigPrice = (slug: string) => 1 + ((Number(slug.replace(/\D/g, "")) * 7) % 90);

export const getInventory = async (): Promise<RawInventoryData | null> => {
  if (inTauri()) return parseInv(await commands.getInventory());
  if (mockSource === "none") return null;
  const n = bigInv();
  const f = Math.min(10_000, Number(mockParam("bigFoundry")) || 0);
  const base = n ? { ...MOCK_INVENTORY, RawUpgrades: Array.from({ length: n }, (_, i) => ({ ItemType: bigPath(i), ItemCount: 1 + (i % 4) })) } : MOCK_INVENTORY;
  // `?bigFoundry=N` adds that many blueprints, to check the Foundry view at a real account's size.
  return f ? { ...base, Recipes: [...(base.Recipes ?? []), ...Array.from({ length: f }, (_, i) => ({ ItemType: `/Lotus/Types/Recipes/Bulk/Bulk${i}Blueprint`, ItemCount: 1 }))] } : base;
};

export const getInventoryStatus = async (): Promise<InventoryStatus> =>
  inTauri() ? await commands.getInventoryStatus() : mockStatus();

/** Native file pickers; resolve to the loaded inventory, or null when cancelled or unreadable. */
export const openInventoryFile = async (): Promise<RawInventoryData | null> =>
  inTauri() ? parseInv(await commands.openInventoryFile()) : setMockSource("manual");

export const openAlecaFrameInventoryFile = async (): Promise<RawInventoryData | null> =>
  inTauri() ? parseInv(await commands.openAlecaframeInventoryFile()) : setMockSource("aleca");

/** AlecaFrame's lastData.dat when it sits in its default folder, else null. */
export const findAlecaFrameCache = async (): Promise<string | null> =>
  inTauri() ? await commands.findAlecaframeCache() : "C:/Users/Tenno/AppData/Local/AlecaFrame/lastData.dat";

/** Imports a dropped file by path (`.dat` is an AlecaFrame cache). */
export const importInventoryPath = async (path: string): Promise<RawInventoryData | null> =>
  inTauri() ? parseInv(await commands.importInventoryPath(path)) : setMockSource(path.endsWith(".dat") ? "aleca" : "manual");

/** Forgets the imported snapshot and the deltas applied to it. */
export const clearInventory = async (): Promise<void> => {
  if (inTauri()) await commands.clearInventory();
  else setMockSource("none");
};

/** Reward overlay "I took this": one estimated copy of the named (English) relic reward. */
export const applyInventoryPick = async (name: string): Promise<boolean> =>
  inTauri() ? await commands.inventoryApplyPick(name) : true;

/** Files dropped on the window (absolute paths); no-op outside Tauri. */
export const onFilesDropped = async (cb: (paths: string[]) => void): Promise<UnlistenFn> =>
  inTauri()
    ? getCurrentWebview().onDragDropEvent((e) => e.payload.type === "drop" && cb(e.payload.paths))
    : () => {};

let mockAccount: string | null = null;
/** The public account id used for the profile endpoint; never read from the game. */
export const getProfileAccount = async (): Promise<string | null> =>
  inTauri() ? await commands.profileGetAccount() : mockAccount;

/** Saves the id found in a pasted id or profile link; empty clears. Throws when none is found. */
export const setProfileAccount = async (input: string): Promise<string | null> => {
  if (inTauri()) return unwrap(await commands.profileSetAccount(input));
  const id = input.toLowerCase().match(/[0-9a-f]{24}/)?.[0] ?? null;
  if (input.trim() && !id) throw new Error("invalid");
  return (mockAccount = id);
};

export const exportInventory = async (): Promise<ExportResult> =>
  inTauri() ? await commands.exportInventory() : { saved: true, path: "C:/inventory.json", error: null };

/** `null` means the inventory was cleared (source set to none). */
export const onInventoryUpdated = async (cb: (data: RawInventoryData | null) => void): Promise<UnlistenFn> =>
  inTauri() ? listen<string | null>("inventory-updated", (e) => cb(parseInv(e.payload)))
    : (mockListeners.add(cb), () => void mockListeners.delete(cb));

export const onInventoryStatusUpdated = async (cb: (status: InventoryStatus) => void): Promise<UnlistenFn> =>
  inTauri() ? listen<InventoryStatus>("inventory-status-updated", (e) => cb(e.payload)) : () => {};
// ---- External helper (warframe-api-helper) ----
let mockHelper: HelperStatus = {
  exeFound: false, running: false, lastRunAt: null, lastRunOk: null, lastRunReason: null,
  inventoryLastModified: null, installerAutoInstallHelper: null,
};
const mockProgressListeners = new Set<(p: HelperDownloadProgress) => void>();
const mockProgress = (p: Partial<HelperDownloadProgress> & Pick<HelperDownloadProgress, "stage">) =>
  mockProgressListeners.forEach((cb) => cb({ percent: 0, bytesReceived: 0, bytesTotal: 0, error: null, ...p }));

/** Custom titlebar controls; no-ops outside Tauri. */
export const windowMinimize = async (): Promise<void> => void (inTauri() && (await commands.windowMinimize()));
export const windowToggleMaximize = async (): Promise<void> => void (inTauri() && (await commands.windowToggleMaximize()));
export const windowClose = async (): Promise<void> => void (inTauri() && (await commands.windowClose()));

export const getHelperStatus = async (): Promise<HelperStatus> =>
  inTauri() ? await commands.helperGetStatus() : { ...mockHelper };

/** The manual "Run helper now": resolves to whether an inventory was fetched. */
export const runHelperNow = async (): Promise<boolean> => {
  if (inTauri()) return unwrap(await commands.helperRunNow());
  mockHelper = { ...mockHelper, lastRunAt: Date.now(), lastRunOk: false, lastRunReason: "game-not-running" };
  return false;
};

/** Downloads the pinned helper; progress arrives through `onHelperDownloadProgress`. */
export const downloadHelper = async (): Promise<{ ok: boolean; error: string | null }> => {
  if (inTauri()) {
    const r = unwrap(await commands.helperDownload());
    return { ok: r.ok, error: r.error ?? null };
  }
  mockProgress({ stage: "resolving" });
  mockProgress({ stage: "downloading", percent: 50, bytesReceived: 2_621_440, bytesTotal: 5_242_880 });
  mockProgress({ stage: "done", percent: 100, bytesReceived: 5_242_880, bytesTotal: 5_242_880 });
  mockHelper = { ...mockHelper, exeFound: true };
  return { ok: true, error: null };
};

export const onHelperDownloadProgress = async (cb: (p: HelperDownloadProgress) => void): Promise<UnlistenFn> =>
  inTauri() ? listen<HelperDownloadProgress>("helper-download-progress", (e) => cb(e.payload))
    : (mockProgressListeners.add(cb), () => void mockProgressListeners.delete(cb));

// ---- relics, drops, spawn nodes ----

export const getRelicDatabase = async (): Promise<RelicDatabase> =>
  inTauri() ? unwrap(await commands.getRelicDatabase()) : structuredClone(MOCK_RELIC_DB);

export const getRelicDataInfo = async (): Promise<RelicDataInfo> =>
  inTauri() ? commands.getRelicDataInfo() : { version: "1.1276.6", source: "bundled", publishedAt: null, updateError: null, updateOffline: false, newerUnusable: false };

/** Fires when a newer relic data set replaced the bundled one (or the bundle came back). */
export const onRelicDbUpdated = async (cb: () => void): Promise<UnlistenFn> =>
  inTauri() ? listen("relic-db-updated", cb) : () => {};

/** The loaded inventory as a plain record (relics read it); outside Tauri a relic-shaped fixture. */
export const getRawInventory = async (): Promise<Record<string, unknown> | null> =>
  inTauri() ? ((await getInventory()) as Record<string, unknown> | null) : mockSource === "none" ? null : structuredClone(MOCK_RELIC_INVENTORY);

const MOCK_DROPS: DropRow[] = [
  { item: "Forma Blueprint", place: "Arbitrations, Rotation C", rarity: "Rare", chance: 5.97, kind: "mission" },
  { item: "Forma Blueprint", place: "Arid Butcher", rarity: "Uncommon", chance: 12.5, kind: "enemy" },
  { item: "Forma Blueprint", place: "Lith A1 Relic (Radiant)", rarity: "Rare", chance: 2, kind: "relic" },
  { item: "Orokin Cell", place: "Arid Butcher", rarity: "Common", chance: 21.5, kind: "enemy" },
  { item: "Orokin Cell", place: "Cetus Bounty, Level 15 - 25", rarity: "Uncommon", chance: 11.11, kind: "bounty" },
];

export const dropSearch = async (query: string, mode: DropSearchMode = "item", limit = 300): Promise<DropSearchResult> => {
  if (inTauri()) return commands.dropSearch(query, mode, limit);
  const q = query.trim().toLowerCase();
  const rows = MOCK_DROPS.filter((r) => (mode === "item" ? r.item : mode === "place" ? r.place : r.kind === "enemy" ? r.place : "").toLowerCase().includes(q));
  return { rows, total: rows.length };
};

/** What the drop tables list for one item, best chance first. */
export const dropItemSources = async (name: string): Promise<DropRow[]> =>
  inTauri() ? commands.dropItemSources(name) : [];

const mockNode = (nodeId: string, node: string, mission: string, minLevel: number, maxLevel: number): SpawnNode => ({
  nodeId, node, nodeEn: node, planet: "Mars", planetEn: "Mars", planetIndex: 3, missionKey: "MT_DEFENSE", mission, missionEn: mission, factionKeys: ["grineer"], minLevel, maxLevel, tileSet: "Grineer Settlement", darkSector: false,
});
const MOCK_SPAWN_NODES = [mockNode("SolNode23", "Gradivus Dominus", "Defense", 8, 13), mockNode("SolNode66", "Tharsis", "Survival", 6, 11), mockNode("SolNode22", "Spear", "Exterminate", 7, 12)];

export const spawnNodesGet = async (): Promise<SpawnNode[]> => (inTauri() ? unwrap(await commands.spawnNodesGet()) : MOCK_SPAWN_NODES);
/** Cached by the backend for a minute; a failed fetch serves the last good state. */
export const getWorldState = async (): Promise<WorldState> =>
  inTauri() ? commands.getWorldState() : mockWorldState();

/** Fires when every world-state source failed; the payload is the error text. */
export const onWorldStateFetchError = async (cb: (message: string) => void): Promise<UnlistenFn> =>
  inTauri() ? listen<string>("world-state-fetch-error", (e) => cb(e.payload)) : () => {};

// ---- trackers ----
const IDLE_GAME: TrackersStatus = { game: { running: false, logged_in: false, mission_tag: null } };

export const getTrackersStatus = async (): Promise<TrackersStatus> =>
  inTauri() ? commands.trackersStatus() : IDLE_GAME;

/** Fires when the game starts or stops writing its log; the payload is the new state. */
export const onGameRunning = async (cb: (running: boolean) => void): Promise<UnlistenFn> =>
  inTauri() ? listen<boolean>("game-running", (e) => cb(e.payload)) : () => {};

/** Fires once per login to the game. */
export const onGameLogin = async (cb: () => void): Promise<UnlistenFn> =>
  inTauri() ? listen("game-login", cb) : () => {};

// warframe.market catalog. Errors throw (a transient failure is never "no data"); outside Tauri a tiny list answers.
// Unowned sets in the mastery fixture, so the roadmap can price what they lack.
const MOCK_SET_PARTS = ["Rhino Prime", "Khora", "Kogake Prime", "Amesha", "Venari"];
const slugOf = (name: string) => name.toLowerCase().replace(/ /g, "_");
const MOCK_WFM_ITEMS: CatalogItem[] = [
  { id: "mock_serration", slug: "serration", name: "Serration", thumb: null, icon: null, maxRank: 10, gameRef: "/Lotus/Upgrades/Mods/Rifle/WeaponDamageAmountMod" },
  { id: "mock_ash_set", slug: "ash_prime_set", name: "Ash Prime Set", thumb: null, icon: null, maxRank: null, gameRef: null },
  { id: "mock_boltor_prime", slug: "boltor_prime", name: "Boltor Prime", thumb: null, icon: null, maxRank: null, gameRef: null },
  { id: "mock_lex_prime", slug: "lex_prime", name: "Lex Prime", thumb: null, icon: null, maxRank: null, gameRef: null },
  { id: "mock_point_strike", slug: "point_strike", name: "Point Strike", thumb: null, icon: null, maxRank: null, gameRef: "/Lotus/Upgrades/Mods/Rifle/WeaponCritChanceMod" },
  { id: "mock_vitality", slug: "vitality", name: "Vitality", thumb: null, icon: null, maxRank: null, gameRef: "/Lotus/Upgrades/Mods/Warframe/AvatarHealthMaxMod" },
  ...["Rhino Prime Neuroptics"].map((n) => ({ id: `mock_${slugOf(n)}`, slug: slugOf(n), name: n, thumb: null, icon: null, maxRank: null, gameRef: null })),
  ...MOCK_SET_PARTS.flatMap((name) => [`${name} Set`, `${name} Chassis`, `${name} Systems`].map((n) => ({ id: `mock_${slugOf(n)}`, slug: slugOf(n), name: n, thumb: null, icon: null, maxRank: null, gameRef: null }))),
];
export const getWfmItems = async (): Promise<CatalogItem[]> =>
  inTauri()
    ? unwrap(await commands.getWfmItems())
    : [...MOCK_WFM_ITEMS, ...Array.from({ length: bigInv() }, (_, i) => ({ id: `mock_bulk_${i}`, slug: slugOf(bigName(i)), name: bigName(i), thumb: null, icon: null, maxRank: null, gameRef: bigPath(i) }))];

/** Live details of one item; null when warframe.market has no such item (400/404). */
export const wfmItem = async (slug: string): Promise<WfmItem | null> =>
  inTauri() ? unwrap(await commands.wfmItem(slug)) : null;

export const wfmRivenWeapons = async (): Promise<RivenWeapon[]> =>
  inTauri() ? unwrap(await commands.wfmRivenWeapons()) : [];

// warframe.market prices. A transient failure with nothing cached throws; "no data" is a status, never an error.

const MOCK_PRICES: Record<string, number> = {
  serration: 22,
  ash_prime_set: 64,
  boltor_prime: 14,
  lato_prime: 6,
  vitality: 3,
  // Rewards of the mock Meso C2 / F3 relics, so the planner previews show a plat EV.
  ember_prime_neuroptics: 18, kronen_prime_handle: 9, nekros_prime_systems: 11, scindo_prime_blade: 6, zephyr_prime_chassis: 14,
  boar_prime_receiver: 5, equinox_prime_chassis: 12, gara_prime_systems: 30, mesa_prime_blueprint: 45, wukong_prime_neuroptics: 7,
  rhino_prime_set: 120, rhino_prime_neuroptics: 20, rhino_prime_systems: 35,
  ...Object.fromEntries(MOCK_SET_PARTS.flatMap((name, i) => [[slugOf(`${name} Set`), 90 + i * 25], [slugOf(`${name} Chassis`), 15 + i * 6], [slugOf(`${name} Systems`), 12 + i * 5]])),
};
/** Ranked mods trade higher maxed; the mock says so for Serration (rank 10). */
const MOCK_MAX_PRICES: Record<string, number> = { serration: 48 };
const mockPrice = (slug: string): PriceEntry => {
  const price = MOCK_PRICES[slug] ?? (slug.startsWith("bulk_mod_") ? bigPrice(slug) : null);
  return { slug, status: price == null ? "no_data" : "ok", price, maxPrice: MOCK_MAX_PRICES[slug] ?? null, maxRank: MOCK_MAX_PRICES[slug] ? 10 : null, stale: false, updatedAt: null };
};
export const wfmPrice = async (slug: string): Promise<PriceEntry> =>
  inTauri() ? unwrap(await commands.wfmPrice(slug)) : mockPrice(slug);

/** Many slugs at once (inventory); a slug that fails comes back `failed` instead of rejecting the call. */
export const wfmPricesBulk = async (slugs: string[]): Promise<PriceEntry[]> =>
  inTauri() ? unwrap(await commands.wfmPricesBulk(slugs)) : [...new Set(slugs)].map(mockPrice);

/** Daily closed-sales history (up to a year) for the Browse chart. */
export const wfmStatsHistory = async (slug: string): Promise<MarketStatPoint[]> =>
  inTauri() ? unwrap(await commands.wfmStatsHistory(slug)) : mockHistory(slug);

// --- warframe.market presence and socket ---
export type { PresenceOptions, Status as WfmStatus, WsStatus } from "../bindings";
export type WfmPresenceState = PresenceState_Deserialize;
const IDLE_PRESENCE: WfmPresenceState = { status: null, expires_at: null, auto_active: false, away_active: false };
let mockPresence: WfmPresenceState = IDLE_PRESENCE;
const mockPresenceListeners = new Set<(s: WfmPresenceState) => void>();

export const wfmPresenceGet = async (): Promise<WfmPresenceState> =>
  inTauri() ? unwrap(await commands.wfmPresenceGet()) : mockPresence;

/** A manual pick: wins over the auto-ingame and away rules until the next game or idle edge. */
export const wfmPresenceSet = async (status: WfmStatus): Promise<WfmPresenceState> => {
  if (inTauri()) return unwrap(await commands.wfmPresenceSet(status));
  mockPresence = { ...IDLE_PRESENCE, status };
  mockPresenceListeners.forEach((cb) => cb(mockPresence));
  return mockPresence;
};

/** Push the persisted presence settings; call on boot and on every save. */
export const wfmPresenceSetOptions = async (options: PresenceOptions): Promise<void> => {
  if (inTauri()) unwrap(await commands.wfmPresenceSetOptions(options));
};

export const wfmWsStatus = async (): Promise<WsStatus> => (inTauri() ? commands.wfmWsStatus() : "stopped");

export const onWfmPresence = async (cb: (s: WfmPresenceState) => void): Promise<UnlistenFn> => {
  if (inTauri()) return listen<WfmPresenceState>("wfm-presence", (e) => cb(e.payload));
  mockPresenceListeners.add(cb);
  return () => mockPresenceListeners.delete(cb);
};

export const onWfmWsStatus = async (cb: (s: WsStatus) => void): Promise<UnlistenFn> =>
  inTauri() ? listen<WsStatus>("wfm-ws-status", (e) => cb(e.payload)) : () => {};

// --- Notifications: channels, webhooks, history and sound. Webhook URLs never come back, only masked. ---
const freshChannelState = (): ChannelState => ({
  webhooks: { discord: { configured: false, masked: "" }, generic: { configured: false, masked: "" } },
  sources: { ...DEFAULT_SOURCE_CHANNELS },
  nativeOnlyWhileGameRunning: false,
  discordPingUserId: "",
  soundEnabled: true,
  soundVolume: 1,
});
let mockChannels: ChannelState = freshChannelState();
let mockLog: NotificationEntry[] = [];
let mockSound: SoundAsset | null = null;
const mockLogListeners = new Set<(e: NotificationEntry) => void>();
const copyChannels = (): ChannelState => structuredClone(mockChannels);
const patchChannels = (patch: Partial<ChannelState>): ChannelState => ((mockChannels = { ...mockChannels, ...patch }), copyChannels());

export const notifGetState = async (): Promise<ChannelState> => (inTauri() ? commands.notifGetState() : copyChannels());

export type SetWebhookResult = { ok: true; state: ChannelState } | { ok: false; error: WebhookUrlError };
export const notifSetWebhook = async (channel: WebhookChannel, url: string): Promise<SetWebhookResult> => {
  if (inTauri()) {
    const r = await commands.notifSetWebhook(channel, url);
    return r.status === "ok" ? { ok: true, state: r.data } : { ok: false, error: r.error };
  }
  const text = url.trim();
  if (!text) return { ok: false, error: "empty" };
  if (!text.startsWith("https://")) return { ok: false, error: "not-https" };
  const masked = `${text.split("/").slice(0, 3).join("/")}/...${text.slice(-4)}`;
  return { ok: true, state: patchChannels({ webhooks: { ...mockChannels.webhooks, [channel]: { configured: true, masked } } }) };
};

export const notifClearWebhook = async (channel: WebhookChannel): Promise<ChannelState> =>
  inTauri()
    ? commands.notifClearWebhook(channel)
    : patchChannels({ webhooks: { ...mockChannels.webhooks, [channel]: { configured: false, masked: "" } } });

export type WebhookTestResult = { ok: true } | { ok: false; error: WebhookTestError };
export const notifTestWebhook = async (channel: WebhookChannel): Promise<WebhookTestResult> => {
  if (!inTauri()) return mockChannels.webhooks[channel]?.configured ? { ok: true } : { ok: false, error: "not-configured" };
  const r = await commands.notifTestWebhook(channel);
  return r.status === "ok" ? { ok: true } : { ok: false, error: r.error };
};

export const notifSetSource = async (source: NotificationSource, toggles: SourceToggles): Promise<ChannelState> =>
  inTauri() ? commands.notifSetSource(source, toggles) : patchChannels({ sources: { ...mockChannels.sources, [source]: { ...toggles } } });

export const notifSetGameGate = async (enabled: boolean): Promise<ChannelState> =>
  inTauri() ? commands.notifSetGameGate(enabled) : patchChannels({ nativeOnlyWhileGameRunning: enabled });

export type SetDiscordPingResult = { ok: true; state: ChannelState } | { ok: false; error: DiscordPingError };
export const notifSetDiscordPing = async (userId: string): Promise<SetDiscordPingResult> => {
  if (inTauri()) {
    const r = await commands.notifSetDiscordPing(userId);
    return r.status === "ok" ? { ok: true, state: r.data } : { ok: false, error: r.error };
  }
  const id = userId.trim();
  return id === "" || /^[0-9]{17,20}$/.test(id)
    ? { ok: true, state: patchChannels({ discordPingUserId: id }) }
    : { ok: false, error: "invalid-user-id" };
};

/** Newest first. */
export const notifLogGet = async (): Promise<NotificationEntry[]> =>
  inTauri() ? unwrap(await commands.notifLogGet()) : [...mockLog].reverse();

export const notifLogRemove = async (id: string): Promise<boolean> => {
  if (inTauri()) return unwrap(await commands.notifLogRemove(id));
  const before = mockLog.length;
  mockLog = mockLog.filter((e) => e.id !== id);
  return mockLog.length < before;
};

export const notifLogClear = async (): Promise<void> => {
  if (inTauri()) {
    unwrap(await commands.notifLogClear());
    return;
  }
  mockLog = [];
};

export const onNotificationAdded = async (cb: (e: NotificationEntry) => void): Promise<UnlistenFn> => {
  if (inTauri()) return listen<NotificationEntry>("notification-added", (e) => cb(e.payload));
  mockLogListeners.add(cb);
  return () => mockLogListeners.delete(cb);
};

/** Mock only: what a native delivery does in the backend (history row + push). */
export const mockNotify = (kind: NotificationEntry["kind"], title: string, body = ""): NotificationEntry => {
  const entry: NotificationEntry = { id: `mock-${mockLog.length}-${Date.now()}`, at: new Date().toISOString(), kind, title, body };
  mockLog = [...mockLog, entry].slice(-NOTIFICATION_LOG_MAX_ENTRIES);
  mockLogListeners.forEach((cb) => cb(entry));
  return entry;
};

export const notifSoundGet = async (): Promise<SoundAsset | null> => (inTauri() ? commands.notifSoundGet() : mockSound);

export const notifSoundSet = async (upload: SoundUpload): Promise<SoundAsset> => {
  if (inTauri()) return unwrap(await commands.notifSoundSet(upload));
  return (mockSound = { name: upload.name.trim(), dataUrl: `data:audio/wav;base64,${upload.data}`, revision: String(upload.data.length) });
};

export const notifSoundReset = async (): Promise<void> => {
  if (inTauri()) return commands.notifSoundReset();
  mockSound = null;
};

export const notifSoundSetPrefs = async (enabled: boolean, volume: number): Promise<ChannelState> =>
  inTauri()
    ? commands.notifSoundSetPrefs(enabled, volume)
    : patchChannels({ soundEnabled: enabled, soundVolume: Math.min(1, Math.max(0, Number.isFinite(volume) ? volume : 1)) });

export const notifSoundPreview = async (): Promise<void> => {
  if (inTauri()) return commands.notifSoundPreview();
};

export const arbiRunsGet = async (): Promise<ArbiRunsPayload> =>
  inTauri() ? unwrap(await commands.arbiRunsGet()) : { runs: MOCK_ARBI_RUNS() as unknown as ArbiRunRecord[], diskUsageBytes: 120_000 };

export const arbiRunGet = async (id: string): Promise<ArbiRunRecord | null> =>
  inTauri() ? commands.arbiRunGet(id) : null;

/** The updated record; null for an unknown id or a value outside 0..=10M. */
export const arbiRunSetVitus = async (id: string, vitus: number | null): Promise<ArbiRunRecord | null> =>
  inTauri() ? commands.arbiRunSetVitus(id, vitus) : null;

/** Tags and notes as the backend normalized and stored them; null for an unknown id. */
export const runSetTags = async (id: string, tags: string[]): Promise<string[] | null> =>
  inTauri() ? commands.runSetTags(id, tags) : tags;

export const runSetNotes = async (id: string, notes: string): Promise<string | null> =>
  inTauri() ? commands.runSetNotes(id, notes) : notes;

/** Drops the raw capture and keeps the record. */
export const runDeleteLog = async (id: string): Promise<boolean> => (inTauri() ? commands.runDeleteLog(id) : false);

export const runDelete = async (id: string): Promise<boolean> => (inTauri() ? commands.runDelete(id) : false);

/** Splits an external EE.log into runs; already stored runs count as skipped. */
export const arbiImportLog = async (path: string): Promise<ArbiImportResult> =>
  inTauri() ? unwrap(await commands.arbiImportLog(path)) : { imported: [], skipped: 0 };

export const onArbiRunSaved = async (cb: (run: ArbiRunRecord) => void): Promise<UnlistenFn> =>
  inTauri() ? listen<ArbiRunRecord>("arbi-run-saved", (e) => cb(e.payload)) : () => {};

// --- World-state notification preferences ---
const freshWorldNotify = (): WorldNotifyPrefs => ({
  enabled: true,
  cycleAlerts: { earth: false, cetus: false, vallis: false, cambion: false, duviri: false },
  cycleAlertMinutesBefore: 3,
  fissureAlerts: [],
});
let mockWorldNotify: WorldNotifyPrefs = freshWorldNotify();

export const worldNotifyGet = async (): Promise<WorldNotifyPrefs> =>
  inTauri() ? commands.worldNotifyGet() : structuredClone(mockWorldNotify);

export const worldNotifySet = async (prefs: WorldNotifyPrefs): Promise<WorldNotifyPrefs> => {
  if (inTauri()) return unwrap(await commands.worldNotifySet(prefs));
  mockWorldNotify = { ...structuredClone(prefs), cycleAlertMinutesBefore: Math.min(120, prefs.cycleAlertMinutesBefore) };
  return structuredClone(mockWorldNotify);
};

export const ptRunsGet = async (): Promise<PtRunsPayload> =>
  inTauri() ? (unwrap(await commands.ptRunsGet()) as PtRunsPayload) : { runs: MOCK_PT_RUNS(), diskUsageBytes: 8_000 };

export const ptRunGet = async (id: string): Promise<PtRunRecord | null> =>
  inTauri() ? ((await commands.ptRunGet(id)) as PtRunRecord | null) : null;

/** Splits an external EE.log into Profit Taker runs; already stored runs count as skipped. */
export const ptImportLog = async (path: string): Promise<PtImportResult> =>
  inTauri() ? (unwrap(await commands.ptImportLog(path)) as PtImportResult) : { imported: [], skipped: 0 };

export const onPtRunSaved = async (cb: (run: PtRunRecord) => void): Promise<UnlistenFn> =>
  inTauri() ? listen<PtRunRecord>("pt-run-saved", (e) => cb(e.payload)) : () => {};

// Test hook: restores the in-memory mock to its defaults.
export const resetMock = (): void => {
  mockSettings = { ...DEFAULT_SETTINGS };
  resetMockAlerts();
  mockContracts = freshContracts();
  mockSource = "manual";
  mockAccount = null;
  mockListeners.clear();
  mockProgressListeners.clear();
  mockHelper = { ...mockHelper, exeFound: false, lastRunAt: null, lastRunOk: null, lastRunReason: null };

  mockPresence = IDLE_PRESENCE;
  mockPresenceListeners.clear();
  mockOrders = [];
  mockJobs = [];
  mockSession = LOGGED_OUT;
  mockWorkbench = IDLE_WORKBENCH;

  mockChannels = freshChannelState();
  mockLog = [];
  mockSound = null;
  mockLogListeners.clear();
  mockWorldNotify = freshWorldNotify();
};

// Orders. Creating or changing an order is only ever called from a user action. Errors throw an Error with the
// WFM detail; a missing-subtype refusal also carries `code: "subtype_required"` and `subtypes` (see `subtypeChoicesOf`).
export type { MyOrder, MyOrders, BookOrder, CreateOrder, OrderUpdates };

export const wfmErrorOf = (e: { kind: string; detail?: string; status?: number; subtypes?: string[]; retry_after_secs?: number | null }): Error => {
  if (e.kind === "subtype_required") {
    return Object.assign(new Error(`warframe.market needs a subtype for this item: ${(e.subtypes ?? []).join(", ")}`), {
      code: "subtype_required",
      subtypes: e.subtypes ?? [],
    });
  }
  return Object.assign(new Error(e.detail ?? (e.kind === "rate_limited" ? "Rate limited by warframe.market." : e.kind)), { kind: e.kind, status: e.status });
};
const unwrapWfm = <T>(r: { status: "ok"; data: T } | { status: "error"; error: Parameters<typeof wfmErrorOf>[0] }): T => {
  if (r.status === "error") throw wfmErrorOf(r.error);
  return r.data;
};

let mockOrders: MyOrder[] = [];
let mockOrderSeq = 0;
const mockOrderSplit = (): MyOrders => ({
  sell: mockOrders.filter((o) => o.orderType === "sell"),
  buy: mockOrders.filter((o) => o.orderType === "buy"),
});
const mockOrderById = (id: string): MyOrder => {
  const o = mockOrders.find((x) => x.id === id);
  if (!o) throw new Error("Order not found.");
  return o;
};

export const wfmOrdersMy = async (): Promise<MyOrders> => {
  const t0 = performance.now();
  const orders = inTauri() ? unwrapWfm(await commands.wfmOrdersMy()) : mockOrderSplit();
  logTiming("fetch_orders", { orders: orders.sell.length + orders.buy.length, ms: msSince(t0) });
  return orders;
};

/** The cached copy of my orders at once (`orders: null`: nothing fetched yet); `refreshing` says a background refresh was started and will arrive through `onWfmOrdersUpdated`. */
export const wfmOrdersMyCached = async (): Promise<{ orders: MyOrders | null; refreshing: boolean }> =>
  inTauri() ? unwrapWfm(await commands.wfmOrdersMyCached()) : { orders: mockOrderSplit(), refreshing: false };

/** The Refresh button: always asks warframe.market. */
export const wfmOrdersRefresh = async (): Promise<MyOrders> => (inTauri() ? unwrapWfm(await commands.wfmOrdersRefresh()) : mockOrderSplit());

/** Hides or shows every order of one side in a single request; the new state arrives through `onWfmOrdersUpdated`. Returns how many changed. */
export const wfmOrdersSetAllVisible = async (orderType: "sell" | "buy", visible: boolean): Promise<number> => {
  if (inTauri()) return unwrapWfm(await commands.wfmOrdersSetAllVisible(orderType, visible));
  const changed = mockOrders.filter((o) => o.orderType === orderType && o.visible !== visible).length;
  mockOrders = mockOrders.map((o) => (o.orderType === orderType ? { ...o, visible } : o));
  notifyMockOrders();
  return changed;
};

const mockOrderListeners = new Set<(o: MyOrders) => void>();
const notifyMockOrders = () => mockOrderListeners.forEach((cb) => cb(mockOrderSplit()));

/** Every change to my orders (fetch, confirmed write, trade close, sign-out): the whole new copy. */
export const onWfmOrdersUpdated = async (cb: (o: MyOrders) => void): Promise<UnlistenFn> => {
  if (inTauri()) return listen<MyOrders>("wfm-orders-updated", (e) => cb(e.payload));
  mockOrderListeners.add(cb);
  return () => void mockOrderListeners.delete(cb);
};

export const wfmOrderCreate = async (order: CreateOrder): Promise<MyOrder> => {
  if (inTauri()) return unwrapWfm(await commands.wfmOrderCreate(order));
  const item = MOCK_WFM_ITEMS.find((i) => i.id === order.itemId);
  const created: MyOrder = {
    id: (++mockOrderSeq).toString(16).padStart(24, "0"),
    orderType: order.orderType.toLowerCase(),
    platinum: order.platinum,
    quantity: order.quantity,
    perTrade: order.perTrade ?? 1,
    visible: order.visible ?? true,
    modRank: order.modRank ?? null,
    subtype: order.subtype?.toLowerCase() ?? null,
    itemId: order.itemId,
    itemName: item?.name ?? "(unknown)",
    itemSlug: item?.slug ?? null,
    itemThumb: item?.thumb ?? null,
    createdAt: new Date().toISOString(),
    updatedAt: null,
  };
  mockOrders = [...mockOrders, created];
  notifyMockOrders();
  return created;
};

export const wfmOrderUpdate = async (orderId: string, updates: OrderUpdates): Promise<MyOrder> => {
  if (inTauri()) return unwrapWfm(await commands.wfmOrderUpdate(orderId, updates));
  const next: MyOrder = {
    ...mockOrderById(orderId),
    ...(updates.platinum != null && { platinum: updates.platinum }),
    ...(updates.quantity != null && { quantity: updates.quantity }),
    ...(updates.visible != null && { visible: updates.visible }),
    ...(updates.modRank != null && { modRank: updates.modRank }),
    ...(updates.subtype != null && { subtype: updates.subtype.toLowerCase() }),
  };
  mockOrders = mockOrders.map((o) => (o.id === orderId ? next : o));
  notifyMockOrders();
  return next;
};

export const wfmOrderDelete = async (orderId: string): Promise<{ id: string }> => {
  if (inTauri()) return unwrapWfm(await commands.wfmOrderDelete(orderId));
  mockOrderById(orderId);
  mockOrders = mockOrders.filter((o) => o.id !== orderId);
  notifyMockOrders();
  return { id: orderId };
};

/** Closes `quantity` units (a whole trade for bulk listings). The backend lowers its copy of my orders and announces it. */
export const wfmOrderClose = async (orderId: string, quantity: number, perTrade: number | null = null): Promise<{ id: string }> => {
  if (inTauri()) return unwrapWfm(await commands.wfmOrderClose(orderId, quantity, perTrade));
  const o = mockOrderById(orderId);
  mockOrders =
    o.quantity <= quantity
      ? mockOrders.filter((x) => x.id !== orderId)
      : mockOrders.map((x) => (x.id === orderId ? { ...x, quantity: x.quantity - quantity } : x));
  notifyMockOrders();
  return { id: orderId };
};

// --- Background jobs ---
import type { Job_Serialize as Job, JobItem_Serialize as JobItem, JobKind, JobUpdate_Serialize as JobUpdate } from "../bindings";
export type { Job, JobItem, JobKind, JobUpdate };

let mockJobs: Job[] = [];
let mockJobSeq = 0;
const mockJobListeners = new Set<(u: JobUpdate) => void>();
const jobUpdate = (j: Job, item: JobItem | null, withJob = false): JobUpdate => ({
  id: j.id,
  state: j.state,
  total: j.total,
  done: j.done,
  failed: j.failed,
  item: item && { id: item.id, status: item.status, reason: item.reason },
  job: withJob ? j : null,
});
const sendJob = (u: JobUpdate) => mockJobListeners.forEach((cb) => cb(u));

/** Browser and test stand-in for the backend runner: one item at a time through the mock order functions. */
const mockStartJob = (kind: JobKind, items: JobItem[]): string => {
  const job: Job = { id: `job-${++mockJobSeq}`, kind, state: "running", total: items.length, done: 0, failed: 0, items, createdAt: Date.now(), finishedAt: null };
  mockJobs.push(job);
  sendJob(jobUpdate(job, null, true));
  void (async () => {
    for (const item of job.items) {
      if (job.state === "stopping") {
        item.status = "skipped";
        continue;
      }
      item.status = "running";
      sendJob(jobUpdate(job, item));
      try {
        if (kind.type === "ordersDelete") await wfmOrderDelete(item.id);
        else await wfmOrderUpdate(item.id, (kind.type === "reprice" ? { platinum: item.platinum } : { visible: kind.type === "ordersVisibility" && kind.visible }) as never);
        item.status = "done";
        job.done++;
      } catch (e) {
        item.status = "failed";
        item.reason = (e as Error).message;
        job.failed++;
      }
      sendJob(jobUpdate(job, item));
    }
    job.state = "done";
    job.finishedAt = Date.now();
    sendJob(jobUpdate(job, null));
  })();
  return job.id;
};
const mockJobItems = (ids: string[], price?: Map<string, number>): JobItem[] =>
  ids.map((id) => ({ id, name: mockOrderById(id).itemName, platinum: price?.get(id) ?? null, status: "pending", reason: null }));
const mockRedo = (id: string, status: JobItem["status"]): string => {
  const job = mockJobs.find((j) => j.id === id);
  if (!job) throw new Error("No such job.");
  mockJobs = mockJobs.filter((j) => j !== job);
  return mockStartJob(job.kind, job.items.filter((i) => i.status === status).map((i) => ({ ...i, status: "pending", reason: null })));
};

/** Hide or show these orders as one background job; progress arrives through `onJobUpdated`. */
export const jobsStartOrdersVisibility = async (ids: string[], visible: boolean): Promise<string> =>
  inTauri() ? unwrap(await commands.jobsStartOrdersVisibility(ids, visible)) : mockStartJob({ type: "ordersVisibility", visible }, mockJobItems(ids));
export const jobsStartOrdersDelete = async (ids: string[]): Promise<string> =>
  inTauri() ? unwrap(await commands.jobsStartOrdersDelete(ids)) : mockStartJob({ type: "ordersDelete" }, mockJobItems(ids));
export const jobsStartReprice = async (items: { id: string; platinum: number }[]): Promise<string> =>
  inTauri() ? unwrap(await commands.jobsStartReprice(items)) : mockStartJob({ type: "reprice" }, mockJobItems(items.map((i) => i.id), new Map(items.map((i) => [i.id, i.platinum]))));
/** The item in flight finishes, no further one starts. */
export const jobsStop = async (id: string): Promise<void> => {
  if (inTauri()) return void unwrap(await commands.jobsStop(id));
  const job = mockJobs.find((j) => j.id === id);
  if (job?.state === "running") {
    job.state = "stopping";
    sendJob(jobUpdate(job, null));
  }
};
/** A new job with only the failed items. */
export const jobsRetryFailed = async (id: string): Promise<string> => (inTauri() ? unwrap(await commands.jobsRetryFailed(id)) : mockRedo(id, "failed"));
/** A new job with the items an interrupted one never ran. */
export const jobsContinue = async (id: string): Promise<string> => (inTauri() ? unwrap(await commands.jobsContinue(id)) : mockRedo(id, "pending"));
export const jobsDismiss = async (id: string): Promise<void> => {
  if (inTauri()) return void unwrap(await commands.jobsDismiss(id));
  mockJobs = mockJobs.filter((j) => j.id !== id);
};
/** Jobs running or needing attention (failures, interrupted). */
export const jobsList = async (): Promise<Job[]> => (inTauri() ? commands.jobsList() : mockJobs.filter((j) => j.state !== "done" || j.failed > 0));
export const onJobUpdated = async (cb: (u: JobUpdate) => void): Promise<UnlistenFn> => {
  if (inTauri()) return listen<JobUpdate>("job-updated", (e) => cb(e.payload));
  mockJobListeners.add(cb);
  return () => void mockJobListeners.delete(cb);
};

/** Public orders of an item, both sides in one list (each row has `type`); pass them to `normalizeWfmOrderBookSide`.
 *  Leave `rank` unset for veiled rivens: they have no rank and `rank: 0` empties the book. */
export const wfmOrdersItem = async (slug: string, opts: { top?: boolean; rank?: number | null; subtype?: string | null } = {}): Promise<BookOrder[]> =>
  inTauri() ? unwrapWfm(await commands.wfmOrdersItem(slug, opts.top ?? false, opts.rank ?? null, opts.subtype ?? null)) : mockBook(slug);

// warframe.market session. The password only ever travels as an argument: it is never stored or logged here.
export type WfmSession = SessionStatus_Serialize;
const LOGGED_OUT: WfmSession = { logged_in: false, user_name: null, platform: null, expired: false, persistable: true, source: null };
let mockSession: WfmSession = LOGGED_OUT;

export const wfmSessionStatus = async (): Promise<WfmSession> =>
  inTauri() ? commands.wfmSessionStatus() : mockSession;

export const wfmSignIn = async (email: string, password: string): Promise<WfmSession> => {
  if (inTauri()) return unwrapWfm(await commands.wfmSignIn(email, password));
  if (password === "wrong") throw wfmErrorOf({ kind: "unauthorized", detail: "Wrong email or password." });
  mockSession = { ...LOGGED_OUT, logged_in: true, user_name: email.split("@")[0] };
  return mockSession;
};

/** Sign in on the official warframe.market page in its own window; the password never reaches the app. */
export const wfmSignInWebview = async (): Promise<WfmSession> => {
  if (inTauri()) return unwrapWfm(await commands.wfmSignInWebview());
  mockSession = { ...LOGGED_OUT, logged_in: true, user_name: "WebUser", source: "webview" };
  return mockSession;
};
export const wfmSignInWebviewCancel = async (): Promise<void> => {
  if (inTauri()) await commands.wfmSignInWebviewCancel();
};

export const wfmSignOut = async (): Promise<WfmSession> => {
  if (inTauri()) return commands.wfmSignOut();
  mockOrders = [];
  mockSession = LOGGED_OUT;
  mockWorkbench = IDLE_WORKBENCH;
  return mockSession;
};

// Riven contracts, auction search and +rep. v1-only on the WFM side and may disappear: `wfmContractsMy` then
// resolves an empty page with `unavailable` set instead of throwing. Creating, changing or closing an auction and
// sending +rep are only ever called from a user action. Errors throw like the order wrappers.
export type { WfmContract, WfmContractsPage, RivenSearchOptions, RivenAuctionCreate, RivenAuctionUpdate, SendRepResult };

const seedContract = (id: string, itemName: string, platinum: number, stats: [string, boolean][]): WfmContract => ({
  id, itemName, itemId: null, itemUrlName: null, weaponUrlName: itemName.split(" ")[0].toLowerCase(), rivenSuffix: itemName.split(" ")[1]?.toLowerCase() ?? null,
  itemThumb: null, platinum, buyoutPlatinum: platinum, startingPlatinum: Math.max(1, platinum - 20), quantity: 1, visible: true, modRank: 8, rerolls: 4,
  masteryLevel: 12, polarity: "madurai", minimalReputation: 0, isDirectSell: false, listedAt: null, updatedAt: null, note: null, listingUrl: `https://warframe.market/auction/${id}`,
  stats: stats.map(([urlName, positive]) => ({ urlName, label: urlName.replace(/_/g, " "), value: positive ? 90 : -40, positive })), sourceType: null, seller: null, sellerStatus: null,
});
let mockContracts: WfmContract[] = [];
const freshContracts = () => [
  seedContract("a1", "Rubico Critacan", 150, [["critical_chance", true], ["multishot", true], ["damage_vs_grineer", false]]),
  seedContract("a2", "Kuva Bramma Visiata", 80, [["critical_damage", true], ["fire_rate", true]]),
];
mockContracts = freshContracts();

export const wfmContractsMy = async (page: number | null = null, limit: number | null = null): Promise<WfmContractsPage> =>
  inTauri()
    ? unwrapWfm(await commands.wfmContractsMy(page, limit))
    : { contracts: mockContracts, page: page ?? 1, totalPages: null, hasMore: false, unavailable: null };

/** Similar rivens for sale, cheapest first; rows from other users carry `seller`. */
export const wfmRivenSearch = async (weaponSlug: string, options: RivenSearchOptions | null = null): Promise<WfmContract[]> =>
  inTauri() ? unwrapWfm(await commands.wfmRivenSearch(weaponSlug, options)) : [];

export const wfmRivenAuctionCreate = async (auction: RivenAuctionCreate): Promise<{ id: string | null }> =>
  inTauri() ? unwrapWfm(await commands.wfmRivenAuctionCreate(auction)) : { id: "mock-auction" };

export const wfmRivenAuctionUpdate = async (auctionId: string, update: RivenAuctionUpdate): Promise<{ id: string | null }> => {
  if (inTauri()) return unwrapWfm(await commands.wfmRivenAuctionUpdate(auctionId, update));
  mockContracts = mockContracts.map((c) =>
    c.id === auctionId ? { ...c, platinum: update.buyoutPrice ?? update.startingPrice ?? c.platinum, buyoutPlatinum: update.buyoutPrice || null, visible: update.visible ?? c.visible } : c,
  );
  return { id: auctionId };
};

export const wfmRivenAuctionClose = async (auctionId: string): Promise<void> => {
  if (inTauri()) unwrapWfm(await commands.wfmRivenAuctionClose(auctionId));
  else mockContracts = mockContracts.filter((c) => c.id !== auctionId);
};

export const wfmSendPlusRep = async (username: string): Promise<SendRepResult> =>
  inTauri() ? unwrapWfm(await commands.wfmSendPlusRep(username)) : "sent";

/** The signed-in user's warframe.market reputation, or null when unknown. */
export const wfmMyReputation = async (): Promise<number | null> => (inTauri() ? unwrapWfm(await commands.wfmMyReputation()) : null);

export type ArbiScheduleEntry = Finite<ArbiScheduleEntryBinding>;
export type ArbiScheduleAlerts = ArbiScheduleAlertsBinding;
export type ArbiSchedulePayload = { entries: ArbiScheduleEntry[]; fetchedAt: number | null; alerts: ArbiScheduleAlerts };

let mockSchedule: ArbiScheduleAlerts = { occurrences: [], favoriteNodes: [], minutesBefore: 5 };
const mockScheduleEntries = (): ArbiScheduleEntry[] => {
  const hour = 3_600_000;
  const start = Math.floor(Date.now() / hour) * hour;
  const nodes: [string, string, string, string][] = [
    ["SolNode149", "Casta (Ceres)", "Defense", "Grineer"],
    ["SolNode167", "Oestrus (Eris)", "Infested Salvage", "Infested"],
    ["SolNode23", "Hydron (Sedna)", "Defense", "Grineer"],
  ];
  return Array.from({ length: 12 }, (_, i) => {
    const [nodeId, node, mission, faction] = nodes[i % nodes.length];
    return { epochMs: start + i * hour, nodeId, node, mission, faction };
  });
};

export const arbiScheduleGet = async (): Promise<ArbiSchedulePayload> => {
  if (!inTauri()) return { entries: mockScheduleEntries(), fetchedAt: Date.now(), alerts: mockSchedule };
  const p = await commands.arbiScheduleGet();
  return { entries: p.entries as ArbiScheduleEntry[], fetchedAt: p.fetchedAt, alerts: p.alerts };
};

const withAlerts = async (call: () => Promise<ArbiScheduleAlerts | null>, mock: () => ArbiScheduleAlerts) =>
  (inTauri() ? await call() : (mockSchedule = mock()));

export const arbiScheduleSetOccurrence = (key: string, enabled: boolean) =>
  withAlerts(() => commands.arbiScheduleSetOccurrence(key, enabled), () => ({
    ...mockSchedule,
    occurrences: enabled ? [...new Set([...mockSchedule.occurrences, key])] : mockSchedule.occurrences.filter((k) => k !== key),
  }));

export const arbiScheduleSetFavorite = (nodeId: string, enabled: boolean) =>
  withAlerts(() => commands.arbiScheduleSetFavorite(nodeId, enabled), () => ({
    ...mockSchedule,
    favoriteNodes: enabled ? [...new Set([...mockSchedule.favoriteNodes, nodeId])] : mockSchedule.favoriteNodes.filter((n) => n !== nodeId),
  }));

export const arbiScheduleSetLead = (minutes: number) =>
  withAlerts(() => commands.arbiScheduleSetLead(minutes), () => ({ ...mockSchedule, minutesBefore: minutes }));

// --- Live trade tracker ---
/** A trade read from EE.log, already in the ledger, with the listings it would close (never auto-closed). */
export type { TradeDetected };
export const onTradeDetected = async (cb: (t: TradeDetected) => void): Promise<UnlistenFn> =>
  inTauri() ? listen<TradeDetected>("trade-detected", (e) => cb(e.payload)) : () => {};

// --- Live riven roll sessions ---
export type { RivenRollSession, RivenBestAttributes, RivenGoodRollsInfo };

const mockRivenSessions = (): RivenRollSession[] => [
  { id: "riven-3", weapon: "Braton Prime", rolls: 14, kuvaSpent: 16800, startedAt: 1_700_000_300_000, endedAt: 1_700_000_900_000, notes: "chasing crit + damage", pinned: true },
  { id: "riven-2", weapon: "Kuva Bramma", rolls: 6, kuvaSpent: 7200, startedAt: 1_700_000_200_000, endedAt: 1_700_000_250_000, notes: null, pinned: false },
  { id: "riven-1", weapon: "Gram Prime", rolls: 3, kuvaSpent: 3600, startedAt: 1_700_000_100_000, endedAt: 1_700_000_150_000, notes: null, pinned: false },
];
const mockRivens = mockRivenSessions();

export type DecodedRiven = Finite<DecodedRivensBinding["unveiled"][number]>;
export type VeiledRivenEntry = DecodedRivensBinding["veiled"][number];
export type DecodedRivens = { unveiled: DecodedRiven[]; veiled: VeiledRivenEntry[]; veiledUnseen: Finite<DecodedRivensBinding["veiledUnseen"]> };

const mockDecodedRivens = (): DecodedRivens => ({
  unveiled: [
    {
      itemId: "riven-a",
      weaponName: "Braton Prime",
      weaponUniqueName: "/Lotus/Weapons/Tenno/Rifle/BratonPrime",
      rivenName: "Braton Prime Critacan",
      masteryReq: 11,
      currentRank: 8,
      maxRank: 8,
      rerolls: 4,
      polarity: "AP_ATTACK",
      disposition: 1.1,
      stats: [
        { tag: "WeaponCritChanceMod", name: "Critical Chance", displayValue: 180.2, maxRankValue: 180.2, rankValues: [], rollFloat: 0.9, grade: "S", positive: true, multiplier: false },
        { tag: "WeaponDamageAmountMod", name: "Damage", displayValue: 150.5, maxRankValue: 150.5, rankValues: [], rollFloat: 0.8, grade: "A", positive: true, multiplier: false },
        { tag: "WeaponFireRateMod", name: "Fire Rate", displayValue: -30.1, maxRankValue: -30.1, rankValues: [], rollFloat: 0.5, grade: "B", positive: false, multiplier: false },
      ],
      overallGrade: "A+",
      attributeGrade: "Great",
      statPerfectness: 0.8,
      rivenType: "Rifle",
    },
    {
      itemId: "riven-b",
      weaponName: "Gram Prime",
      weaponUniqueName: "/Lotus/Weapons/Tenno/Melee/Swords/PrimeGram/PrimeGram",
      rivenName: "Gram Prime Visitox",
      masteryReq: 9,
      currentRank: 3,
      maxRank: 8,
      rerolls: 12,
      polarity: "AP_TACTIC",
      disposition: 0.8,
      stats: [
        { tag: "WeaponMeleeDamageMod", name: "Melee Damage", displayValue: 90.1, maxRankValue: 120.4, rankValues: [], rollFloat: 0.3, grade: "C", positive: true, multiplier: false },
        { tag: "WeaponStunChanceMod", name: "Status Chance", displayValue: 60.2, maxRankValue: 80.2, rankValues: [], rollFloat: 0.4, grade: "C", positive: true, multiplier: false },
      ],
      overallGrade: "C",
      attributeGrade: "Bad",
      statPerfectness: 0.3,
      rivenType: "Melee",
    },
    {
      itemId: "riven-c",
      weaponName: "Soma Prime",
      weaponUniqueName: "/Lotus/Weapons/Tenno/Rifle/SomaPrime",
      rivenName: "Soma Prime Crita-satiata",
      masteryReq: 12,
      currentRank: 5,
      maxRank: 8,
      rerolls: 7,
      polarity: "AP_ATTACK",
      disposition: 0.9,
      stats: [
        { tag: "WeaponCritChanceMod", name: "Critical Chance", displayValue: 142.8, maxRankValue: 160.4, rankValues: [], rollFloat: 0.7, grade: "A", positive: true, multiplier: false },
        { tag: "WeaponElectricityDamageMod", name: "Electricity", displayValue: 88.4, maxRankValue: 99.3, rankValues: [], rollFloat: 0.6, grade: "B", positive: true, multiplier: false },
        { tag: "WeaponRecoilMod", name: "Recoil", displayValue: -42.1, maxRankValue: -47.3, rankValues: [], rollFloat: 0.4, grade: "B", positive: false, multiplier: false },
      ],
      overallGrade: "B+",
      attributeGrade: "Good",
      statPerfectness: 0.62,
      rivenType: "Rifle",
    },
    {
      itemId: "riven-d",
      weaponName: "Acceltra",
      weaponUniqueName: "/Lotus/Weapons/Tenno/Rifle/Acceltra",
      rivenName: "Acceltra Hexaacri",
      masteryReq: 14,
      currentRank: 8,
      maxRank: 8,
      rerolls: 31,
      polarity: "AP_TACTIC",
      disposition: 1.3,
      stats: [
        { tag: "WeaponDamageAmountMod", name: "Damage", displayValue: 112.9, maxRankValue: 112.9, rankValues: [], rollFloat: 0.95, grade: "S", positive: true, multiplier: false },
        { tag: "WeaponCritDamageMod", name: "Critical Damage", displayValue: 74.2, maxRankValue: 74.2, rankValues: [], rollFloat: 0.88, grade: "S", positive: true, multiplier: false },
        { tag: "WeaponFireRateMod", name: "Fire Rate", displayValue: 61.8, maxRankValue: 61.8, rankValues: [], rollFloat: 0.74, grade: "A", positive: true, multiplier: false },
        { tag: "WeaponZoomFovMod", name: "Zoom", displayValue: -38.5, maxRankValue: -38.5, rankValues: [], rollFloat: 0.3, grade: "C", positive: false, multiplier: false },
      ],
      overallGrade: "S",
      attributeGrade: "Great",
      statPerfectness: 0.92,
      rivenType: "Rifle",
    },
  ],
  veiled: [
    { itemType: "/Lotus/Upgrades/Mods/Randomized/LotusRifleRandomModRare", label: "Rifle", challengeType: "KillEnemies", challengeDesc: "Kill 150 enemies with a headshot", challengeGroup: "Kill enemies with a headshot", challengeProgress: 40, challengeRequired: 150 },
    { itemType: "/Lotus/Upgrades/Mods/Randomized/LotusPistolRandomModRare", label: "Pistol", challengeType: null, challengeDesc: null, challengeGroup: null, challengeProgress: null, challengeRequired: null },
  ],
  veiledUnseen: [{ itemType: "RawShotgunRandomMod", label: "Shotgun", count: 2 }],
});

/** The inventory rivens with their decoded stats and grades; outside Tauri a small fixture. */
export const rivensDecoded = async (): Promise<DecodedRivens> =>
  inTauri() ? (unwrap(await commands.rivensDecoded()) as DecodedRivens) : mockDecodedRivens();

export const rivensGet = async (): Promise<RivenRollSession[]> =>
  inTauri() ? unwrap(await commands.rivensGet()) : structuredClone(mockRivens);

/** The note as stored ("" when cleared); null for an unknown id. */
export const rivensSetNote = async (id: string, note: string): Promise<string | null> =>
  inTauri() ? commands.rivensSetNote(id, note) : note.trim();

export const rivensPin = async (id: string, pinned: boolean): Promise<boolean> =>
  inTauri() ? commands.rivensPin(id, pinned) : true;

export const rivensDelete = async (id: string): Promise<boolean> =>
  inTauri() ? commands.rivensDelete(id) : true;

export const rivenBestAttributes = async (weaponName: string, melee: boolean): Promise<RivenBestAttributes | null> =>
  inTauri() ? unwrap(await commands.rivenBestAttributes(weaponName, melee)) : { positives: ["Critical Chance", "Damage"], negatives: ["Recoil"] };

const mockGoodRolls: RivenGoodRollsInfo = { updatedAt: "2026-10-20T12:00:00.000Z", weapons: 420 };

export const rivensGoodRollsInfo = async (): Promise<RivenGoodRollsInfo> =>
  inTauri() ? unwrap(await commands.rivensGoodRollsInfo()) : { ...mockGoodRolls };

/** Re-downloads the good-roll sheet now; an unchanged `updatedAt` means the download failed and the cache stayed. */
export const rivensRefreshGoodRolls = async (): Promise<RivenGoodRollsInfo> =>
  inTauri() ? unwrap(await commands.rivensRefreshGoodRolls()) : { ...mockGoodRolls, updatedAt: new Date().toISOString() };

export const onRivenSessionSaved = async (cb: (s: RivenRollSession) => void): Promise<UnlistenFn> =>
  inTauri() ? listen<RivenRollSession>("riven-session-saved", (e) => cb(e.payload)) : () => {};

// Market alerts. Errors throw; the mock keeps rules in memory and fires nothing.
import type {
  MarketAlertEntry_Serialize as MarketAlertEntry,
  MarketAlertHit_Serialize as MarketAlertHit,
  MarketAlertEngineStatus_Serialize as MarketAlertEngineStatus,
  MarketAlertRule_Serialize as MarketAlertRule,
  MarketAlertSave_Deserialize as MarketAlertSave,
  MarketAlertTestFire,
} from "../bindings";
export type { MarketAlertEntry, MarketAlertHit, MarketAlertEngineStatus, MarketAlertRule, MarketAlertSave, MarketAlertTestFire };

let mockAlerts: MarketAlertEntry[] = [];
let mockAlertSeq = 0;
let mockAlertHits: MarketAlertHit[] = [];
const resetMockAlerts = (): void => {
  mockAlerts = [];
  mockAlertHits = [];
};

export const marketAlertsGet = async (): Promise<MarketAlertEntry[]> =>
  inTauri() ? unwrap(await commands.marketAlertsGet()) : mockAlerts;

export const marketAlertsSet = async (payload: MarketAlertSave): Promise<MarketAlertRule> => {
  if (inTauri()) return unwrap(await commands.marketAlertsSet(payload));
  const rule = { id: `mock-alert-${++mockAlertSeq}`, enabled: true, cooldownMinutes: 30, noCooldown: false, riven: null, item: null, baro: null, ...payload.rule } as MarketAlertRule;
  const entry: MarketAlertEntry = { rule, binding: payload.binding ?? { native: true }, cooldownUntil: null };
  mockAlerts = mockAlerts.some((a) => a.rule.id === rule.id) ? mockAlerts.map((a) => (a.rule.id === rule.id ? entry : a)) : [...mockAlerts, entry];
  return rule;
};

export const marketAlertsDelete = async (id: string): Promise<boolean> => {
  if (inTauri()) return unwrap(await commands.marketAlertsDelete(id));
  mockAlerts = mockAlerts.filter((a) => a.rule.id !== id);
  return true;
};

export const marketAlertsSetEnabled = async (id: string, enabled: boolean): Promise<boolean> => {
  if (inTauri()) return unwrap(await commands.marketAlertsSetEnabled(id, enabled));
  mockAlerts = mockAlerts.map((a) => (a.rule.id === id ? { ...a, rule: { ...a.rule, enabled } } : a));
  return true;
};

export const marketAlertsClearCooldown = async (id: string): Promise<boolean> => {
  if (inTauri()) return unwrap(await commands.marketAlertsClearCooldown(id));
  mockAlerts = mockAlerts.map((a) => (a.rule.id === id ? { ...a, cooldownUntil: null } : a));
  return true;
};

/** Dry run against the live market; never notifies. */
export const marketAlertsTest = async (id: string): Promise<MarketAlertTestFire> =>
  inTauri() ? unwrap(await commands.marketAlertsTest(id)) : { matches: 0, detail: "" };

/** The rules' criteria as shareable JSON; `ids` null exports all. */
export const marketAlertsExport = async (ids: string[] | null = null): Promise<string> =>
  inTauri() ? unwrap(await commands.marketAlertsExport(ids)) : JSON.stringify(mockAlerts.map((a) => a.rule));

/** Adds the rules of an export as disabled; resolves to how many were added. */
export const marketAlertsImport = async (text: string): Promise<number> => {
  if (inTauri()) return unwrap(await commands.marketAlertsImport(text));
  const rules = JSON.parse(text) as MarketAlertRule[];
  mockAlerts = [...mockAlerts, ...rules.map((r) => ({ rule: { ...r, id: `mock-alert-${++mockAlertSeq}`, enabled: false }, binding: { native: true }, cooldownUntil: null }) as MarketAlertEntry)];
  return rules.length;
};

export const marketAlertsHitsGet = async (): Promise<MarketAlertHit[]> =>
  inTauri() ? unwrap(await commands.marketAlertsHitsGet()) : mockAlertHits;

export const marketAlertsHitsClear = async (): Promise<void> => {
  if (inTauri()) unwrap(await commands.marketAlertsHitsClear());
  else mockAlertHits = [];
};

export const marketAlertsStatus = async (): Promise<MarketAlertEngineStatus> =>
  inTauri()
    ? commands.marketAlertsStatus()
    : { running: true, ruleCount: mockAlerts.length, enabledCount: mockAlerts.filter((a) => a.rule.enabled).length, lastTickAt: null, requestsLastHour: 0, lastError: null };

// --- Trade ledger ---
export type LedgerPage = { events: TradeEvent[]; total: number };
export type { LedgerQuery, LedgerPatch };
export type LedgerImportPreview = ImportPreview;
export type LedgerImportResult = ApplyResult;

const mockLedgerEvents = (): TradeEvent[] => {
  const DAY = 86_400_000;
  const base = Date.now();
  const items = [
    ["/Lotus/Types/Recipes/WarframeRecipes/RhinoPrimeBlueprint", "Rhino Prime Blueprint", "rhino_prime_blueprint"],
    ["/Lotus/Types/Items/MiscItems/Forma", "Forma Blueprint", "forma_blueprint"],
    ["/Lotus/Types/Recipes/Weapons/BoltoPrimeBarrel", "Boltor Prime Barrel", "boltor_prime_barrel"],
  ] as const;
  return Array.from({ length: 36 }, (_, i): TradeEvent => {
    const [internalName, displayName, wfmSlug] = items[i % items.length];
    const type = i % 4 === 3 ? "purchase" : "sale";
    return {
      id: `mock-${i}`,
      date: new Date(base - (i * 9 + 1) * DAY).toISOString(),
      type,
      platChange: 10 + ((i * 7) % 40),
      partner: ["Tenno_A", "Tenno_B", "Tenno_C"][i % 3],
      items: [{ internalName, displayName, wfmSlug, count: 1, direction: type === "sale" ? "given" : "received" }],
    };
  });
};
let mockLedger = mockLedgerEvents();

export const ledgerGetRange = async (query: LedgerQuery): Promise<LedgerPage> => {
  if (inTauri()) return unwrap(await commands.ledgerGetRange(query)) as LedgerPage;
  const day = (e: TradeEvent) => localDayKey(new Date(e.date));
  const text = query.text?.toLowerCase();
  const hits = mockLedger
    .filter((e) => (!query.from || day(e) >= query.from) && (!query.to || day(e) <= query.to) && (!query.type || e.type === query.type))
    .filter((e) => !text || `${e.partner ?? ""} ${e.items.map((i) => i.displayName).join(" ")}`.toLowerCase().includes(text))
    .sort((a, b) => (a.date < b.date ? 1 : -1));
  const offset = query.offset ?? 0;
  return { events: hits.slice(offset, offset + Math.min(query.limit ?? 200, 200)), total: hits.length };
};

export type LedgerBatch = LedgerBatchBinding;
let mockBatches: LedgerBatch[] = [];

/** Imported batches still in the ledger, newest first. */
export const ledgerListBatches = async (): Promise<LedgerBatch[]> =>
  inTauri() ? unwrap(await commands.ledgerListBatches()) : mockBatches;

/** Removes every row a GDPR import batch added; resolves to how many. */
export const ledgerUndoBatch = async (batchId: string): Promise<number> => {
  if (inTauri()) return unwrap(await commands.ledgerUndoBatch(batchId));
  mockBatches = mockBatches.filter((b) => b.batchId !== batchId);
  return 0;
};

export const ledgerEdit = async (id: string, patch: LedgerPatch): Promise<void> => {
  if (inTauri()) return void unwrap(await commands.ledgerEdit(id, patch));
  mockLedger = mockLedger.map((e) =>
    e.id === id
      ? { ...e, platChange: patch.platChange ?? e.platChange, partner: patch.partner ?? e.partner, type: patch.type ?? e.type }
      : e,
  );
};

export const gdprPreviewFile = async (path: string): Promise<LedgerImportPreview> =>
  inTauri()
    ? (unwrap(await commands.gdprPreviewFile(path)) as LedgerImportPreview)
    : { batchId: "mock-batch", fileName: path, counts: { parsed: 3, duplicates: 0, unresolved: 0, rejected: 0 }, rows: [] };

export const gdprApply = async (batchId: string): Promise<LedgerImportResult> => {
  if (inTauri()) return unwrap(await commands.gdprApply(batchId));
  mockBatches = [{ batchId, source: "gdpr", rows: 3, from: "2024-03-04T12:00:00Z", to: "2024-03-09T12:00:00Z" }, ...mockBatches];
  return { applied: 3, skippedDuplicates: 0 };
};

// ---- trade workbench: the engine lives in Rust; outside Tauri a mock plan executes instantly ----
export type {
  WorkbenchExecuteResult_Serialize as WorkbenchExecuteResult,
  WorkbenchPlanValidation_Serialize as WorkbenchPlanValidation,
  WorkbenchReviewReport_Serialize as WorkbenchReviewReport,
  WorkbenchReviewClassification,
  WorkbenchRowProgress_Serialize as WorkbenchRowProgress,
  WorkbenchRowStatus,
  WorkbenchStopReason,
} from "../bindings";
import type { WorkbenchTrade, WorkbenchTradeMatch_Serialize, WorkbenchExecuteResult_Serialize as WbExecute, WorkbenchPlanValidation_Serialize as WbValidation, WorkbenchReviewReport_Serialize as WbReport, WorkbenchReviewResolution, WorkbenchState_Serialize as WbState } from "../bindings";
import type { WorkbenchOverrideAck, WorkbenchPlan, WorkbenchSafetySnapshot } from "../../config/shared/tradeWorkbenchTypes";

const IDLE_WORKBENCH: WbState = { phase: "idle", reviewRequired: false, journalError: null, unsettledCount: 0, run: null };
let mockWorkbench: WbState = IDLE_WORKBENCH;

export const workbenchGetState = async (): Promise<WbState> => (inTauri() ? commands.workbenchGetState() : mockWorkbench);

export const onWorkbenchUpdated = async (cb: (s: WbState) => void): Promise<UnlistenFn> =>
  inTauri() ? listen<WbState>("workbench-updated", (e) => cb(e.payload)) : () => {};

export const workbenchPreviewPlan = async (plan: WorkbenchPlan, safety: WorkbenchSafetySnapshot): Promise<WbValidation> => {
  if (inTauri()) return unwrap(await commands.workbenchPreviewPlan(plan, safety));
  const rows = plan.rows.map((r) => ({ rowId: r.rowId, ok: true, reason: null }));
  return { ok: rows.length > 0, planError: rows.length ? null : "empty", rows, totalUnits: plan.rows.reduce((s, r) => s + r.quantity, 0), totalPlatinum: plan.rows.reduce((s, r) => s + r.quantity * r.platinum, 0) };
};

export const workbenchExecutePlan = async (plan: WorkbenchPlan, safety: WorkbenchSafetySnapshot): Promise<WbExecute> => {
  if (inTauri()) return commands.workbenchExecutePlan(plan, safety);
  const now = Date.now();
  mockWorkbench = { ...IDLE_WORKBENCH, run: { planId: plan.planId, startedAt: now, finishedAt: now, stopReason: "completed", rows: plan.rows.map((r) => ({ rowId: r.rowId, itemName: r.itemName, status: "done", orderId: null, error: null })) } };
  return { started: true, error: null, validation: null, state: mockWorkbench };
};

export type WorkbenchTradeMatch = WorkbenchTradeMatch_Serialize;

/** Read-only: which of the user's warframe.market listings a finished trade would close (empty when signed out). */
export const workbenchMatchTrade = async (trade: WorkbenchTrade): Promise<WorkbenchTradeMatch[]> =>
  inTauri() ? unwrap(await commands.workbenchMatchTrade(trade)) : [];

/** Closes one matched listing; false when it was already gone or the session ended. With the ledger row's id, the row is flagged `wfmClosed`. */
export const workbenchCloseMatch = async (match: WorkbenchTradeMatch, eventId?: string): Promise<boolean> => {
  if (inTauri()) return unwrap(await commands.workbenchCloseMatch(match, eventId ?? null));
  mockLedger = mockLedger.map((e) => (e.id === eventId ? { ...e, wfmClosed: true } : e));
  return true;
};

export const workbenchCancelRun = async (): Promise<WbState> => (inTauri() ? commands.workbenchCancelRun() : mockWorkbench);
export const workbenchAckOverride = async (ack: WorkbenchOverrideAck): Promise<WbState | null> =>
  inTauri() ? unwrap(await commands.workbenchAckOverride(ack)) : mockWorkbench;
export const workbenchReconcile = async (): Promise<WbReport> =>
  inTauri() ? unwrap(await commands.workbenchReconcile()) : { generatedAt: Date.now(), rows: [], fetchError: null };
export const workbenchResolveReview = async (resolutions: WorkbenchReviewResolution[], resetCorruptJournal = false): Promise<WbState> => {
  if (inTauri()) return unwrap(await commands.workbenchResolveReview({ resolutions, resetCorruptJournal }));
  return (mockWorkbench = IDLE_WORKBENCH);
};

// ---- Daily stats ----
import type { DailyStatEntry_Serialize, SessionStats_Serialize, StatsImportEntry_Deserialize, StatsImportResult } from "../bindings";
export type DailyStatEntry = Finite<DailyStatEntry_Serialize>;
export type SessionStats = Finite<SessionStats_Serialize>;
export type StatsImportEntry = StatsImportEntry_Deserialize;

const dayKey = (ago: number) => new Date(Date.now() - ago * 86_400_000).toLocaleDateString("sv");
const mockStatsDays = (): DailyStatEntry[] =>
  [3, 2, 1].map((ago, i) => ({
    date: dayKey(ago),
    relicsOpened: 4 + i * 3,
    daysPlayed: 1,
    dailyTrades: 2 + i,
    resources: {
      plat: { delta: 40 + i * 25, abs: 300 + i * 40 },
      credits: { delta: 850_000 + i * 120_000, abs: 4_200_000 },
      endo: { delta: -12_000 + i * 9_000, abs: 61_500 },
      ducats: { delta: 90 * (i + 1), abs: 1_240 },
      kuva: { delta: 6_000 + i * 2_000, abs: 31_000 },
    },
  }));
let mockStatsReset = false;

export const statsGetSession = async (): Promise<SessionStats> => {
  if (inTauri()) return (await commands.statsGetSession()) as SessionStats;
  const today = mockStatsReset ? null : mockStatsDays()[2];
  return {
    resources: Object.fromEntries(Object.entries(today?.resources ?? {}).map(([id, d]) => [id, { delta: d.delta, current: d.abs ?? null }])),
    relicsOpened: today?.relicsOpened ?? 0,
    dailyTrades: today?.dailyTrades ?? 0,
    hasData: today !== null,
  };
};

export const statsGetDaily = async (): Promise<DailyStatEntry[]> =>
  inTauri() ? unwrap(await commands.statsGetDaily()) as DailyStatEntry[] : mockStatsReset ? [] : mockStatsDays();

export const statsImport = async (entries: StatsImportEntry[]): Promise<StatsImportResult> =>
  inTauri() ? commands.statsImport(entries) : { ok: true, count: entries.length };

export const statsReset = async (): Promise<void> => {
  if (inTauri()) unwrap(await commands.statsReset());
  else mockStatsReset = true;
};

// --- Stats view ---
/** Adds each trade to the ledger; ids already there are skipped by the backend, so re-importing is safe. */
export const ledgerAddTrades = async (events: TradeEvent[]): Promise<void> => {
  // shortcut: one invoke per row (<= 10k), a batch command when imports get slow.
  if (inTauri()) for (const event of events) unwrap(await commands.ledgerAdd(event as Parameters<typeof commands.ledgerAdd>[0]));
  else mockLedger = [...events.filter((e) => !mockLedger.some((m) => m.id === e.id)), ...mockLedger];
};

// --- Mission rewards ---
import type {
  MissionRewardsPayload_Serialize,
  MissionRewardsPage_Serialize,
  MissionRewardsStatus_Serialize,
} from "../bindings";

export type MissionRewardSummary = Finite<MissionRewardsPayload_Serialize["summaries"][number]>;
export type MissionRewardsStatus = Finite<MissionRewardsStatus_Serialize>;
export type MissionRewardsPayload = { summaries: MissionRewardSummary[]; status: MissionRewardsStatus };
export type MissionRewardsPage = Omit<Finite<MissionRewardsPage_Serialize>, "summaries" | "latest" | "status"> & {
  summaries: MissionRewardSummary[];
  latest: MissionRewardSummary | null;
  status: MissionRewardsStatus;
};
export interface MissionRewardsQuery {
  offset: number;
  limit: number;
  since?: number;
  todaySince?: number;
  missionType?: string;
  /** A mission matches when it received any of these. */
  uniqueNames?: string[];
}

const mockMission = (n: number, minutesAgo: number, missionType: string | null, node: string | null, items: [string, number][], credits: number, endo: number, missionCount = 1): MissionRewardSummary => ({
  id: `mock-${n}`,
  endedAt: Date.now() - minutesAgo * 60_000,
  readAt: Date.now() - minutesAgo * 60_000 + 3_000,
  missionCount,
  missionType,
  node,
  items: items.map(([uniqueName, count]) => ({ uniqueName, count })),
  credits,
  endo,
  baselineAt: Date.now() - (minutesAgo + 12) * 60_000,
});
const FERRITE = "/Lotus/Types/Items/MiscItems/Ferrite";
const PLASTIDS = "/Lotus/Types/Items/MiscItems/Plastids";
const CELL = "/Lotus/Types/Items/MiscItems/OrokinCell";
const NEURODES = "/Lotus/Types/Items/MiscItems/Neurodes";
const FORMA = "/Lotus/Types/Items/MiscItems/Forma";
// Oldest first, like the store; the mock answers the same queries the backend does.
const MOCK_MISSIONS: MissionRewardSummary[] = [
  mockMission(1, 60 * 30, "MT_SURVIVAL", "SolNode25", [[FERRITE, 1400], [PLASTIDS, 220]], 12_000, 300),
  mockMission(2, 60 * 26, "MT_DEFENSE", "SolNode305", [[CELL, 2], [NEURODES, 5]], 8_000, 120),
  mockMission(3, 60 * 3, "MT_TERRITORY", "SolNode18", [[FERRITE, 900], [FORMA, 1]], 20_000, 640),
  mockMission(4, 90, null, null, [], 0, 0, 3),
  mockMission(5, 12, "MT_SURVIVAL", "SolNode25", [[PLASTIDS, 310], [CELL, 1]], 15_500, 410),
];
let mockMissionTracking = true;
const mockMissionStatus = (): MissionRewardsStatus => ({
  phase: "idle",
  blocked: mockMissionTracking ? null : "tracking-off",
  lastFailure: null,
  pendingMissions: 0,
});

const mockMissionPage = (q: MissionRewardsQuery): MissionRewardsPage => {
  const all = [...MOCK_MISSIONS].reverse();
  const hit = all.filter(
    (m) =>
      (q.since === undefined || m.endedAt >= q.since) &&
      (q.missionType === undefined || m.missionType === q.missionType) &&
      (q.uniqueNames === undefined || m.items.some((i) => q.uniqueNames!.includes(i.uniqueName))),
  );
  const counts = new Map<string, number>();
  for (const m of hit) for (const i of m.items) counts.set(i.uniqueName, (counts.get(i.uniqueName) ?? 0) + i.count);
  return {
    summaries: hit.slice(q.offset, q.offset + q.limit),
    matched: hit.length,
    totals: {
      missions: hit.reduce((s, m) => s + m.missionCount, 0),
      credits: hit.reduce((s, m) => s + m.credits, 0),
      endo: hit.reduce((s, m) => s + m.endo, 0),
      items: [...counts].map(([uniqueName, count]) => ({ uniqueName, count })).sort((a, b) => b.count - a.count),
    },
    latest: all[0] ?? null,
    recorded: all.length,
    today: q.todaySince === undefined ? 0 : all.filter((m) => m.endedAt >= q.todaySince!).reduce((s, m) => s + m.missionCount, 0),
    missionTypes: [...new Set(all.flatMap((m) => (m.missionType ? [m.missionType] : [])))].sort(),
    itemTypes: [...new Set(all.flatMap((m) => m.items.map((i) => i.uniqueName)))].sort(),
    status: mockMissionStatus(),
  };
};

/** One page of recorded missions, newest first; throws when the backend rejects the query. */
export const missionRewardsPage = async (query: MissionRewardsQuery): Promise<MissionRewardsPage> =>
  inTauri() ? (unwrap(await commands.missionRewardsPage(query)) as MissionRewardsPage) : mockMissionPage(query);

export const missionRewardsSetTracking = async (enabled: boolean): Promise<MissionRewardsStatus> => {
  if (inTauri()) return unwrap(await commands.missionRewardsSetTracking(enabled)) as MissionRewardsStatus;
  mockMissionTracking = enabled;
  return mockMissionStatus();
};

/** Fires when a mission is recorded or the tracker's status changes. */
export const onMissionReward = async (cb: (p: MissionRewardsPayload) => void): Promise<UnlistenFn> =>
  inTauri() ? listen<MissionRewardsPayload>("mission-reward", (e) => cb(e.payload)) : () => {};

// Overlay layouts: JSON text per key, persisted in the Rust settings store.
const mockLayouts: Record<string, string> = {};
export const overlayLayoutGet = async (key: string): Promise<string | null> =>
  inTauri() ? commands.overlayLayoutGet(key) : (mockLayouts[key] ?? null);
export const overlayLayoutSave = async (key: string, json: string): Promise<void> => {
  if (inTauri()) return void unwrap(await commands.overlayLayoutSave(key, json));
  mockLayouts[key] = json;
};
export const overlayLayoutReset = async (key: string): Promise<void> => {
  if (inTauri()) return void unwrap(await commands.overlayLayoutReset(key));
  delete mockLayouts[key];
};

/** Next key combo pressed anywhere, as an accelerator string; rejects on Esc / timeout. */
export const hotkeyCapture = async (): Promise<string> =>
  inTauri() ? unwrap(await commands.hotkeyCapture()) : "Ctrl+Shift+F9";

export type { MissionRewardItem } from "../bindings";

/** Opens a view or one layout section in its own window (no-op in the browser mock). */
export const popoutOpen = async (target: PopoutTarget): Promise<void> => {
  if (inTauri()) unwrap(await commands.popoutOpen(target));
};
import type { PopoutTarget } from "../bindings";
export type { PopoutTarget };

// --- Codex scans (player profile). Refreshed by the backend every few hours and on demand; `error` is no-account | fetch-failed | no-data.
export interface CodexScansView {
  fetchedAt: number | null;
  scans: CodexScanEntry[] | null;
  error: "no-account" | "fetch-failed" | "no-data" | null;
  nextRefreshAt: number;
}
const MOCK_CODEX_SCANS: CodexScanEntry[] = [
  { type: "/Lotus/Types/Enemies/CaptureTargets/CaptureTargetGhoulAgent", count: 3 },
  { type: "/Lotus/Types/Enemies/CaptureTargets/CaptureTargetGrineerGhoulShip", count: 2 },
  { type: "/Lotus/Types/Enemies/Grineer/AIWeek/Avatars/NightwatchHeavyGunnerAvatar", count: 7 },
  { type: "/Lotus/Types/Enemies/Corpus/AladVPet/AladVPet", count: 1 },
  { type: "/Lotus/Types/Enemies/Infested/AiWeek/Ancients/AncientAvatar", count: 5 },
  { type: "/Lotus/Types/Enemies/Acolytes/AreaCasterAcolyteAvatar", count: 3 },
];
export const getCodexScans = async (refresh = false): Promise<CodexScansView> => {
  if (!inTauri()) return { fetchedAt: Date.now() - 3_600_000, scans: MOCK_CODEX_SCANS, error: null, nextRefreshAt: refresh ? Date.now() + 60_000 : 0 };
  const r = unwrap(await commands.getCodexScans(refresh));
  return { fetchedAt: r.fetchedAt, scans: r.scans, error: (r.error as CodexScansView["error"]) ?? null, nextRefreshAt: r.nextRefreshAt ?? 0 };
};
import type { CodexScanEntry, RivenRollTally } from "../bindings";

// Riven overlay roll session and fits-in label (parity T25). The tally also rides on the overlay view.
export type { RivenRollTally };
const mockTally: { t: RivenRollTally | null } = { t: null };
export const rivenSessionStart = async (kuvaPerRoll: number): Promise<RivenRollTally> =>
  inTauri() ? unwrap(await commands.rivenSessionStart(kuvaPerRoll)) : (mockTally.t = { kuvaPerRoll, rollCount: 0, totalKuvaSpent: 0 });
export const rivenSessionRollDone = async (): Promise<RivenRollTally> => {
  if (inTauri()) return unwrap(await commands.rivenSessionRollDone());
  if (!mockTally.t) throw new Error("no riven session running");
  const { kuvaPerRoll, rollCount, totalKuvaSpent } = mockTally.t;
  return (mockTally.t = { kuvaPerRoll, rollCount: rollCount + 1, totalKuvaSpent: (totalKuvaSpent ?? 0) + kuvaPerRoll });
};
export const rivenSessionEnd = async (): Promise<void> => {
  if (inTauri()) unwrap(await commands.rivenSessionEnd());
  else mockTally.t = null;
};
/** OCR lines of the fits-in caption; resolves to the weapon the card switched to, or null when it kept its own. */
export const rivenOverlayApplyLabel = async (lines: string[]): Promise<string | null> =>
  inTauri() ? unwrap(await commands.rivenOverlayApplyLabel(lines)) : null;


import type { MonitorInfo, OverlayRect, PlacementEntry } from "../bindings";

// Setup wizard / tour state (parity T21, T23): persisted by the backend, not localStorage.
export type OnboardingState = { setupDone: boolean; tourSeen: boolean };
let mockOnboarding: OnboardingState = { setupDone: false, tourSeen: false };
const onboarding = (s: { setup_done: boolean; tour_seen: boolean }): OnboardingState => ({ setupDone: s.setup_done, tourSeen: s.tour_seen });
export const getOnboarding = async (): Promise<OnboardingState> => (inTauri() ? onboarding(unwrap(await commands.onboardingGet())) : { ...mockOnboarding });
export const setSetupDone = async (done: boolean): Promise<OnboardingState> => {
  if (inTauri()) return onboarding(unwrap(await commands.onboardingSetSetupDone(done)));
  return (mockOnboarding = { ...mockOnboarding, setupDone: done });
};
/** `false` restarts the tour. */
export const setTourSeen = async (seen: boolean): Promise<OnboardingState> => {
  if (inTauri()) return onboarding(unwrap(await commands.onboardingSetTourSeen(seen)));
  return (mockOnboarding = { ...mockOnboarding, tourSeen: seen });
};

// Overlay placement per game resolution (parity T21): fractions of the monitor.
const MOCK_MONITOR: MonitorInfo = { name: "Mock display", x: 0, y: 0, width: 1920, height: 1080, scale_factor: 1, primary: true };
const mockPlacement = new Map<string, PlacementEntry[]>();
const placementKey = (w: number, h: number) => `${w}x${h}`;
export const overlayMonitors = async (): Promise<MonitorInfo[]> => (inTauri() ? unwrap(await commands.overlayMonitors()) : [MOCK_MONITOR]);
/** The monitor the overlays are drawn on now (game's, else primary); null when unknown. */
export const overlayPlacementDetect = async (): Promise<OverlayRect | null> =>
  inTauri() ? unwrap(await commands.overlayPlacementDetect()) : { x: 0, y: 0, w: MOCK_MONITOR.width, h: MOCK_MONITOR.height };
export const overlayPlacementGet = async (width: number, height: number): Promise<PlacementEntry[]> =>
  inTauri() ? unwrap(await commands.overlayPlacementGet(width, height)) : [...(mockPlacement.get(placementKey(width, height)) ?? [])];
const mockPlacementEdit = (w: number, h: number, kind: PlacementEntry["kind"], f: (p: PlacementEntry["placement"]) => void) => {
  const list = mockPlacement.get(placementKey(w, h)) ?? [];
  let e = list.find((x) => x.kind === kind);
  if (!e) list.push((e = { kind, placement: { x_frac: 0, y_frac: 0, scale: 1 } }));
  f(e.placement);
  mockPlacement.set(placementKey(w, h), list);
};
export const overlayPlacementSave = async (width: number, height: number, kind: PlacementEntry["kind"], xFrac: number, yFrac: number): Promise<void> => {
  if (inTauri()) return void unwrap(await commands.overlayPlacementSave(width, height, kind, xFrac, yFrac));
  mockPlacementEdit(width, height, kind, (p) => ((p.x_frac = xFrac), (p.y_frac = yFrac)));
};
export const overlayPlacementSaveScale = async (width: number, height: number, kind: PlacementEntry["kind"], scale: number): Promise<void> => {
  if (inTauri()) return void unwrap(await commands.overlayPlacementSaveScale(width, height, kind, scale));
  mockPlacementEdit(width, height, kind, (p) => (p.scale = scale));
};
export const overlayPlacementReset = async (width: number, height: number): Promise<void> => {
  if (inTauri()) return void unwrap(await commands.overlayPlacementReset(width, height));
  mockPlacement.delete(placementKey(width, height));
};

// --- Personal stats (T17): profile career/appearance (manual refresh) and saved loadouts from the inventory.
import type { PersonalProfileResult, SavedLoadout } from "../bindings";
export type { PersonalProfile, PersonalProfileResult, SavedLoadout } from "../bindings";
const MOCK_PERSONAL_PROFILE: PersonalProfileResult = {
  profile: {
    displayName: "Tenno",
    masteryRank: 30,
    registeredAt: 1_500_000_000_000,
    career: { TimePlayedSec: 7_200_000, MissionsCompleted: 5400, Deaths: 310, MeleeKills: 80_000 },
    equipment: [
      { type: "/Lotus/Weapons/Tenno/Rifle/BoltorPrime", name: null, stats: { kills: 90_000, headshots: 41_000, equipTime: 400_000 } },
    ],
    enemies: [{ type: "/Lotus/Types/Enemies/Grineer/GrineerLancer", name: null, stats: { kills: 120_000, scans: 3 } }],
    abilities: [{ type: "/Lotus/Powersuits/Excalibur/SlashDash", name: null, stats: { used: 5000 } }],
    missions: [{ type: "SolNode1", name: null, stats: { highScore: 20_000 } }],
    appearance: [
      { category: "LongGuns", type: "/Lotus/Weapons/Tenno/Rifle/BratonPrime", activeConfig: 0, hiddenWhenHolstered: null, configs: [] },
      { category: "Pistols", type: "/Lotus/Weapons/Tenno/Pistols/PrimeLex/PrimeLex", activeConfig: 0, hiddenWhenHolstered: null, configs: [] },
      { category: "LongGuns", type: "/Lotus/Weapons/Tenno/Rifle/Acceltra", activeConfig: null, hiddenWhenHolstered: null, configs: [] },
    ],
  },
  fetchedAt: Date.now() - 3_600_000,
  status: "ready",
  nextRefreshAt: 0,
};
export const getPersonalProfile = async (refresh = false): Promise<PersonalProfileResult> => {
  if (!inTauri()) {
    if (mockParam("noinv") !== null && mockParam("profile") === null) return { profile: null, fetchedAt: null, status: "no-account", nextRefreshAt: 0 };
    return { ...MOCK_PERSONAL_PROFILE, nextRefreshAt: refresh ? Date.now() + 60_000 : 0 };
  }
  return unwrap(await commands.getPersonalProfile(refresh));
};
const MOCK_LOADOUTS: SavedLoadout[] = [
  {
    name: "Fixture loadout",
    slots: [
      {
        category: "Suits",
        item: {
          category: "Suits",
          type: "/Lotus/Powersuits/Excalibur/Excalibur",
          activeConfig: 0,
          hiddenWhenHolstered: null,
          configs: [{ name: "Look", skins: [], colors: {} }],
          activeModConfig: 0,
          modConfigs: [{ name: "Build", upgrades: [{ slot: 0, type: "/Lotus/Upgrades/Mods/Warframe/AvatarHealthMaxMod", rank: 10 }] }],
        },
      },
    ],
  },
];
export const getPersonalLoadouts = async (): Promise<SavedLoadout[]> => (inTauri() ? commands.getPersonalLoadouts() : MOCK_LOADOUTS);

/** Move mode: every enabled overlay shows sample content and can be dragged; `false` locks them. */
export const overlayMoveMode = async (on: boolean): Promise<void> => {
  if (inTauri()) unwrap(await commands.overlayMoveMode(on));
};
