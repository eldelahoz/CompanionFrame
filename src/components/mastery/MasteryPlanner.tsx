import { Info, Pin, PinOff } from "lucide-react";
import { useMemo, useState } from "react";
import { buildItemNameIndex } from "../../lib/componentResolution";
import { formatNumber } from "../../lib/format";
import type { Translator } from "../../lib/i18n";
import { itemLabel } from "../../lib/itemLabel";
import { groupPlannedItems, missingOnly, plannerModalTarget, plannerPartState, unfinishedParts, type MasteryPlan, type PlannedItem, type PlannerSort } from "../../lib/masteryPlanner";
import { masteryCraftableCount, masteryPartCounts } from "../../lib/masteryRoadmap";
import { creditsRow } from "../../lib/syndicates/rankup";
import type { ActiveComponent } from "../../stores/modals";
import type { ItemDbEntry, RawInventoryData } from "../../types/inventory";
import ItemImage from "../ItemImage";
import ItemTile from "../ItemTile";
import { Badge, Button, EmptyState, IconButton, Meter, Readout, Segmented, StatStrip } from "../ui";

const MATERIAL_CHIP_LIMIT = 4;

interface Material {
  key: string;
  label: string;
  credits?: boolean;
  owned: number;
  needed: number;
  missing: number;
}

const covered = (r: { owned: number; needed: number; missing: number }) => (r.missing <= 0 ? 100 : Math.floor(Math.max(0, Math.min(1, r.needed > 0 ? r.owned / r.needed : 1)) * 100));
const shortShare = (r: { missing: number; needed: number }) => (r.needed > 0 ? r.missing / r.needed : 0);

interface Props {
  tr: Translator;
  locale: string;
  plan: MasteryPlan;
  db: Record<string, ItemDbEntry>;
  inventory: RawInventoryData | null;
  sort: PlannerSort;
  onSort: (s: PlannerSort) => void;
  onUnpin: (uniqueName: string) => void;
  onOpenItem: (uniqueName: string) => void;
  onOpenComponent: (c: ActiveComponent) => void;
}

