// Usage: node scripts/bump-version.mjs X.Y.Z
// Sets the version in package.json, src-tauri/tauri.conf.json, src-tauri/Cargo.toml and the companionframe entry of Cargo.lock.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "..");
const SEMVER = /^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/;

// Each rule rewrites the first match only; a missing match throws so a silent no-op cannot ship.
export const rules = [
  ["package.json", /("version": ")[^"]+/],
  ["src-tauri/tauri.conf.json", /("version": ")[^"]+/],
  ["src-tauri/Cargo.toml", /(^version = ")[^"]+/m],
  ["src-tauri/Cargo.lock", /(name = "companionframe"\r?\nversion = ")[^"]+/],
];

export function bump(text, re, version) {
  if (!re.test(text)) throw new Error(`version pattern ${re} not found`);
  return text.replace(re, `$1${version}`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const version = process.argv[2];
  if (!SEMVER.test(version ?? "")) throw new Error("usage: node scripts/bump-version.mjs X.Y.Z");
  for (const [file, re] of rules) {
    const p = path.join(root, file);
    fs.writeFileSync(p, bump(fs.readFileSync(p, "utf8"), re, version));
    console.log(`${file} -> ${version}`);
  }
}
