import { ChevronDown, ChevronRight, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { sortRelicRewards } from "../../config/shared/relicRewardOrder";
import { dropRarityColour } from "../lib/dropDisplay";
import { itemLabel } from "../lib/itemLabel";
import type { Translator } from "../lib/i18n";
import { RELIC_QUALITY_SHORT_KEY } from "../lib/relic/relicConstants";
import { ownedRelicQualities, relicGroupForDisplayName } from "../lib/relic/relicInventory";
import type { OwnedCounts, RelicDatabase } from "../lib/relic/types";
import { ownedRelicDropsFirst } from "../lib/resolveDrops";
import { buildWikiUrl } from "../lib/wikiUrl";
import { openRelicDetailed, setActiveComponent } from "../stores/modals";
import type { DropInfo } from "../bindings";
import { RelicIcon, type RelicTier } from "./icons";
import { Button, IconButton } from "./ui";

interface Props {
  drops: DropInfo[];
  relicDb: RelicDatabase | null;
  relicOwned: OwnedCounts;
  tr: Translator;
  /** Empty means the default heading, which has to stay translatable. */
  title?: string;
  initialLimit?: number;
}

/** One row per relic (base row preferred), then relics whose refinement rows were the only ones listed. */
export function dedupeDrops(drops: DropInfo[], db: RelicDatabase | null): DropInfo[] {
  const out: DropInfo[] = [];
  const seen = new Set<string>();
  for (const d of drops) {
    const rg = relicGroupForDisplayName(db, d.location);
    if (!rg) {
      out.push(d);
      continue;
    }
    if (seen.has(rg.key)) continue;
    if (/\((Exceptional|Flawless|Radiant)\)\s*$/i.test(d.location)) continue;
    seen.add(rg.key);
    out.push(d);
  }
  for (const d of drops) {
    const rg = relicGroupForDisplayName(db, d.location);
    if (rg && !seen.has(rg.key)) {
      seen.add(rg.key);
      // Bare group name: the suffix is appended (translated) at render time.
      out.push({ ...d, location: rg.name });
    }
  }
  return out;
}

const CHIP = "ui-badge ui-badge--success";

export default function DropsList({ drops, relicDb, relicOwned, tr, title = "", initialLimit = 5 }: Props) {
  const [showAll, setShowAll] = useState(false);
  const [openKey, setOpenKey] = useState<string | null>(null);
  const deduped = useMemo(() => dedupeDrops(drops || [], relicDb), [drops, relicDb]);
  const listed = useMemo(() => ownedRelicDropsFirst(deduped, relicDb, relicOwned), [deduped, relicDb, relicOwned]);
  const dropsKey = (drops || []).map((d) => d.location).join("|");
  useEffect(() => {
    setShowAll(false);
    setOpenKey(null);
  }, [dropsKey]);

  if (deduped.length === 0) return null;
  const toggle = (key: string) => setOpenKey((cur) => (cur === key ? null : key));

  return (
    <div className="detail-section">
      <h3>{title || tr("drops.acquisition")}</h3>
      <div className="detail-acquisition">
        {(showAll ? listed : listed.slice(0, initialLimit)).map((d, i) => {
          const rg = relicGroupForDisplayName(relicDb, d.location);
          if (!rg) {
            return (
              <div key={`${d.location}-${i}`} className="flex items-center justify-start gap-2 border-b border-dashed border-border-subtle py-1.5 last:border-b-0">
                <span className="text-text-primary">{d.location}</span>
                {d.chance ? <span className="shrink-0 text-xs text-accent">{d.chance.toFixed(1)}%</span> : null}
                {d.rarity && <span className="text-text-muted">({d.rarity})</span>}
              </div>
            );
          }
          const ownedHere = ownedRelicQualities(relicOwned, rg.key);
          const open = openKey === rg.key;
          const quality = rg.qualities?.intact ?? Object.values(rg.qualities ?? {})[0];
          return (
            <div key={rg.key}>
              <button
                type="button"
                className={`-mx-2 flex w-full cursor-pointer items-center justify-between gap-2 rounded-md border-b border-dashed border-border-subtle px-2 py-1.5 text-left text-sm text-text-secondary transition-colors last:border-b-0 hover:bg-surface-hover hover:text-text-primary ${open ? "bg-surface-hover text-text-primary" : ""}`}
                onClick={(e) => {
                  e.stopPropagation();
                  toggle(rg.key);
                }}
                onKeyDown={(e) => e.key === "Escape" && setOpenKey(null)}
              >
                <span className="flex min-w-0 flex-wrap items-center gap-1.5">
                  <span className="text-text-primary">
                    {rg.name} {tr("drops.relicSuffix")}
                  </span>
                  <span className={`ui-badge ui-badge--${rg.vaulted ? "warning" : "success"}`} data-relic-vault={rg.vaulted ? "vaulted" : "unvaulted"} title={rg.vaulted ? tr("common.vaulted") : tr("common.unvaulted")}>
                    {rg.vaulted ? tr("common.vaulted") : tr("common.unvaulted")}
                  </span>
                  {ownedHere.map((e) => (
                    <span key={e.quality} className={CHIP} data-relic-owned={e.quality}>
                      {tr(RELIC_QUALITY_SHORT_KEY[e.quality])} {e.count}
                    </span>
                  ))}
                </span>
                <span className="flex shrink-0 items-center gap-2">
                  {d.chance ? <span className="text-xs text-accent">{d.chance.toFixed(1)}%</span> : null}
                  {d.rarity && <span className="text-text-muted">({d.rarity})</span>}
                  <span className="text-text-muted" aria-hidden="true">
                    {open ? <ChevronDown className="h-4 w-4" /> : <ChevronRight className="h-4 w-4" />}
                  </span>
                </span>
              </button>
              {open && (
                <div className="my-2 rounded-lg border border-border-strong bg-bg-raised px-3 py-2.5 shadow-[var(--ui-panel-shadow)]">
                  <div className="mb-2 flex items-center gap-2 border-b border-border pb-2">
                    <RelicIcon tier={rg.tier.toLowerCase() as RelicTier} label={rg.name} className="h-8 w-8 shrink-0" />
                    <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                      <span className="truncate font-display text-sm font-semibold text-text-primary">{rg.name}</span>
                      <span className="flex flex-wrap items-center gap-1.5">
                        <span className={`ui-badge ${rg.vaulted ? "ui-badge--warning" : "ui-badge--info"}`} data-relic-popover-vault={rg.vaulted ? "vaulted" : "unvaulted"}>
                          {rg.vaulted ? tr("common.vaulted") : tr("common.unvaulted")}
                        </span>
                        {ownedHere.map((e) => (
                          <span key={e.quality} className={CHIP}>
                            {tr(RELIC_QUALITY_SHORT_KEY[e.quality])} {e.count}
                          </span>
                        ))}
                      </span>
                    </div>
                    <IconButton label={tr("common.close")} onClick={() => setOpenKey(null)}>
                      <X />
                    </IconButton>
                  </div>
                  <div className="mb-2 flex items-center gap-1.5">
                    <Button
                      size="sm"
                      className="flex-1"
                      onClick={() => {
                        setOpenKey(null);
                        setActiveComponent(null);
                        openRelicDetailed(rg);
                      }}
                    >
                      {tr("common.detailed")}
                    </Button>
                    <a className="ui-btn ui-btn--sm flex-1" href={buildWikiUrl(rg.name)} target="_blank" rel="noreferrer noopener">
                      {tr("common.wiki")}
                    </a>
                  </div>
                  <div className="flex max-h-[240px] flex-col overflow-y-auto">
                    {sortRelicRewards(quality?.rewards ?? []).map((r, ri) => (
                      <div key={ri} className="flex items-center gap-2 border-b border-dashed border-border-subtle py-1 last:border-b-0">
                        {r.imageUrl && <img src={r.imageUrl} alt={itemLabel(r.uniqueName ?? "", r)} className="h-[22px] w-[22px] shrink-0 object-contain opacity-90" />}
                        <span className="min-w-0 flex-1 truncate text-xs text-text-primary">{itemLabel(r.uniqueName ?? "", r)}</span>
                        <span className="shrink-0 text-xs font-semibold" style={{ color: dropRarityColour(r.rarity) }}>
                          {r.rarity}
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>
          );
        })}
        {!showAll && deduped.length > initialLimit && (
          <Button variant="ghost" size="sm" className="w-full justify-start" data-drops-show-all onClick={() => setShowAll(true)}>
            {tr("drops.viewAllSources", { count: deduped.length })}
          </Button>
        )}
        {showAll && deduped.length > initialLimit && (
          <Button variant="ghost" size="sm" className="w-full justify-start" onClick={() => setShowAll(false)}>
            {tr("common.showFewer")}
          </Button>
        )}
      </div>
    </div>
  );
}
