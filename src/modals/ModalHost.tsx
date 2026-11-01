import { useEffect, useMemo, useState } from "react";
import { ownershipFor } from "../stores/inventory";
import { getRelicDatabase, onRelicDbUpdated, type ItemDatabase } from "../lib/ipc";
import type { Translator } from "../lib/i18n";
import { parseOwnedRelics } from "../lib/relic/relicInventory";
import type { RelicDatabase } from "../lib/relic/types";
import { buildItemNameIndex } from "../lib/componentResolution";
import { buildParsedItemFromDb } from "../lib/parsedItemFromDb";
import { readRelicViewPreference, simpleRelicFor } from "../lib/relic/relicView";
import { clearItemRequest, setActiveItem, setActiveRelic, useModals } from "../stores/modals";
import { useWfmCatalog } from "../stores/market";
import ComponentDetailModal from "./ComponentDetailModal";
import ItemDetailModal from "./ItemDetailModal";
import RelicDetailModal from "./RelicDetailModal";
import type { ModalCtx } from "./types";

interface Props {
  tr: Translator;
  locale: string;
  db: ItemDatabase;
  inventory: Record<string, unknown> | null;
}

/** Hosts every detail modal; any view opens one through stores/modals. */
export default function ModalHost({ tr, locale, db, inventory }: Props) {
  const { item, component, request, relic } = useModals();
  const open = item !== null || component !== null || relic !== null;
  const [relicDb, setRelicDb] = useState<RelicDatabase | null>(null);
  const { catalog } = useWfmCatalog();

  // The relic data is only needed once a modal is up.
  useEffect(() => {
    if (!open) return;
    let live = true;
    const load = () => void getRelicDatabase().then((next) => live && setRelicDb(next), () => {});
    load();
    const off = onRelicDbUpdated(load);
    return () => {
      live = false;
      void off.then((f) => f());
    };
  }, [open]);

  const itemDb = db as unknown as ModalCtx["db"]; // the generated type widens a few flags
  const ownership = ownershipFor(inventory, db);
  const relicOwned = useMemo(() => parseOwnedRelics(inventory, relicDb), [inventory, relicDb]);
  // Resolve a by-name request once the item database is there; an unknown name is dropped.
  useEffect(() => {
    if (!request || Object.keys(itemDb).length === 0) return;
    const uniqueName = "uniqueName" in request ? request.uniqueName : buildItemNameIndex(itemDb).get(request.name);
    const entry = uniqueName ? itemDb[uniqueName] : undefined;
    clearItemRequest();
    if (uniqueName && entry) setActiveItem(buildParsedItemFromDb(uniqueName, entry, ownership));
  }, [request, itemDb, ownership]);
  // A remembered "Normal" opens a relic as the plain item popup (the switch in the breakdown says so).
  useEffect(() => {
    if (!relic || readRelicViewPreference() !== "simple") return;
    const next = simpleRelicFor(relic, itemDb, ownership);
    if (next) {
      setActiveRelic(null);
      setActiveItem(next);
    }
  }, [relic, itemDb, ownership]);
  if (!open) return null;
  const ctx: ModalCtx = { tr, locale, db: itemDb, ownership, relicDb, relicOwned, catalog };

  return (
    <>
      {item && <ItemDetailModal ctx={ctx} item={item} />}
      {component && <ComponentDetailModal ctx={ctx} data={component} />}
      {relic && <RelicDetailModal ctx={ctx} group={relic} />}
    </>
  );
}
