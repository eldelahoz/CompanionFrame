import type { Translator } from "../lib/i18n";
import type { CatalogIndex } from "../lib/wfm/catalog";
import type { OwnedCounts, RelicDatabase } from "../lib/relic/types";
import type { ItemDbEntry } from "../types/inventory";

/** Data every detail modal reads; ModalHost builds it once per render from the shared hooks. */
export interface ModalCtx {
  tr: Translator;
  locale: string;
  db: Record<string, ItemDbEntry>;
  /** Owned copies per uniqueName (foundry-pending blueprints excluded). */
  ownership: Map<string, number>;
  relicDb: RelicDatabase | null;
  relicOwned: OwnedCounts;
  catalog: CatalogIndex | null;
}
