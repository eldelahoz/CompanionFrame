import { useMemo } from "react";
import ItemImage from "../ItemImage";
import { Badge, Card, EmptyState, Segmented } from "../ui";
import { usePersisted } from "../../lib/usePersisted";
import type { MessageKey, Translator } from "../../lib/i18n";
import { sanitizeDisplayName } from "../../../config/shared/displayName";
import {
  INCARNON_FILTERS,
  INCARNON_MAX_EVOLUTION,
  buildIncarnonWeapons,
  circuitWeeksLabel,
  filterIncarnon,
  incarnonUnlockedLabel,
  summarizeIncarnon,
  type IncarnonFilter,
  type IncarnonWeapon,
} from "../../lib/incarnon";
import type { ItemDbEntry, RawInventoryData } from "../../types/inventory";
import { openItemBy } from "../../stores/modals";
import { useWorldState } from "../../stores/world";

const FILTER_LABELS: Record<IncarnonFilter, MessageKey> = {
  all: "common.all",
  unlocked: "incarnon.filterUnlocked",
  adapter: "incarnon.filterAdapter",
  missing: "common.missing",
};
const EVOLUTION_PIPS = Array.from({ length: INCARNON_MAX_EVOLUTION }, (_, i) => i + 1);

const label = (w: IncarnonWeapon) => sanitizeDisplayName(w.displayName || w.name);

function statusLine(w: IncarnonWeapon, t: Translator): string {
  if (w.status === "unlocked") return w.kind === "genesis" || w.evolution ? incarnonUnlockedLabel(w, t) : t("common.owned");
  if (w.status === "adapter") return t("incarnon.spareAdapters", { count: w.adapterCount });
  if (w.kind === "native") return w.blueprintOwned ? t("incarnon.blueprintOwned") : t("mastery.notOwned");
  return w.circuitWeeks !== undefined ? t("incarnon.nextCircuit", { when: circuitWeeksLabel(w.circuitWeeks, t) }) : t("common.missing");
}

interface Props {
  tr: Translator;
  itemDb: Record<string, ItemDbEntry>;
  inventory: RawInventoryData | null;
}

export default function IncarnonTracker({ tr, itemDb, inventory }: Props) {
  const [filter, setFilter] = usePersisted<IncarnonFilter>("mastery-incarnon-filter", INCARNON_FILTERS, "all");
  const { data: world } = useWorldState();
  const hard = world?.duviriCycle?.choices.find((c) => c.category === "hard")?.choices;
  const weapons = useMemo(() => buildIncarnonWeapons(itemDb, inventory, hard ?? []), [itemDb, inventory, hard]);
  const summary = summarizeIncarnon(weapons);
  const visible = filterIncarnon(weapons, filter);
  const groups = [
    { kind: "genesis", title: tr("incarnon.genesis"), items: visible.filter((w) => w.kind === "genesis") },
    { kind: "native", title: tr("incarnon.native"), items: visible.filter((w) => w.kind === "native") },
  ].filter((g) => g.items.length > 0);

  return (
    <div data-incarnon-tracker>
      <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
        <span data-incarnon-filters>
          <Segmented label={tr("incarnon.filterLabel")} active={filter} onSelect={setFilter} items={INCARNON_FILTERS.map((key) => ({ key, label: tr(FILTER_LABELS[key]) }))} />
        </span>
        <span className="ms-sub" data-incarnon-summary>{tr("incarnon.summary", { unlocked: summary.unlocked, total: summary.total })}</span>
      </div>
      {!inventory && <p className="ms-sub ms-sub--muted mb-3">{tr("app.noInventoryLoaded")}</p>}
      {groups.length === 0 && (
        <Card>
          <EmptyState title={tr("world.noData")} />
        </Card>
      )}
      {groups.map((group) => (
        <div key={group.kind} className="mb-4 grid gap-2" data-incarnon-group={group.kind}>
          <div className="ms-eyebrow">
            {group.title} ({group.items.length})
          </div>
          <div className="ms-grid ms-grid--tight">
            {group.items.map((w) => (
              <button
                key={w.name}
                type="button"
                disabled={!w.uniqueName}
                className="ms-inc-card"
                aria-label={tr("common.openDetailsFor", { name: label(w) })}
                data-incarnon-card={w.name}
                data-incarnon-status={w.status}
                onClick={() => openItemBy({ uniqueName: w.uniqueName })}
              >
                <span className="ms-card-img">
                  <ItemImage src={w.imageUrl || null} fallback={w.fallbackImageUrl} alt="" tr={tr} />
                  {w.adapterCount > 0 && (
                    <span className="ms-card-vault" data-incarnon-adapters={w.adapterCount}>
                      <Badge tone="info" title={tr("incarnon.spareAdapters", { count: w.adapterCount })}>
                        x{w.adapterCount}
                      </Badge>
                    </span>
                  )}
                  <span className={`ms-card-dot ms-card-dot--${w.status}`} aria-hidden="true" />
                </span>
                <span className="ms-card-body">
                  <span className="ms-card-name">{label(w)}</span>
                  <span className="ms-sub ms-sub--muted" data-incarnon-line>{statusLine(w, tr)}</span>
                  {w.status === "adapter" && !w.weaponOwned && <span className="ms-sub text-warning">{tr("incarnon.weaponMissing")}</span>}
                  {w.evolution && (
                    <span className="ms-pips" title={tr("incarnon.evolution", { tier: w.evolution, max: INCARNON_MAX_EVOLUTION })} data-incarnon-evolution={w.evolution}>
                      {EVOLUTION_PIPS.map((pip) => (
                        <span key={pip} className={pip <= w.evolution! ? "is-on" : undefined} />
                      ))}
                    </span>
                  )}
                </span>
              </button>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
