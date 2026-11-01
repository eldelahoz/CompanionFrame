// Copies the game data the app reads from the two npm data packages into
// src-tauri/resources/game-data (bundled by tauri.conf.json bundle.resources).
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "..");
const out = path.join(root, "src-tauri", "resources", "game-data");
const pkg = (name) => fs.realpathSync(path.join(root, "node_modules", name));

// Only the languages the app ships; the other 11 dictionaries are ~40 MB.
const LOCALES = ["en", "de", "zh", "fr", "es"];
// @wfcd/items files the app never reads (same list as WFHelper's package.json build.files).
const WFCD_UNSHIPPED = ["Enemy", "Glyphs", "Node", "Quests", "Railjack", "SentinelWeapons", "Sigils", "Skins", "i18n"];

// Skip the 70 MB copy when the packages, locales and file filters have not changed since the last run.
const version = [pkg("warframe-public-export-plus"), pkg("@wfcd/items")]
  .map((d) => JSON.parse(fs.readFileSync(path.join(d, "package.json"), "utf8")).version)
  .join("+") + `|${LOCALES}|${WFCD_UNSHIPPED}`;
const stamp = path.join(out, ".version");
if (fs.existsSync(stamp) && fs.readFileSync(stamp, "utf8") === version) process.exit(0);

fs.rmSync(path.join(out, "pep"), { recursive: true, force: true });
fs.rmSync(path.join(out, "wfcd"), { recursive: true, force: true });
fs.mkdirSync(path.join(out, "pep"), { recursive: true });
fs.mkdirSync(path.join(out, "wfcd"), { recursive: true });

const pep = pkg("warframe-public-export-plus");
for (const f of fs.readdirSync(pep)) {
  const dict = /^dict\.([a-z]{2})\.json$/.exec(f);
  if (/^Export[A-Za-z]+\.json$/.test(f) || (dict && LOCALES.includes(dict[1]))) {
    fs.copyFileSync(path.join(pep, f), path.join(out, "pep", f));
  }
}

const wfcd = pkg("@wfcd/items");
fs.copyFileSync(path.join(wfcd, "package.json"), path.join(out, "wfcd", "package.json"));
const data = path.join(wfcd, "data", "json");
for (const f of fs.readdirSync(data)) {
  if (f.endsWith(".json") && !WFCD_UNSHIPPED.includes(f.slice(0, -5))) {
    fs.copyFileSync(path.join(data, f), path.join(out, "wfcd", f));
  }
}
fs.writeFileSync(stamp, version);
console.log(`game data copied to ${out}`);
