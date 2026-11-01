import type { MessageKey } from "./i18n";

// Order and label keys of the routable views (setup wizard excluded). Sidebar glyphs: components/icons/NavIcon.
export const VIEWS = [
  ["dashboard", "nav.dashboard"],
  ["inventory", "common.inventory"],
  ["foundry", "common.foundry"],
  ["mastery", "common.mastery"],
  ["stats", "common.stats"],
  ["world", "common.world"],
  ["syndicates", "common.syndicates"],
  ["market", "common.market"],
  ["analytics", "common.analytics"],
  ["relics", "common.relics"],
  ["wiki", "common.wiki"],
  ["rivens", "common.rivens"],
  ["arbi", "nav.runAnalysis"],
  ["missions", "enemy.missions"],
  ["settings", "common.settings"],
] as const satisfies ReadonlyArray<readonly [string, MessageKey]>;

export type ViewName = (typeof VIEWS)[number][0];

