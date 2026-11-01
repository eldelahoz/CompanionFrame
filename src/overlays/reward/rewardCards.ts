import type { RewardOverlayItem, RewardOverlayPayload, RewardOverlayView } from "../../bindings";
import type { Translator } from "../../lib/i18n";
import { toMarketSlug } from "../../lib/marketNaming";
import type { CardView } from "./RewardOverlayCanvas";
import { bestSlot, rewardStatus } from "./rewardStatus";

const RARITIES = ["common", "uncommon", "rare"] as const;
type Rarity = (typeof RARITIES)[number];

/** The cards of a live payload, left to right, in the form the canvas shows. */
export function rewardCardViews(payload: RewardOverlayPayload, tr: Translator, hasInventory: boolean): CardView[] {
  const best = bestSlot(payload.slots);
  return payload.slots.flatMap((slot, i) => {
    if (!slot) return [];
    const { item } = slot;
    const r = item.rarity.toLowerCase();
    const rarity = (RARITIES as readonly string[]).includes(r) ? (r as Rarity) : null;
    const status = rewardStatus(item, hasInventory);
    return {
      name: item.name,
      rarity,
      rarityLabel: rarity ? tr(`overlay.reward.rarity.${rarity}`) : item.rarity,
      platinum: slot.price ?? null,
      ducats: item.ducats > 0 ? item.ducats : null,
      status,
      // The "needed" badge already carries the set progress; else it is its own chip.
      setOwned: hasInventory && status.kind !== "needed" && item.setRequiredCount > 0 ? { owned: item.setOwnedCount, required: item.setRequiredCount } : null,
      setPrice: slot.setPrice ?? null,
      foundry: hasInventory && item.building,
      vaulted: !!item.vaulted,
      best: i === best,
      slug: toMarketSlug(item.name) || null,
      imageUrl: item.setParts.find((p) => p.isReward)?.imageUrl ?? null,
    };
  });
}

// Outside Tauri there is no wfmimg:// cache, so the preview round carries the item DB’s own (WFCD) image URLs.
const art = (file: string) => [{ name: "", imageUrl: `https://cdn.warframestat.us/img/${file}.png`, ownedCount: 0, requiredCount: 1, isReward: true, building: false }];

/** Mock round for `overlay.html?kind=reward&mode=preview` (browser previews and screenshots). */
export const PREVIEW_VIEW: RewardOverlayView = {
  phase: "ready",
  failure: null,
  autoCloseMs: 8500,
  token: 1,
  presentation: {
    count: 4,
    slots: [
      { item: previewItem("Ash Prime Systems Blueprint", "Rare", 100, { partOwnedCount: 0, mastered: false, setOwnedCount: 2, setUrlName: "ash_prime_set", setParts: art("AshPrime") }), price: 38, setPrice: 92 },
      { item: previewItem("Nekros Prime Blueprint", "Uncommon", 45, { partOwnedCount: 2, mastered: false, setOwnedCount: 3, setUrlName: "nekros_prime_set", setParts: art("NekrosPrime") }), price: 15, setPrice: 70 },
      { item: previewItem("Braton Prime Receiver", "Common", 15, { partOwnedCount: 1, mastered: true, vaulted: false, setOwnedCount: 4, setUrlName: "braton_prime_set", setParts: art("BratonPrime") }), price: 4, setPrice: 24 },
      { item: previewItem("Forma Blueprint", "Common", 0, { partOwnedCount: 3, vaulted: null, setRequiredCount: 0, setOwnedCount: 0, setUrlName: null, setParts: art("GenericComponent") }), price: null, setPrice: null },
    ],
  },
};

function previewItem(name: string, rarity: string, ducats: number, over: Partial<RewardOverlayItem> = {}): RewardOverlayItem {
  return {
    name, rarity, ducats, partOwnedCount: 1, partRequiredCount: 1, mastered: null, vaulted: true, building: false,
    setOwnedCount: 2, setRequiredCount: 4, setUrlName: null, setParts: [], ...over,
  };
}

