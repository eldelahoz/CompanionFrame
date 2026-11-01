import type { MessageKey } from "../i18n";
import type { RelicQuality } from "./types";

export { RELIC_QUALITY_MODES as QUALITY_MODES } from "../../../config/shared/relicPlannerView";

/** Int / Ex / Fl / Rad. */
export const RELIC_QUALITY_SHORT_KEY: Record<RelicQuality, MessageKey> = {
  intact: "relics.qualityShort.intact",
  exceptional: "relics.qualityShort.exceptional",
  flawless: "relics.qualityShort.flawless",
  radiant: "relics.qualityShort.radiant",
};

export const RELIC_QUALITY_LABEL_KEY: Record<RelicQuality, MessageKey> = {
  intact: "relics.quality.intact",
  exceptional: "relics.quality.exceptional",
  flawless: "relics.quality.flawless",
  radiant: "relics.quality.radiant",
};

export function fissureTierClass(tier: string = ""): string {
  const t = tier.toLowerCase();
  for (const known of ["lith", "meso", "neo", "axi", "requiem", "omnia"]) {
    if (t.includes(known)) return known;
  }
  return "default";
}
