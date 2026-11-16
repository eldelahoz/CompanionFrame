import { describe, expect, it } from "vitest";
import { rewriteLatest, updateAssetName } from "./updater-asset.mjs";

const latest = () => ({
  version: "0.5.4",
  notes: "Fixes things",
  pub_date: "2026-11-10T12:00:00.000Z",
  platforms: {
    "windows-x86_64": { signature: "SIG-A", url: "https://api.github.com/repos/o/r/releases/assets/111" },
    "windows-x86_64-nsis": { signature: "SIG-B", url: "https://api.github.com/repos/o/r/releases/assets/222" },
  },
});
const opts = { repo: "o/r", tag: "v0.5.4", assetName: "CompanionFrame_0.5.4_x64-update.exe" };
const url = "https://github.com/o/r/releases/download/v0.5.4/CompanionFrame_0.5.4_x64-update.exe";

describe("rewriteLatest", () => {
  it("repoints every platform url and keeps the rest", () => {
    const input = latest();
    const out = rewriteLatest(input, opts);
    expect(Object.keys(out.platforms)).toEqual(["windows-x86_64", "windows-x86_64-nsis"]);
    expect(out.platforms["windows-x86_64"]).toEqual({ signature: "SIG-A", url });
    expect(out.platforms["windows-x86_64-nsis"]).toEqual({ signature: "SIG-B", url });
    expect(out.version).toBe("0.5.4");
    expect(out.notes).toBe("Fixes things");
    expect(out.pub_date).toBe("2026-11-10T12:00:00.000Z");
  });

  it("does not mutate the input", () => {
    const input = latest();
    rewriteLatest(input, opts);
    expect(input).toEqual(latest());
  });

  it("throws on missing or empty platforms", () => {
    expect(() => rewriteLatest({ version: "1" }, opts)).toThrow(/no platforms/);
    expect(() => rewriteLatest({ platforms: {} }, opts)).toThrow(/no platforms/);
  });

  it("throws on a platform without a signature", () => {
    const bad = latest();
    bad.platforms["windows-x86_64-nsis"].signature = "";
    expect(() => rewriteLatest(bad, opts)).toThrow(/windows-x86_64-nsis.*signature/);
    delete bad.platforms["windows-x86_64-nsis"].signature;
    expect(() => rewriteLatest(bad, opts)).toThrow(/signature/);
  });
});

describe("updateAssetName", () => {
  it("swaps -setup.exe for -update.exe", () => {
    expect(updateAssetName("CompanionFrame_0.5.4_x64-setup.exe")).toBe("CompanionFrame_0.5.4_x64-update.exe");
  });

  it("throws on other names", () => {
    for (const n of ["latest.json", "CompanionFrame_0.5.4_x64-setup.exe.sig", "CompanionFrame_0.5.4_x64-update.exe", "setup.exe"]) {
      expect(() => updateAssetName(n)).toThrow(/-setup\.exe/);
    }
  });
});

it("importing the module does not run the CLI", async () => {
  // The CLI throws without a tag/repo, so a failed import would reject here.
  await expect(import("./updater-asset.mjs")).resolves.toBeDefined();
});
