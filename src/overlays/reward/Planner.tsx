import { Info, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { RelicIcon, type RelicTier } from "../../components/icons";
import { commands, type RelicPlannerEvent } from "../../bindings";
import { getRawInventory, getRelicDatabase, inTauri } from "../../lib/ipc";
import { RELIC_QUALITY_LABEL_KEY } from "../../lib/relic/relicConstants";
import { parseOwnedRelics } from "../../lib/relic/relicInventory";
import type { RelicDatabase } from "../../lib/relic/types";
import { OvReadout } from "../common/OvReadout";
import type { OverlayContext } from "../common/OverlayShell";
import { loadRelicOverlayFilters } from "../../lib/relic/overlayFilters";
import { relicPrice, useRelicPrices } from "../../lib/relic/relicPrices";
import { SAMPLE_PLANNER_ROWS } from "../common/moveSamples";
import { plannerPriceSlugs, plannerRows } from "./plannerRows";

/** Event of the backend's relic planner: detecting, ready (with the era) or closed. */
export const PLANNER_EVENT = "relic-planner";
const SHOWN_ROWS = 5;
const SQUADS = [
  [1, "relics.squad.solo"],
  [2, "relics.squad.p2"],
  [3, "relics.squad.p3"],
  [4, "relics.squad.p4"],
] as const;

const TIERS: readonly RelicTier[] = ["lith", "meso", "neo", "axi", "requiem"];

/**
 * The relics the player should open for the fissure's era. The ranking is local: changing the squad
 * size re-ranks without the backend re-reading the screen. With no detected era every owned relic
 * is listed, so a failed OCR degrades to a longer list rather than an error.
 */
export function Planner({ tr, interactive, moving, event }: OverlayContext & { event: RelicPlannerEvent }) {
  const [db, setDb] = useState<RelicDatabase | null>(null);
  // undefined while loading; null when no inventory was ever loaded (passive mode after a wipe).
  const [inventory, setInventory] = useState<Record<string, unknown> | null | undefined>(undefined);
  const [squad, setSquad] = useState(4);
  // The inventory may have synced since the last picker; reload per scan.
  useEffect(() => {
    void getRelicDatabase().then(setDb, () => {});
    void getRawInventory().then(setInventory, () => {});
  }, [event.token]);

  const owned = useMemo(() => parseOwnedRelics(inventory ?? null, db), [inventory, db]);
  // The Relics view's pushed filters, read per scan like the inventory.
  const push = useMemo(() => loadRelicOverlayFilters(), [event.token]);
  // No inventory at all (passive mode without an import): rank every relic of the era instead.
  const allRelics = inventory === null;
  // Plat EV from warframe.market medians, the session cache the Relics view also uses; rows re-rank as prices land.
  const slugs = useMemo(() => (db ? plannerPriceSlugs(db, event.era ?? null) : []), [db, event.era]);
  const priceRev = useRelicPrices(slugs);
  const computed = useMemo(
    () => (db ? plannerRows(db, owned, event.era ?? null, squad, push, allRelics, relicPrice).slice(0, SHOWN_ROWS) : []),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [db, owned, event.era, squad, push, allRelics, priceRev],
  );
  // Move mode shows fixed sample rows, whatever the inventory holds.
  const rows = moving ? SAMPLE_PLANNER_ROWS : computed;
  const showAll = allRelics && !moving;
  const detecting = event.phase === "detecting";
  const close = () => {
    if (inTauri()) void commands.relicPlannerClose();
  };

  const tier = (t: string) => (TIERS.includes(t as RelicTier) ? (t as RelicTier) : null);
  const eraTier = event.era ? tier(event.era) : null;
  return (
    <div className="overlay-panel relic-planner" data-interactive={interactive}>
      <header className="pl-head">
        {eraTier && <RelicIcon tier={eraTier} refinement="radiant" className="pl-era" />}
        <div className="pl-title">
          <h3>{tr("overlay.planner.title")}</h3>
          <span className="pl-sub">
            {detecting ? tr("overlay.planner.detecting") : event.era ? tr(`relics.tier.${event.era}` as "relics.tier.lith") : tr("overlay.planner.noEra")}
          </span>
        </div>
        {interactive && (
          <>
            <select aria-label={tr("relics.squadTitle")} value={squad} onChange={(e) => setSquad(Number(e.target.value))}>
              {SQUADS.map(([n, key]) => (
                <option key={n} value={n}>
                  {tr(key)}
                </option>
              ))}
            </select>
            <button type="button" className="ov-btn ov-btn-icon" aria-label={tr("overlay.planner.close")} onClick={close}>
              <X className="ov-icon" aria-hidden />
            </button>
          </>
        )}
      </header>
      {event.ocrUnavailable && <p role="alert" className="pl-note">{tr("overlay.planner.ocrUnavailable")}</p>}
      {!detecting && showAll && (
        <p className="pl-note relic-planner-note">
          <Info className="ov-icon" aria-hidden />
          {tr("overlay.planner.noInventory")}
        </p>
      )}
      {!detecting && rows.length === 0 && inventory !== undefined && <p className="pl-note">{tr("relics.empty")}</p>}
      {rows.length > 0 && (
        <div className="pl-cols">
          <span>{tr("overlay.planner.colRelic")}</span>
          <span className="pl-num">{tr("overlay.planner.colPlat")}</span>
          <span className="pl-num">{tr("overlay.planner.colDucat")}</span>
        </div>
      )}
      <ol>
        {rows.map((r, i) => {
          const t = tier(r.tier.toLowerCase());
          return (
            <li key={r.group.key} className="pl-row" data-best={i === 0 || undefined}>
              {t ? <RelicIcon tier={t} refinement={r.quality} className="pl-relic" /> : <span />}
              <div>
                <div className="relic-planner-name">{r.name}</div>
                <div className="pl-q">
                  <span>{tr(RELIC_QUALITY_LABEL_KEY[r.quality])}</span>
                  {!showAll && <span>{`×${r.count}`}</span>}
                </div>
              </div>
              {r.plat != null ? <OvReadout currency="platinum" value={r.plat.toFixed(1)} /> : <span className="relic-planner-ev">—</span>}
              {r.ducat != null ? <OvReadout currency="ducat" value={r.ducat.toFixed(1)} dim /> : <span className="relic-planner-ev">—</span>}
            </li>
          );
        })}
      </ol>
    </div>
  );
}
