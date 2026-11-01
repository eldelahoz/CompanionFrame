import { useCallback, useEffect, useMemo, useState } from "react";
import { DEFAULT_SAFETY_SETTINGS, normalizeSafetySettings, type InventorySafetySettings, type SafetyContext } from "../lib/inventory/safetyRules";
import { foundryBuildProducts } from "../lib/inventory/foundryResources";
import { getMasteryProgress, type MasteryProgress } from "../lib/ipc";
import { foundryFor, ownershipFor } from "./inventory";
import { useMasteryPins } from "./masteryPins";
import { buildSelectionSafetyContext } from "../lib/tradeWorkbench/queueModel";
import type { ItemDbEntry, RawInventoryData } from "../types/inventory";

const KEY = "inventory.safety";

/** One blob: the rules are read together on every row, and a half-written pair of keys would reserve the wrong counts. */
const load = (): InventorySafetySettings => {
  try {
    const raw = localStorage.getItem(KEY);
    return raw ? normalizeSafetySettings(JSON.parse(raw)) : DEFAULT_SAFETY_SETTINGS;
  } catch {
    return DEFAULT_SAFETY_SETTINGS;
  }
};

const toggled = (list: readonly string[], key: string) => (list.includes(key) ? list.filter((k) => k !== key) : [...list, key]);

/**
 * Locks, spares and set-keep flags behind `safeToList`, plus the context they feed.
 * Mastery, pinned mastery goals and foundry builds are read here, so no rule stays degraded once they load.
 */
export function useInventorySafety(inventory: RawInventoryData | null, itemDb: Record<string, ItemDbEntry>) {
  const [settings, setSettings] = useState(load);
  const pins = useMasteryPins();
  // null until the first answer or when none exists: the unmastered-recipe rule then reads as degraded.
  const [mastery, setMastery] = useState<MasteryProgress | null>(null);
  // False until the first answer, so a rule is not called degraded while its data is still loading.
  const [masteryReady, setMasteryReady] = useState(false);
  // Re-read per inventory sync: the mastery data follows the same import.
  useEffect(() => {
    let live = true;
    const done = (d: MasteryProgress | null) => live && (setMastery(d), setMasteryReady(true));
    void getMasteryProgress().then(done, () => done(null));
    return () => void (live = false);
  }, [inventory]);
  const commit = useCallback((edit: (s: InventorySafetySettings) => InventorySafetySettings) => {
    setSettings((cur) => {
      const next = normalizeSafetySettings(edit(cur));
      try {
        localStorage.setItem(KEY, JSON.stringify(next));
      } catch {
        // best-effort persistence
      }
      return next;
    });
  }, []);

  const context: SafetyContext = useMemo(
    () =>
      buildSelectionSafetyContext({
        itemDb,
        settings,
        mastery: mastery as never,
        pins,
        buildingUniqueNames: inventory ? foundryBuildProducts(foundryFor(inventory, itemDb as never)) : undefined,
        ownership: ownershipFor(inventory, itemDb as never),
      }),
    [itemDb, settings, inventory, mastery, pins],
  );

  return {
    settings,
    context,
    mastery,
    masteryReady,
    setSpareDefault: (count: number) => commit((s) => ({ ...s, spareDefault: count })),
    /** `null` drops the override so the row falls back to the global default. */
    setItemSpare: (key: string, count: number | null) =>
      commit((s) => {
        const spares = { ...s.spares };
        if (count == null) delete spares[key];
        else spares[key] = count;
        return { ...s, spares };
      }),
    toggleLock: (key: string) => commit((s) => ({ ...s, locks: toggled(s.locks, key) })),
    /** Keyed by set ROOT uniqueName: pass `setRootOf(item.internalName)`. */
    toggleSetKeep: (root: string) => commit((s) => ({ ...s, setKeep: toggled(s.setKeep, root) })),
    reset: () => commit(() => DEFAULT_SAFETY_SETTINGS),
  };
}