export default function MasteryPlanner({ tr, locale, plan, db, inventory, sort, onSort, onUnpin, onOpenItem, onOpenComponent }: Props) {
  const [showCovered, setShowCovered] = useState(false);
  const [openMaterials, setOpenMaterials] = useState<Record<string, boolean>>({});
  const nameIndex = useMemo(() => buildItemNameIndex(db), [db]);
  const groups = useMemo(() => groupPlannedItems(plan.items, sort, (i) => itemLabel(i.uniqueName, i)), [plan.items, sort]);

  const credits = creditsRow(plan.totalCredits, inventory);
  const shortTotals = missingOnly(plan.totals);
  const creditsShown = credits.needed > 0 && (showCovered || credits.missing > 0);
  const rows: Material[] = [
    ...(showCovered ? plan.totals : shortTotals).map((r) => ({ key: r.uniqueName, label: itemLabel(r.uniqueName, r), owned: r.owned, needed: r.needed, missing: r.missing })),
    ...(creditsShown ? [{ key: "credits", label: tr("common.credits"), credits: true, owned: credits.owned, needed: credits.needed, missing: credits.missing }] : []),
  ].sort((a, b) => (a.missing > 0 !== b.missing > 0 ? (a.missing > 0 ? -1 : 1) : a.missing > 0 ? shortShare(b) - shortShare(a) : a.label.localeCompare(b.label)));
  const hidden = plan.totals.length - shortTotals.length + (credits.needed > 0 && credits.missing <= 0 ? 1 : 0);
  const allRows = plan.totals.length + (credits.needed > 0 ? 1 : 0);
  const plannedXp = plan.items.reduce((s, i) => s + i.masteryXpRemaining, 0);
  const num = (n: number) => n.toLocaleString(locale);

  if (plan.items.length === 0) {
    return (
      <div className="ui-card">
        <EmptyState icon={<Pin />} title={tr("mastery.viewPlanned")}>
          {tr("mastery.planner.empty")}
        </EmptyState>
      </div>
    );
  }

  const openRow = (row: { uniqueName: string; name: string; displayName?: string; needed: number; owned: number; missing: number }) => onOpenComponent(plannerModalTarget(row, db, nameIndex));

  const card = (item: PlannedItem) => {
    const listed = unfinishedParts(item.components);
    const parts = masteryPartCounts(item.components.map(plannerPartState));
    const craftable = masteryCraftableCount(item.components, plannerPartState, db);
    const missingMaterials = missingOnly(item.resources);
    const open = openMaterials[item.uniqueName] === true;
    const shown = open ? missingMaterials : missingMaterials.slice(0, MATERIAL_CHIP_LIMIT);
    const label = itemLabel(item.uniqueName, item);
    return (
      <div key={item.uniqueName} className="ms-plan-card ui-card" data-planner-row={item.uniqueName}>
        <div className="ms-plan-head">
          <span className="ms-thumb">
            <ItemImage src={item.imageUrl} alt="" cls="max-h-full max-w-full object-contain" tr={tr} />
          </span>
          <span className="min-w-0">
            <strong className="ms-name">
              <span className="truncate">{label}</span>
            </strong>
            <span className="ms-name">
              {item.masteryXpRemaining > 0 && <Badge tone="accent">{tr("mastery.roadmap.xpAmount", { amount: num(item.masteryXpRemaining) })}</Badge>}
              {!item.hasRecipe ? <span className="ms-sub ms-sub--muted">{tr("mastery.planner.noRecipe")}</span> : item.craftableNow ? <Badge tone="success">{tr("mastery.planner.craftableNow")}</Badge> : <span className="ms-sub ms-sub--muted tabular-nums">{Math.round(item.completeness * 100)}%</span>}
            </span>
          </span>
          <span className="flex gap-1">
            <IconButton label={tr("common.openDetailsFor", { name: label })} onClick={() => onOpenItem(item.uniqueName)}>
              <Info />
            </IconButton>
            <IconButton label={tr("mastery.planner.unpin")} data-mastery-pin={item.uniqueName} onClick={() => onUnpin(item.uniqueName)}>
              <PinOff />
            </IconButton>
          </span>
        </div>

        {item.hasRecipe && (
          <>
            <Meter value={item.completeness * 100} tone={item.craftableNow ? "success" : "accent"} />
            {listed.length > 0 && (
              <div className="ms-block">
                <span className="ms-eyebrow">{tr("mastery.planner.parts")}</span>
                <div className="flex flex-wrap gap-1.5">
                  {listed.map((c) => (
                    <span key={c.uniqueName} className="relative" data-part-state={c.state}>
                      <ItemTile
                        tileKey={c.uniqueName}
                        tone={c.missing > 0 ? "danger" : "neutral"}
                        imageUrl={c.imageUrl}
                        label={itemLabel(c.uniqueName, c)}
                        count={`${formatNumber(c.state === "blueprint" ? c.built : c.owned, locale)}/${formatNumber(c.needed, locale)}`}
                        ariaLabel={tr("mastery.openComponentDetailsAria", { name: itemLabel(c.uniqueName, c) || tr("mastery.componentFallback") })}
                        onOpen={() => openRow(c)}
                        tr={tr}
                      />
                      {c.state === "blueprint" && <span className="ms-dot" role="img" title={tr("common.blueprintOwnedNotBuilt")} aria-label={tr("common.blueprintOwnedNotBuilt")} />}
                    </span>
                  ))}
                </div>
              </div>
            )}
            {missingMaterials.length > 0 && (
              <div className="ms-block">
                <span className="ms-eyebrow">{tr("mastery.planner.itemMaterials")}</span>
                <div className="flex flex-wrap gap-1.5">
                  {shown.map((r) => (
                    <button key={r.uniqueName} type="button" className="ms-chip ms-chip--short" aria-label={tr("mastery.openComponentDetailsAria", { name: itemLabel(r.uniqueName, r) || tr("mastery.componentFallback") })} onClick={() => openRow(r)}>
                      <span className="truncate">{itemLabel(r.uniqueName, r)}</span>
                      <b>
                        {num(r.owned)}/{num(r.needed)}
                      </b>
                    </button>
                  ))}
                  {missingMaterials.length > MATERIAL_CHIP_LIMIT && (
                    <button type="button" className="ms-chip" aria-expanded={open} aria-label={tr("mastery.planner.toggleMaterials", { name: label })} onClick={() => setOpenMaterials({ ...openMaterials, [item.uniqueName]: !open })}>
                      {open ? tr("common.showFewer") : tr("common.moreCount", { count: missingMaterials.length - MATERIAL_CHIP_LIMIT })}
                    </button>
                  )}
                </div>
              </div>
            )}
            <span className="ms-sub ms-sub--muted flex flex-wrap items-center gap-x-3">
              {item.components.length > 0 && <span>{craftable > 0 ? tr("mastery.roadmap.partsOwnedCraftableShort", { owned: parts.built, craftable, total: parts.total }) : tr("mastery.roadmap.partsOwnedShort", { owned: parts.built, total: parts.total })}</span>}
              {item.credits > 0 && <Readout currency="credits" value={item.credits} locale={locale} />}
            </span>
          </>
        )}
      </div>
    );
  };

  const section = (title: string, list: PlannedItem[]) =>
    list.length > 0 && (
      <div className="grid gap-2">
        <span className="ms-eyebrow">{title}</span>
        <div className="ms-grid-2">{list.map(card)}</div>
      </div>
    );

  return (
    <div className="grid gap-3">
      <StatStrip
        className="ms-stats"
        items={[
          { key: "pinned", label: tr("mastery.planner.pinnedTitle"), value: num(plan.items.length) },
          { key: "craftable", label: tr("mastery.planner.craftableNow"), value: num(plan.craftableCount), tone: "success" },
          { key: "xp", label: tr("common.mastery"), value: <Readout currency="mastery" value={plannedXp} locale={locale}><span>{tr("mastery.roadmap.xpAmount", { amount: num(plannedXp) })}</span></Readout> },
          { key: "credits", label: tr("common.credits"), value: <Readout currency="credits" value={plan.totalCredits} locale={locale} /> },
        ]}
      />

      <div className="flex flex-wrap items-center justify-end gap-2">
        <span className="ms-sub ms-sub--muted">{tr("common.sort")}</span>
        <Segmented
          label={tr("common.sort")}
          active={sort}
          onSelect={onSort}
          items={[
            { key: "mastery_xp", label: tr("mastery.sort.masteryXp") },
            { key: "completeness", label: tr("mastery.planner.sortCompleteness") },
            { key: "name", label: tr("common.name") },
          ]}
        />
      </div>

      <div className="ui-card ms-totals" data-planner-totals>
        <div className="flex flex-wrap items-center justify-between gap-2">
          <span className="ms-eyebrow">{tr("mastery.planner.combinedMaterials")}</span>
          <span className="flex flex-wrap items-center gap-2 text-xs text-text-muted">
            {tr("mastery.planner.pinnedCount", { count: plan.items.length })}
            {hidden > 0 && (
              <Button size="sm" onClick={() => setShowCovered(!showCovered)}>
                {showCovered ? tr("common.showFewer") : tr("mastery.planner.showAllMaterials", { count: allRows })}
              </Button>
            )}
          </span>
        </div>
        {rows.length === 0 ? (
          <p className="ms-sub ms-sub--muted m-0">{tr("mastery.planner.noMaterialsNeeded")}</p>
        ) : (
          <div className="ms-materials">
            {rows.map((r) => {
              const pct = covered(r);
              const detail = `${r.label}\n${num(r.owned)} / ${num(r.needed)}${r.missing > 0 ? `\n${tr("common.missing")} ${num(r.missing)}` : ""}`;
              return (
                <div key={r.key} className="ms-material" title={detail} role="img" aria-label={detail}>
                  <span className="truncate">{r.label}</span>
                  <span className="tabular-nums">
                    {num(r.owned)} / {num(r.needed)}
                  </span>
                  <Meter className="ms-material-bar" value={pct} tone={r.missing > 0 ? "warning" : "success"} />
                </div>
              );
            })}
          </div>
        )}
      </div>

      {section(tr("mastery.planner.craftableCount", { count: groups.craftable.length }), groups.craftable)}
      {section(tr("common.inProgress"), groups.remaining)}
    </div>
  );
}
