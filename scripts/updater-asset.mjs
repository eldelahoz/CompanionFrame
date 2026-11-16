// Gives the auto-updater its own copy of the installer so GitHub's per-asset download counters
// separate manual installs (-setup.exe) from updates (-update.exe). The copy is byte-identical, and the
// updater verifies the signature over the bytes, never the file name, so latest.json keeps its signature.
// Usage (the release workflow, after tauri-action): node scripts/updater-asset.mjs vX.Y.Z
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

export function updateAssetName(setupName) {
  if (!setupName.endsWith("-setup.exe")) throw new Error(`not a -setup.exe name: ${setupName}`);
  return setupName.replace(/-setup\.exe$/, "-update.exe");
}

export function rewriteLatest(latest, { repo, tag, assetName }) {
  const entries = Object.entries(latest.platforms ?? {});
  if (!entries.length) throw new Error("latest.json has no platforms");
  const url = `https://github.com/${repo}/releases/download/${tag}/${assetName}`;
  const platforms = {};
  for (const [key, p] of entries) {
    if (!p.signature) throw new Error(`platform ${key} has no signature`);
    platforms[key] = { ...p, url };
  }
  return { ...latest, platforms };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const tag = process.argv[2];
  const repo = process.env.GITHUB_REPOSITORY;
  if (!tag || !repo) throw new Error("usage: GITHUB_REPOSITORY=owner/repo node scripts/updater-asset.mjs <tag>");
  const gh = (...args) => execFileSync("gh", args, { stdio: "inherit" });
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "updater-"));
  try {
    gh("release", "download", tag, "-R", repo, "-D", dir, "-p", "latest.json", "-p", "*-setup.exe");
    const setups = fs.readdirSync(dir).filter((f) => f.endsWith("-setup.exe"));
    if (setups.length !== 1) throw new Error(`expected exactly one -setup.exe, found ${setups.length}`);
    const assetName = updateAssetName(setups[0]);
    fs.copyFileSync(path.join(dir, setups[0]), path.join(dir, assetName));
    const file = path.join(dir, "latest.json");
    const latest = rewriteLatest(JSON.parse(fs.readFileSync(file, "utf8")), { repo, tag, assetName });
    fs.writeFileSync(file, JSON.stringify(latest, null, 2));
    gh("release", "upload", tag, "-R", repo, path.join(dir, assetName), file, "--clobber");
    console.log(`uploaded ${assetName}; latest.json now points at it (${Object.keys(latest.platforms).join(", ")})`);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}
