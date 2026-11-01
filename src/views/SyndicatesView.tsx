import { ExternalLink } from "lucide-react";
import { useMemo, useState } from "react";
import type { SyndicateKind, SyndicateMeta } from "../../config/shared/syndicateTypes";
import SyndicateEmblem from "../components/SyndicateEmblem";
import { Badge, Button, Card, CardBody, CardHeader, Chip, EmptyState, PageHeader, Progress, Readout, Skeleton } from "../components/ui";
import { SYNDICATE_RANKS } from "../data/syndicateRanks";
import { formatNumber } from "../lib/format";
import type { MessageKey, Translator } from "../lib/i18n";
import { itemLabel } from "../lib/itemLabel";
import { loadGoals, saveGoals, withGoal } from "../lib/syndicates/goals";
import { aggregateNeeds, alignmentConflicts, planSteps, syndicateStatus, type RankUpStep, type SyndicateStatus } from "../lib/syndicates/rankup";
import { buildWikiUrl } from "../lib/wikiUrl";
import { ownershipFor, useInventory } from "../stores/inventory";
import { useItemDatabase } from "../stores/items";
import { openItemBy } from "../stores/modals";

const GROUPS: { kind: SyndicateKind; labelKey: MessageKey }[] = [
  { kind: "normal", labelKey: "syndicates.groupNormal" },
  { kind: "openWorld", labelKey: "syndicates.groupOpenWorld" },
  { kind: "other", labelKey: "syndicates.groupOther" },
];
const NAME_BY_TAG = new Map(SYNDICATE_RANKS.map((m) => [m.tag, m.name]));

interface SyndicateCard {
  meta: SyndicateMeta;
  status: SyndicateStatus;
  percent: number;
  targets: number[];
  goal: number | null;
  steps: RankUpStep[];
}

/** One sacrifice item or credit cost: art, name and "owned of needed". */
function NeedTile({ name, image, count, enough, onOpen, tileKey, missingKey }: { name: string; image: string | null; count: string; enough: boolean; onOpen: () => void; tileKey: string; missingKey?: string | null }) {
  return (
    <button type="button" className="ui-chip !h-auto !justify-start gap-2 py-1 text-left" data-item-tile={tileKey} title={name} onClick={onOpen}>
      <span className="ui-thumb !h-7 !w-7">{image ? <img src={image} alt="" loading="lazy" /> : name[0]}</span>
      <span className="flex min-w-0 flex-col">
        <span className="truncate text-xs text-text-primary">{name}</span>
        <span className={`text-xs tabular-nums ${enough ? "text-text-secondary" : "text-danger"}`} data-item-tile-missing={missingKey ?? undefined}>{count}</span>
      </span>
    </button>
  );
}

