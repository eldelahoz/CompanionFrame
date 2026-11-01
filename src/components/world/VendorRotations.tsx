import { useMemo } from "react";
import type { Translator } from "../../lib/i18n";
import { timeTo } from "../../lib/format";
import { itemLabel } from "../../lib/itemLabel";
import { itemsByName } from "../../lib/world/itemsByName";
import { codaBatch, TENET_MELEE_STOCK, tenetRotatesAt } from "../../lib/world/dailiesLive";
import { buildBaroOwnedSet } from "../../lib/world/baroPlanner";
import { openItemBy } from "../../stores/modals";
import { useInventory } from "../../stores/inventory";
import { useItemDatabase } from "../../stores/items";
import { useClock } from "../../stores/world";
import IconButtonCard from "./IconButtonCard";

type VendorKind = "coda" | "tenet";

interface Props {
  vendors: readonly VendorKind[];
  /** "section" adds the vendor heading; "row" is the compact strip under a Dailies row. */
  variant?: "row" | "section";
  tr: Translator;
}

/** Coda (Eleanor) and Tenet (Ergo Glast) stock: fixed lists on a 4-day UTC grid, since the worldstate carries no rotation data. */
export default function VendorRotations({ vendors, variant = "row", tr }: Props) {
  const db = useItemDatabase();
  const { data: inventory } = useInventory();
  const nowMs = useClock(1000);
  const byName = useMemo(() => itemsByName(db), [db]);
  const owned = useMemo(() => buildBaroOwnedSet(inventory), [inventory]);
  const coda = codaBatch(nowMs);

  return (
    <>
      {vendors.map((kind) => {
        const names = kind === "coda" ? coda.weapons : TENET_MELEE_STOCK;
        const rotatesAt = kind === "coda" ? coda.rotatesAt : tenetRotatesAt(nowMs);
        return (
          <div key={kind} className={variant === "section" ? "border-t border-border py-3 first:border-t-0 first:pt-0" : "pl-8"} data-vendor={kind}>
            {variant === "section" && (
              <p className="m-0 flex items-center gap-2 text-sm font-semibold text-text-primary">
                {tr(kind === "coda" ? "dailies.task.codaWeapons" : "dailies.task.tenetMelee")}
                {kind === "coda" && <span className="ui-badge ui-badge--accent">{tr("dailies.codaBatch", { batch: coda.batch })}</span>}
              </p>
            )}
            <div className="flex flex-wrap gap-2 py-2">
              {names.map((name) => {
                const entry = byName.get(name);
                return (
                  <div key={name} data-vendor-weapon={name}>
                    <IconButtonCard
                      name={entry ? itemLabel(entry.uniqueName, entry) : name}
                      imageUrl={entry?.imageUrl}
                      owned={entry ? owned.has(entry.uniqueName) : false}
                      onClick={() => entry && openItemBy({ uniqueName: entry.uniqueName })}
                      tr={tr}
                    />
                  </div>
                );
              })}
            </div>
            <p className="m-0 text-xs text-text-muted" data-vendor-rotates={new Date(rotatesAt).toISOString()}>
              {tr("dailies.vendorRotatesIn", { time: timeTo(new Date(rotatesAt), nowMs) })}, {tr("dailies.vendorBonusWiki")}
            </p>
          </div>
        );
      })}
    </>
  );
}
