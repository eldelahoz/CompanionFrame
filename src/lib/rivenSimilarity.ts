// Riven stats that share one attribute under several display/OCR/locTag names; compare via this canonical form, never by substring.
const STAT_NAME_CANON: Record<string, string> = {
  "attack speed": "fire rate",
  "melee damage": "damage",
  "critical chance for slide attack": "slide attack",
  slide: "slide attack",
  recoil: "weapon recoil",
  magazine: "magazine capacity",
  "heavy attack": "heavy attack efficiency",
};

export function canonicalRivenStatName(name: string | null | undefined): string {
  const n = (name || "").toLowerCase().trim();
  return STAT_NAME_CANON[n] || n;
}

// WFM sends the in-game signed value (zoom -4.4, recoil 4.6) and faction damage as a multiplier (0.98), buff or curse alike.
export function formatRivenListingStatValue(stat: { name?: string | null; value: number }): string {
  if (/^Damage Vs /i.test(stat.name || "")) return `x${stat.value.toFixed(2)}`;
  return `${stat.value < 0 ? "−" : "+"}${Math.abs(Math.round(stat.value))}%`;
}

export function computeRivenStatSimilarity(myStatNames: string[], listingStats: Array<{ name?: string | null }>): { pct: number; matchedNames: Set<string> } {
  const matchedNames = new Set<string>();
  if (!myStatNames.length || !listingStats.length) return { pct: 0, matchedNames };
  const listingNames = listingStats.map((s) => String(s.name || "").trim().toLowerCase()).filter(Boolean);
  for (const mine of myStatNames) {
    const match = listingNames.find((name) => name === mine || name.includes(mine) || mine.includes(name));
    if (match) matchedNames.add(match);
  }
  const union = myStatNames.length + listingNames.length - matchedNames.size;
  return { pct: union > 0 ? Math.round((matchedNames.size / union) * 100) : 0, matchedNames };
}
