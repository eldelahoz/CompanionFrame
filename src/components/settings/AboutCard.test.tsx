// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { relicDataLabel } from "./AboutCard";

const tr = ((k: string, p?: Record<string, string>) => (p ? `${k}:${p.version}:${p.date}` : k)) as never;

describe("relicDataLabel", () => {
  it("shows the date only for a downloaded set", () => {
    expect(relicDataLabel(tr, "en", { version: "1.2", source: "bundled", publishedAt: "2026-01-02T00:00:00Z", updateError: null, updateOffline: false, newerUnusable: false })).toBe("1.2");
    expect(relicDataLabel(tr, "en", { version: "1.2", source: "downloaded", publishedAt: "2026-01-02T12:00:00Z", updateError: null, updateOffline: false, newerUnusable: false })).toContain("settings.creditRelicDataValue:1.2:");
  });
  it("says the update failed, with the reason, even on bundled data", () => {
    const info = { version: "1.2", source: "bundled", publishedAt: null, updateError: "0 valid relics", updateOffline: false, newerUnusable: false } as const;
    expect(relicDataLabel(tr, "en", info)).toContain("settings.creditRelicDataFailed:1.2:");
  });
  it("is neutral when only offline", () => {
    const info = { version: "1.2", source: "downloaded", publishedAt: null, updateError: "timeout", updateOffline: true, newerUnusable: false } as const;
    expect(relicDataLabel(tr, "en", info)).toContain("settings.creditRelicDataOffline");
  });
  it("says newer releases have no relic data instead of a failure", () => {
    const info = { version: "1.1276.29", source: "downloaded", publishedAt: null, updateError: null, updateOffline: false, newerUnusable: true } as const;
    expect(relicDataLabel(tr, "en", info)).toContain("settings.creditRelicDataNewerUnusable:1.1276.29");
  });
  it("is empty without a version", () => {
    expect(relicDataLabel(tr, "en", null)).toBeNull();
  });
});