export default function SyndicatesView({ tr, locale }: { tr: Translator; locale: string }) {
  const { data: inv, loaded } = useInventory();
  const db = useItemDatabase();
  const [goals, setGoals] = useState(loadGoals);

  // Foundry-committed blueprints are already spent, so they do not count as owned.
  const owned = ownershipFor(inv, db);

  const cards = useMemo(
    () =>
      SYNDICATE_RANKS.map((meta): SyndicateCard => {
        const status = syndicateStatus(inv, meta);
        const span = status.tierEnd - status.tierStart;
        const goal = goals[meta.tag] ?? null;
        const reachable = goal !== null && goal > status.level ? goal : null;
        const top = meta.titles.reduce((max, t) => Math.max(max, t.level), 0);
        const targets: number[] = [];
        for (let level = Math.max(1, status.level + 1); level <= top; level++) targets.push(level);
        return {
          meta,
          status,
          percent: span > 0 ? ((status.standing - status.tierStart) / span) * 100 : 100,
          targets,
          goal: reachable,
          steps: reachable === null ? [] : planSteps(inv, meta, reachable),
        };
      }),
    [inv, goals],
  );
  const plans = useMemo(() => cards.filter((c) => c.goal !== null && c.steps.length > 0).map((c) => ({ meta: c.meta, targetLevel: c.goal ?? c.status.level, steps: c.steps })), [cards]);
  const totals = useMemo(() => aggregateNeeds(plans, inv, owned), [plans, inv, owned]);
  const conflicts = alignmentConflicts(plans.map((p) => p.meta));

  const setGoal = (tag: string, level: number | null) => {
    const next = withGoal(goals, tag, level);
    setGoals(next);
    saveGoals(next);
  };
  const open = (itemType: string, name: string) => (db[itemType] ? openItemBy({ uniqueName: itemType }) : window.open(buildWikiUrl(name), "_blank", "noopener,noreferrer"));
  const fmt = (n: number) => formatNumber(n, locale);
  const ownedOf = (o: number, n: number) => tr("syndicates.ownedOfNeeded", { owned: fmt(o), needed: fmt(n) });

  return (
    <section data-syndicates-view>
      <PageHeader
        title={tr("common.syndicates")}
        sub={tr("syndicates.hint")}
        actions={<Button size="sm" disabled={Object.keys(goals).length === 0} onClick={() => { setGoals({}); saveGoals({}); }}>{tr("syndicates.clearGoals")}</Button>}
      />
      {!inv ? (
        loaded ? (
        <EmptyState title={tr("syndicates.noData")} />
      ) : (
        <Card className="flex flex-col gap-3 p-4" aria-busy="true" aria-label={tr("common.loading")}>
          <Skeleton width="55%" height="1.1rem" />
          <Skeleton width="40%" height="1.1rem" />
          <Skeleton width="70%" height="1.1rem" />
        </Card>
      )
      ) : (
        <div className="@container">
          <div className="grid grid-cols-1 gap-4 @5xl:grid-cols-[minmax(0,1fr)_22rem]">
            <div className="flex min-w-0 flex-col gap-6">
              {GROUPS.map((group) => {
                const rows = cards.filter((c) => c.meta.kind === group.kind);
                return (
                  rows.length > 0 && (
                    <div key={group.kind} className="flex flex-col gap-3">
                      <h2 className="m-0 text-sm font-semibold text-text-secondary">{tr(group.labelKey)}</h2>
                      {rows.map((card) => (
                        <Card key={card.meta.tag} as="article" data-syndicate-card={card.meta.tag}>
                          <CardHeader
                            title={<span className="inline-flex items-center gap-2"><SyndicateEmblem meta={card.meta} tr={tr} />{card.meta.name}</span>}
                            meta={card.status.title ? `${tr("browse.rankValue", { value: String(card.status.level) })}, ${card.status.title}` : tr("syndicates.unranked")}
                            tools={
                              <>
                                {card.status.dailyRemaining !== null && (
                                  <Badge tone={card.status.dailyRemaining > 0 ? "neutral" : "warning"}>
                                    <span data-syndicate-daily>{card.status.dailyRemaining > 0 ? tr("dailies.standingLeft", { amount: fmt(card.status.dailyRemaining) }) : tr("syndicates.dailyCapped")}</span>
                                  </Badge>
                                )}
                                <a className="ui-btn ui-btn--sm" href={buildWikiUrl(card.meta.wikiPage)} target="_blank" rel="noreferrer noopener" title={tr("wiki.openOnWikiTitle")}>
                                  <ExternalLink aria-hidden="true" />
                                  {tr("common.wiki")}
                                </a>
                              </>
                            }
                          />
                          <CardBody className="flex flex-col gap-3">
                            <div className="flex items-center gap-3">
                              <div className="flex-1"><Progress value={card.percent} label={card.meta.name} /></div>
                              <span className="whitespace-nowrap text-xs tabular-nums text-text-muted">
                                {card.status.nextLevel === null ? tr("syndicates.maxRank") : tr("syndicates.toNextRank", { amount: fmt(card.status.standingToNext), level: String(card.status.nextLevel) })}
                              </span>
                            </div>
                            {card.targets.length > 0 && (
                              <div className="flex flex-wrap items-center gap-1.5">
                                <span className="text-xs text-text-muted">{tr("syndicates.target")}</span>
                                {card.targets.map((level) => (
                                  <Chip key={level} pressed={card.goal === level} data-syndicate-goal={level} onClick={() => setGoal(card.meta.tag, card.goal === level ? null : level)}>
                                    {level}
                                  </Chip>
                                ))}
                              </div>
                            )}
                            {card.steps.map((step) => (
                              <div key={step.level} className="flex flex-col gap-2 border-t border-border pt-3" data-syndicate-step={step.level}>
                                <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1 text-xs">
                                  <span className="font-semibold text-text-primary">
                                    {tr("syndicates.stepTitle", { level: String(step.level), title: step.level === 0 ? tr("syndicates.neutralRank") : step.title })}
                                  </span>
                                  <span className="text-text-muted">{tr("dailies.standing", { amount: fmt(step.standingNeeded) })}</span>
                                  {step.initiation && <Badge tone="warning">{tr("syndicates.initiationIncluded")}</Badge>}
                                </div>
                                <div className="flex flex-wrap items-center gap-2">
                                  {step.credits > 0 && (
                                    <Readout currency="credits" chip value={step.credits} locale={locale} className={totals.credits.owned >= step.credits ? "" : "text-danger"} />
                                  )}
                                  {step.items.map((item) => {
                                    const have = owned.get(item.itemType) ?? 0;
                                    const name = db[item.itemType] ? itemLabel(item.itemType, db[item.itemType]) : item.name;
                                    return (
                                      <NeedTile
                                        key={item.itemType}
                                        tileKey={item.itemType}
                                        name={name}
                                        image={db[item.itemType]?.imageUrl ?? null}
                                        count={ownedOf(have, item.count)}
                                        enough={have >= item.count}
                                        missingKey={have < item.count ? item.itemType : null}
                                        onOpen={() => open(item.itemType, item.name)}
                                      />
                                    );
                                  })}
                                </div>
                              </div>
                            ))}
                          </CardBody>
                        </Card>
                      ))}
                    </div>
                  )
                );
              })}
            </div>

            <aside className="self-start @5xl:sticky @5xl:top-2" data-syndicates-totals>
              <Card>
                <CardHeader title={tr("syndicates.totalsTitle")} />
                <CardBody className="flex flex-col gap-3">
                  {plans.length === 0 ? (
                    <p className="m-0 text-sm text-text-muted">{tr("syndicates.noGoals")}</p>
                  ) : (
                    <>
                      {conflicts.map((c) => (
                        <p key={c.a + c.b} className="m-0 text-xs text-warning" data-syndicate-conflict>
                          {tr("syndicates.alignmentWarning", { a: NAME_BY_TAG.get(c.a) ?? c.a, b: NAME_BY_TAG.get(c.b) ?? c.b })}
                        </p>
                      ))}
                      <div className="flex flex-wrap items-center gap-2 text-xs">
                        <Readout currency="credits" chip value={totals.credits.needed} locale={locale} />
                        {totals.credits.missing > 0 && <span className="text-danger">{tr("syndicates.missingAmount", { amount: fmt(totals.credits.missing) })}</span>}
                      </div>
                      {totals.items.length > 0 && (
                        <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
                          {totals.items.map((item) => {
                            const name = db[item.itemType] ? itemLabel(item.itemType, db[item.itemType]) : item.name;
                            return (
                              <li key={item.itemType} className="flex items-center gap-2 text-xs" data-syndicate-total-item>
                                <span className="ui-thumb !h-6 !w-6">{db[item.itemType]?.imageUrl ? <img src={db[item.itemType]?.imageUrl ?? ""} alt="" loading="lazy" /> : name[0]}</span>
                                <span className="flex-1 truncate text-text-secondary" title={name}>{name}</span>
                                <span className={`tabular-nums ${item.missing > 0 ? "text-danger" : "text-success"}`}>{ownedOf(item.owned, item.needed)}</span>
                              </li>
                            );
                          })}
                        </ul>
                      )}
                      <div className="flex flex-col gap-1">
                        {totals.standing.map((pool) => (
                          <div key={pool.bin} className="flex items-baseline gap-2 text-xs" data-syndicate-pool={pool.bin}>
                            <span className="flex-1 truncate text-text-secondary">{pool.tags.map((t) => NAME_BY_TAG.get(t) ?? t).join(", ")}</span>
                            <span className="tabular-nums text-text-muted">{fmt(pool.needed)}</span>
                            <span className="whitespace-nowrap text-text-muted">{pool.daysEstimate === 0 ? tr("syndicates.daysToday") : tr("syndicates.daysEstimate", { days: String(pool.daysEstimate) })}</span>
                          </div>
                        ))}
                      </div>
                    </>
                  )}
                </CardBody>
              </Card>
            </aside>
          </div>
        </div>
      )}
    </section>
  );
}
