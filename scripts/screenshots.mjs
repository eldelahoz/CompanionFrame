// README screenshots: node scripts/screenshots.mjs [outDir]   -> docs/img/*.png
// Runs the app on the in-browser mocks (plain Vite, no Tauri) in Edge at 1440x900, dark theme, English.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";
import { createServer } from "vite";

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "..");
const out = process.argv[2] ?? path.join(root, "docs", "img");
fs.mkdirSync(out, { recursive: true });

// Stands in for window.__TAURI_INTERNALS__: events are pushed by hand, every command answers "nothing".
function installBridge() {
  const cbs = new Map();
  const listeners = new Map();
  let next = 1;
  const settings = { language: "en", game_language: "auto", theme: "dark", overlay_layouts: {}, overlay_enabled: {}, overlay_opacity: {}, theme_settings: "", ui_scale: 1, data_source_mode: "passive" };
  const status = { path: null, found: true, source: "manual", modifiedAt: null, lastError: null, importedAt: null, tradesApplied: 0, picksApplied: 0, gameSyncedSinceImport: true };
  const answers = { get_settings: settings, get_inventory_status: status, overlay_layout_get: null, overlay_placement_get: [] };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  window.__TAURI_INTERNALS__ = {
    transformCallback: (cb) => (cbs.set(next, cb), next++),
    unregisterCallback: (id) => cbs.delete(id),
    convertFileSrc: (p) => p,
    async invoke(cmd, args) {
      if (cmd === "plugin:event|listen") {
        listeners.set(args.event, [...(listeners.get(args.event) ?? []), args.handler]);
        return args.handler;
      }
      return cmd in answers ? structuredClone(answers[cmd]) : null;
    },
  };
  window.__emit = (event, payload) => (listeners.get(event) ?? []).forEach((id) => cbs.get(id)?.({ event, id, payload }));
}

// Fictional round: four Prime parts with made-up prices.
const rewardView = () => {
  const art = (file) => [{ name: "", imageUrl: `https://cdn.warframestat.us/img/${file}.png`, ownedCount: 0, requiredCount: 1, isReward: true, building: false }];
  const part = (name, rarity, ducats, price, owned, setOwned, file) => ({
    item: { name, rarity, ducats, partOwnedCount: owned, partRequiredCount: 1, mastered: null, vaulted: false, building: false, setOwnedCount: setOwned, setRequiredCount: 4, setUrlName: "", setParts: art(file) },
    price,
    setPrice: null,
  });
  return {
    phase: "ready",
    presentation: {
      count: 4,
      slots: [
        part("Rhino Prime Chassis Blueprint", "Common", 65, 12, 1, 2, "GenericWarframePrimeChassis"),
        part("Rhino Prime Neuroptics Blueprint", "Uncommon", 45, 20, 0, 2, "GenericWarframePrimeHelmet"),
        part("Boltor Prime Receiver", "Common", 15, 5, 2, 1, "BoltorPrime"),
        part("Ash Prime Systems Blueprint", "Rare", 100, 35, 0, 0, "AshPrime"),
      ],
    },
    failure: null,
    autoCloseMs: 600000,
    token: 1,
  };
};

const server = await createServer({ root, configFile: path.join(root, "vite.config.ts"), server: { port: 14330, strictPort: false, hmr: false }, logLevel: "error" });
await server.listen();
const base = server.resolvedUrls.local[0];
const browser = await chromium.launch({ channel: "msedge" });
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1, colorScheme: "dark", locale: "en-US" });
  page.on("pageerror", (e) => console.log(`page error: ${e.message}`));
  await page.addInitScript(() => localStorage.setItem("cf.tour.done", "1"));
  await page.goto(base);
  await page.waitForSelector("[data-view]");
  // Remote icons: wait until every <img> settled; broken ones are hidden so no broken-image glyph shows.
  const settle = async () => {
    await page.waitForTimeout(800);
    await page.evaluate(async () => {
      const imgs = [...document.images];
      await Promise.all(imgs.map((i) => (i.complete ? 0 : new Promise((r) => { i.onload = i.onerror = r; setTimeout(r, 8000); }))));
      for (const i of imgs) if (i.complete && i.naturalWidth === 0) i.style.visibility = "hidden";
    });
    await page.waitForTimeout(300);
  };
  const shot = async (name) => {
    await settle();
    await page.screenshot({ path: path.join(out, `${name}.png`) });
    console.log(name);
  };
  const go = async (view) => {
    await page.click(`[data-view="${view}"]`);
    await page.waitForTimeout(500);
  };
  // Dashboard, then Inventory (filters + value strip over the priced mock inventory).
  await go("dashboard");
  await shot("dashboard");
  await go("inventory");
  await page.waitForTimeout(2500); // prices land after the catalog
  await shot("inventory");
  // Item detail modal with the component list.
  await page.getByText("Rhino Prime", { exact: true }).first().click();
  await page.waitForSelector("text=Components");
  await shot("item-detail");
  await page.keyboard.press("Escape");
  // Market: mock sign-in (fictional account), then the Browse tab on an item.
  await go("market");
  await page.getByText("Use email and password instead").click();
  await page.locator('input[type="email"]').fill("demo@example.com");
  await page.locator('input[type="password"]').fill("demo");
  await page.keyboard.press("Enter");
  await page.getByRole("tab", { name: "Browse" }).click();
  await page.getByPlaceholder(/Search any tradable item/).fill("Serration");
  await page.getByText("Serration", { exact: true }).first().click();
  await page.waitForTimeout(1500);
  await shot("market");
  await go("mastery");
  await shot("mastery");

  // Relic reward overlay: the real overlay page on a stub Tauri bridge, over a neutral backdrop (no game art).
  const overlay = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1, colorScheme: "dark", locale: "en-US" });
  await overlay.addInitScript(installBridge);
  overlay.on("pageerror", (e) => console.log(`overlay error: ${e.message}`));
  await overlay.goto(`${base}overlay.html?kind=reward`);
  await overlay.addStyleTag({ content: "html,body{background:radial-gradient(120% 90% at 50% 20%,#1d2733,#0a0d12) !important}" });
  await overlay.waitForSelector("[data-overlay]", { state: "attached" });
  await overlay.evaluate((view) => window.__emit("reward-overlay", view), rewardView());
  await overlay.waitForTimeout(1500);
  await overlay.screenshot({ path: path.join(out, "reward-overlay.png"), clip: { x: 290, y: 0, width: 860, height: 230 } });
  console.log("reward-overlay");
} finally {
  await browser.close();
  await server.close();
}
