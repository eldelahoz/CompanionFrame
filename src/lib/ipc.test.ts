import { beforeEach, expect, test } from "vitest";
import { appInfo, getPersonalLoadouts, getPersonalProfile, getSettings, resetMock, setSettings, wfmContractsMy, wfmPrice, wfmPricesBulk, wfmRivenAuctionClose, wfmRivenSearch, wfmSendPlusRep, wfmStatsHistory } from "./ipc";

beforeEach(resetMock);

test("mock settings roundtrip outside tauri", async () => {
  const s = await getSettings();
  expect(s).toEqual({ language: "en", game_language: "auto", theme: "dark", reward_scan_hotkey: "F8", overlay_move_hotkey: "F7", riven_rescan_hotkey: "F6", eelog_path: null, overlay_layouts: {}, overlay_enabled: {}, overlay_opacity: {}, message_notifications_enabled: true, message_notifications_while_focused: false, riven_similar_auctions_shown: true, theme_settings: "", ui_scale: 1, keep_running_on_close: false, data_source_mode: "passive", memory_consent_at: null, auto_inventory_sync: true, save_scan_captures: false, auto_update_check: true, auto_close_trade_listings: false, trade_thanks_enabled: false, trade_thanks_message: "Thank you for your purchase. I appreciate the +rep <3" });
  await setSettings({ ...s, language: "de", theme: "light" });
  expect(await getSettings()).toMatchObject({ language: "de", theme: "light" });
  // Like the backend gate: memory mode only with a consent timestamp.
  await expect(setSettings({ ...s, data_source_mode: "memory" })).rejects.toThrow();
});

test("mock app info", async () => {
  expect((await appInfo()).name).toBe("CompanionFrame");
});

test("mock prices: a known slug is ok, an unknown one is no_data, bulk dedupes", async () => {
  expect(await wfmPrice("serration")).toEqual({ slug: "serration", status: "ok", price: 22, maxPrice: 48, maxRank: 10, stale: false, updatedAt: null });
  expect(await wfmPrice("nope")).toMatchObject({ status: "no_data", price: null });
  expect(await wfmPricesBulk(["serration", "serration", "nope"])).toHaveLength(2);
  expect(await wfmStatsHistory("serration")).toHaveLength(90);
});

test("riven contracts mock: seeded contracts that close, no listings, +rep accepted", async () => {
  const page = await wfmContractsMy(2);
  expect(page).toMatchObject({ page: 2, hasMore: false, unavailable: null });
  expect(page.contracts.map((c) => c.id)).toEqual(["a1", "a2"]);
  await wfmRivenAuctionClose("a1");
  expect((await wfmContractsMy()).contracts.map((c) => c.id)).toEqual(["a2"]);
  expect(await wfmRivenSearch("rubico")).toEqual([]);
  expect(await wfmSendPlusRep("Someone")).toBe("sent");
});

test("answers the riven good-roll sheet info and refresh outside Tauri", async () => {
  const { rivensGoodRollsInfo, rivensRefreshGoodRolls } = await import("./ipc");
  const before = await rivensGoodRollsInfo();
  const after = await rivensRefreshGoodRolls();
  expect(after.weapons).toBe(before.weapons);
  expect(after.updatedAt! > before.updatedAt!).toBe(true);
});

test("personal stats mocks outside tauri", async () => {
  const p = await getPersonalProfile(true);
  expect(p.status).toBe("ready");
  expect(p.profile?.career.MissionsCompleted).toBe(5400);
  expect(p.nextRefreshAt).toBeGreaterThan(Date.now());
  expect((await getPersonalLoadouts())[0].slots[0].item?.category).toBe("Suits");
});
