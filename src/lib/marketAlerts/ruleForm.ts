import type { MarketAlertRule } from "../ipc";
import type { MessageKey } from "../i18n";
import { TAG_TO_WFM_URL_NAME } from "../../../config/shared/wfmRivenVocabulary";

export const MIN_COOLDOWN_MINUTES = 5;
export const MAX_COOLDOWN_MINUTES = 24 * 60;
const MAX_NAME_CHARS = 60;

/** WFM auction attribute slugs a riven rule can require or exclude (the backend rejects any other). */
export const RIVEN_ATTRIBUTES: readonly string[] = [...new Set(Object.values(TAG_TO_WFM_URL_NAME))].sort();

export const attributeLabel = (slug: string): string => slug.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());

export function emptyRule(kind: "riven" | "item"): MarketAlertRule {
  return {
    id: "",
    name: "",
    kind,
    enabled: true,
    cooldownMinutes: 30,
    noCooldown: false,
    riven: kind === "riven" ? { weaponUrlName: "", requirePositive: [], allowedNegatives: null, excludeNegatives: null, excludeAttributes: [], hasNegative: null, positiveCount: null, minSimilarityPct: null, includeBidOnly: null, statBounds: [], minMasteryRank: null, maxMasteryRank: null, polarity: null, minModRank: null, maxModRank: null, minPlatinum: null, maxPlatinum: null, minRerolls: null, maxRerolls: null, minEndoPerPlat: null, statuses: [] } : null,
    item: kind === "item" ? { itemUrlName: "", side: "sell", maxPlatinum: null, minPlatinum: null, minQuantity: null, statuses: [], ownedBelow: null, ownedAbove: null } : null,
    baro: null,
  };
}

/** First problem the form can catch before the backend does (it stays the authority), as a message key. */
export function validateRule(rule: MarketAlertRule): MessageKey | null {
  if (!rule.name.trim()) return "marketAlerts.nameRequired";
  if (rule.kind === "riven" && !rule.riven?.weaponUrlName) return "marketAlerts.weaponRequired";
  if (rule.kind === "item") {
    if (!rule.item?.itemUrlName) return "marketAlerts.itemRequired";
    if (rule.item.maxPlatinum == null && rule.item.minPlatinum == null) return "marketAlerts.priceRequired";
  }
  return null;
}

/** Trims the name and keeps the cooldown inside the range the backend accepts. */
export const normalizeRule = (rule: MarketAlertRule): MarketAlertRule => ({
  ...rule,
  name: rule.name.trim().slice(0, MAX_NAME_CHARS),
  cooldownMinutes: Math.min(MAX_COOLDOWN_MINUTES, Math.max(MIN_COOLDOWN_MINUTES, Math.round(rule.cooldownMinutes) || MIN_COOLDOWN_MINUTES)),
});

/** Empty input is "no bound" (null), not 0. */
export const parseOptionalNumber = (raw: string): number | null => {
  const t = raw.trim();
  if (t === "") return null;
  const n = Number(t);
  return Number.isFinite(n) ? n : null;
};
