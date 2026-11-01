import type { RewardOverlayItem, RewardOverlaySlot } from "../../bindings";
import type { Translator } from "../../lib/i18n";

/** What a reward means for the player, shown as one badge on its card. */
export type RewardStatus =
  | { kind: "unknown" }
  | { kind: "mastered"; owned: number | null }
  | { kind: "unbuilt"; owned: number }
  | { kind: "owned"; owned: number }
  | { kind: "needed"; owned: number; required: number }
  | { kind: "notOwned" };

/**
 * The badge of one reward. `mastered` (inventory, else the public profile) is known even without an
 * inventory; owned counts are not, so then anything but "mastered" is unknown rather than a guessed 0.
 */
export function rewardStatus(item: RewardOverlayItem, hasInventory: boolean): RewardStatus {
  if (!hasInventory) return item.mastered ? { kind: "mastered", owned: null } : { kind: "unknown" };
  const owned = item.partOwnedCount;
  if (item.mastered) return { kind: "mastered", owned };
  // `mastered === false`: a masterable item never built. null (Forma...) has nothing to build.
  if (owned > 0) return item.mastered === false ? { kind: "unbuilt", owned } : { kind: "owned", owned };
  if (item.setRequiredCount > 0) return { kind: "needed", owned: item.setOwnedCount, required: item.setRequiredCount };
  return { kind: "notOwned" };
}

/** The badge text; "needed" keeps its k/n count apart so it can be drawn as its own chip. */
export function rewardStatusLabel(s: RewardStatus, tr: Translator): string {
  switch (s.kind) {
    case "unknown":
      return tr("overlay.reward.status.unknown");
    case "mastered":
      return s.owned == null ? tr("overlay.reward.status.masteredOnly") : tr("overlay.reward.status.mastered", { count: s.owned });
    case "unbuilt":
      return tr("overlay.reward.status.unbuilt", { count: s.owned });
    case "owned":
      return tr("overlay.reward.status.owned", { count: s.owned });
    case "needed":
      return tr("overlay.reward.status.needed");
    case "notOwned":
      return tr("overlay.reward.status.notOwned");
  }
}

/** Index of the card to take: the highest platinum price, else the most ducats; null when nothing is valued. */
export function bestSlot(slots: readonly (RewardOverlaySlot | null)[]): number | null {
  let best: number | null = null;
  const score = (s: RewardOverlaySlot) => [s.price ?? -1, s.item.ducats] as const;
  slots.forEach((s, i) => {
    if (!s || (s.price == null && s.item.ducats <= 0)) return;
    const b = best == null ? null : slots[best];
    if (!b) return void (best = i);
    const [p, d] = score(s);
    const [bp, bd] = score(b);
    if (p > bp || (p === bp && d > bd)) best = i;
  });
  return best;
}
