// Prints GitHub download counts per published release: manual installs (-setup.exe) vs updater downloads (-update.exe).
// These are download counts, not unique users. Usage: pnpm downloads
import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

export function summarize(releases) {
  const count = (r, suffix) => r.assets.find((a) => a.name.endsWith(suffix))?.download_count ?? null;
  return releases
    .filter((r) => !r.draft)
    .map((r) => ({
      tag: r.tag_name,
      published: (r.published_at ?? "").slice(0, 10),
      installs: count(r, "-setup.exe") ?? 0,
      updates: count(r, "-update.exe"),
    }));
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const repo =
    process.env.GITHUB_REPOSITORY ??
    execFileSync("git", ["remote", "get-url", "origin"], { encoding: "utf8" }).trim().replace(/^.*github\.com[:/]/, "").replace(/\.git$/, "");
  // --paginate prints one JSON array per page; --slurp merges them.
  const pages = JSON.parse(execFileSync("gh", ["api", `repos/${repo}/releases`, "--paginate", "--slurp"], { encoding: "utf8", maxBuffer: 1 << 26 }));
  const rows = summarize(pages.flat());
  const line = (a, b, c, d) => `${a.padEnd(10)}${b.padEnd(12)}${String(c).padStart(9)}${String(d).padStart(9)}`;
  console.log(line("tag", "published", "installs", "updates"));
  for (const r of rows) console.log(line(r.tag, r.published, r.installs, r.updates ?? "-"));
  const sum = (k) => rows.reduce((n, r) => n + (r[k] ?? 0), 0);
  console.log(line("total", "", sum("installs"), sum("updates")));
  if (rows.some((r) => r.updates === null)) console.log("\n- = release has no -update.exe: its installs also include auto-updates.");
}
