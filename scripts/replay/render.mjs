// Draws the overlay view JSON of `cargo run --example replay` over its screenshot:
//   node scripts/replay/render.mjs [dir]   -> src-tauri/target/replay/<name>.overlay.png
// The real overlay page runs in a 1920x1080 Edge window on a mocked Tauri bridge: no settings, no
// inventory (so the "No inventory" badges show), the default placement. Screenshots stay in their folder.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";
import { createServer } from "vite";

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const out = path.join(root, "src-tauri", "target", "replay");
const shots = process.argv[2] ?? process.env.CF_SCREENSHOTS;
if (!shots) {
  console.error("replay: pass the screenshots folder as an argument or set CF_SCREENSHOTS");
  process.exit(2);
}
const KIND = { "riven-overlay": "riven", "reward-overlay": "reward", "relic-planner": "reward" };

if (!fs.existsSync(out) || !fs.existsSync(shots)) {
  console.log("replay: no output or no screenshots folder, skipping");
  process.exit(0);
}
const relicDb = fs.existsSync(path.join(out, "relic-db.json")) ? JSON.parse(fs.readFileSync(path.join(out, "relic-db.json"), "utf8")) : { groups: {}, byUniqueName: {} };

// Stands in for window.__TAURI_INTERNALS__: events are delivered by hand, commands answer "nothing loaded".
function installBridge(relics) {
  const cbs = new Map();
  const listeners = new Map();
  let next = 1;
  const settings = { language: "en", game_language: "auto", theme: "dark", overlay_layouts: {}, overlay_enabled: {}, overlay_opacity: {}, theme_settings: "", ui_scale: 1, data_source_mode: "passive", riven_similar_auctions_shown: true };
  const noInventory = { path: null, found: false, source: "none", modifiedAt: null, lastError: null, importedAt: null, tradesApplied: 0, picksApplied: 0, gameSyncedSinceImport: false };
  const answers = { get_settings: settings, get_inventory_status: noInventory, get_inventory: null, overlay_layout_get: null, overlay_placement_get: [], get_item_database: {}, get_relic_database: relics, wfm_prices_bulk: [] };
  window.__unmocked = [];
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  window.__TAURI_INTERNALS__ = {
    transformCallback(cb) {
      cbs.set(next, cb);
      return next++;
    },
    unregisterCallback: (id) => cbs.delete(id),
    convertFileSrc: (p) => p,
    async invoke(cmd, args) {
      if (cmd === "plugin:event|listen") {
        listeners.set(args.event, [...(listeners.get(args.event) ?? []), args.handler]);
        return args.handler;
      }
      if (cmd.startsWith("plugin:event|")) return null;
      if (cmd in answers) return structuredClone(answers[cmd]);
      window.__unmocked.push(cmd);
      return null;
    },
  };
  window.__emit = (event, payload) => (listeners.get(event) ?? []).forEach((id) => cbs.get(id)?.({ event, id, payload }));
}

const server = await createServer({ root, configFile: path.join(root, "vite.config.ts"), server: { port: 14320, strictPort: false, hmr: false }, logLevel: "error" });
await server.listen();
const base = server.resolvedUrls.local[0];
const browser = await chromium.launch({ channel: "msedge" });
try {
  for (const file of fs.readdirSync(out).filter((f) => /^\d.*\.json$/.test(f))) {
    const doc = JSON.parse(fs.readFileSync(path.join(out, file), "utf8"));
    const events = doc.events.filter((e) => KIND[e.event]);
    if (!events.length) {
      console.log(`${doc.image}: nothing to draw (screen ${doc.screen})`);
      continue;
    }
    const shot = ["jpg", "jpeg", "png"].map((x) => path.join(shots, `${doc.image}.${x}`)).find(fs.existsSync);
    const page = await browser.newPage({ viewport: { width: 1920, height: 1080 }, deviceScaleFactor: 1 });
    await page.addInitScript(installBridge, relicDb);
    if (shot) await page.route("**/__shot", (r) => r.fulfill({ body: fs.readFileSync(shot), contentType: "image/jpeg" }));
    page.on("pageerror", (e) => console.log(`${doc.image}: page error ${e.message}`));
    await page.goto(`${base}overlay.html?kind=${KIND[events[0].event]}`);
    await page.addStyleTag({ content: "html,body{background:#000 url(/__shot) 0 0/1920px 1080px no-repeat !important}" });
    await page.waitForSelector("[data-overlay]", { state: "attached" });
    for (const e of events) await page.evaluate(([ev, p]) => window.__emit(ev, p), [e.event, e.payload]);
    await page.waitForTimeout(1500);
    const png = path.join(out, `${doc.image}.overlay.png`);
    await page.screenshot({ path: png });
    const unmocked = await page.evaluate(() => [...new Set(window.__unmocked)]);
    console.log(`${doc.image}: ${png}${unmocked.length ? ` (unmocked commands: ${unmocked})` : ""}${shot ? "" : " (no screenshot, black background)"}`);
    await page.close();
  }
} finally {
  await browser.close();
  await server.close();
}
