import { describe, expect, it } from "vitest";
import { translator } from "./i18n";
import { ageLabel, freshnessText, freshnessTone, staleHint } from "./inventoryFreshness";
import type { InventoryStatus } from "./ipc";

const tr = translator("en");
const NOW = 1_000_000_000_000;
const status = (patch: Partial<InventoryStatus> = {}): InventoryStatus => ({
  path: null, found: true, source: "manual", modifiedAt: null, lastError: null,
  importedAt: NOW - 3 * 86_400_000, tradesApplied: 0, picksApplied: 0, gameSyncedSinceImport: false, ...patch,
});

describe("inventory freshness", () => {
  it("ages in the largest whole unit", () => {
    expect(ageLabel(NOW - 5_000, NOW, tr)).toBe("just now");
    expect(ageLabel(NOW - 5 * 60_000, NOW, tr)).toBe("5m ago");
    expect(ageLabel(NOW - 3 * 3_600_000, NOW, tr)).toBe("3h ago");
    expect(ageLabel(NOW - 3 * 86_400_000, NOW, tr)).toBe("3d ago");
  });

  it("lists the deltas applied since the import", () => {
    expect(freshnessText(status({ tradesApplied: 5 }), NOW, tr)).toBe("Imported 3d ago, 5 trades");
    expect(freshnessText(status({ tradesApplied: 1, picksApplied: 2 }), NOW, tr)).toBe("Imported 3d ago, 1 trade, 2 est. picks");
    expect(freshnessText(null, NOW, tr)).toBe("No inventory");
  });

  it("warns when the import is old or the game synced since", () => {
    expect(freshnessTone(status({ importedAt: NOW - 3_600_000 }), NOW)).toBe("success");
    expect(freshnessTone(status(), NOW)).toBe("success");
    expect(freshnessTone(status({ importedAt: NOW - 4 * 86_400_000 }), NOW)).toBe("warning");
    expect(freshnessTone(status({ importedAt: NOW, gameSyncedSinceImport: true }), NOW)).toBe("warning");
    expect(freshnessTone(status({ found: false }), NOW)).toBe("muted");
    expect(staleHint(status({ gameSyncedSinceImport: true }))).toBe("inventory.stale");
    expect(staleHint(status())).toBeNull();
  });
});
