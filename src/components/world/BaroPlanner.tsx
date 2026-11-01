import { useMemo, useState } from "react";
import type { Translator } from "../../lib/i18n";
import { activeWindow } from "../../lib/format";
import { itemLabel } from "../../lib/itemLabel";
import { buildBaroCatalog, baroBudget } from "../../lib/world/baroPlanner";
import { useInventory } from "../../stores/inventory";
import { useItemDatabase } from "../../stores/items";
import { useBaroHistory, useBaroWishlist } from "../../stores/baro";
import { useWorldState, useClock } from "../../stores/world";
import { usePersisted } from "../../lib/usePersisted";
import { Button, Card, CardBody, CardHeader, Checkbox, Chip, EmptyState, Input, Readout, SearchInput, Select, Table } from "../ui";
import BaroLastSeen from "./BaroLastSeen";
import { openItemBy } from "../../stores/modals";

const FILTERS = ["all", "current", "history", "wishlist"] as const;
type Filter = (typeof FILTERS)[number];
const FILTER_LABELS = { all: "common.all", current: "baro.currentVisit", history: "baro.recordedItems", wishlist: "baro.wishlist" } as const;
const PAGE = 50;

interface Props {
  tr: Translator;
  locale: string;
}

/** Baro Ki'Teer wishlist and basket planner. History is whatever visits this install has seen (no backend archive). */
export default function BaroPlanner({ tr, locale }: Props) {
  const { data: world } = useWorldState();
  const { data: inventory, loaded: inventoryLoaded } = useInventory();
  const db = useItemDatabase();
  const clock = useClock(30_000);
  const current = world?.voidTrader ?? null;
  const history = useBaroHistory(current);
  const { wishlist, setQuantity, full } = useBaroWishlist();
  const [alerts, setAlerts] = usePersisted("baro-wishlist-alerts", ["0", "1"] as const, "0");

  const [filter, setFilter] = useState<Filter>("all");
  const [search, setSearch] = useState("");
  const [unowned, setUnowned] = useState(false);
  const [selectedVisit, setSelectedVisit] = useState("");
  const [addSearch, setAddSearch] = useState("");
  const [limit, setLimit] = useState(PAGE);

  const active = activeWindow(current?.activation, current?.expiry, clock);
  const catalog = useMemo(() => buildBaroCatalog(history, current, inventory, db, wishlist, clock), [history, current, inventory, db, wishlist, clock]);
  const budget = useMemo(() => baroBudget(catalog, inventory), [catalog, inventory]);
  const visits = useMemo(() => [...(history?.visits ?? [])].sort((a, b) => b.activation - a.activation), [history]);
  const visit = visits.find((entry) => entry.id === selectedVisit) ?? null;
  const visitItems = useMemo(() => new Map(visit?.items.map((entry) => [entry.uniqueName, entry]) ?? []), [visit]);
  const coverage = history?.coverageStart ? new Date(history.coverageStart).toLocaleDateString(locale) : null;
  const q = search.trim().toLocaleLowerCase(locale);
  const filtered = catalog
    .filter((row) => {
      if (filter === "current" && !row.available) return false;
      if (filter === "history" && (visit ? !visitItems.has(row.uniqueName) : row.lastSeen === null)) return false;
      if (filter === "wishlist" && row.quantity === 0) return false;
      if (unowned && inventory && row.owned) return false;
      return row.name.toLocaleLowerCase(locale).includes(q);
    })
    .sort((a, b) => Number(b.available) - Number(a.available) || a.name.localeCompare(b.name, locale));
  const shown = filtered.slice(0, limit);
  const wishCount = catalog.reduce((total, row) => total + row.quantity, 0);
  const arrivals = catalog.filter((row) => row.available && row.quantity > 0).length;

  const searchIndex = useMemo(
    () =>
      Object.entries(db)
        .filter(([key]) => key.startsWith("/Lotus/"))
        .map(([key, entry]) => ({ key, label: itemLabel(key, entry), searchName: itemLabel(key, entry).toLocaleLowerCase(locale) })),
    [db, locale],
  );
  const candidates = useMemo(() => {
    const query = addSearch.trim().toLocaleLowerCase(locale);
    if (query.length < 2) return [];
    return searchIndex.filter(({ key, searchName }) => !wishlist[key] && searchName.includes(query)).slice(0, 12);
  }, [addSearch, searchIndex, wishlist, locale]);

  const num = (value: number | null) => (value === null ? tr("common.unknown") : value.toLocaleString(locale));
  const budgetCard = (id: "ducats" | "credits") => {
    const unknown = id === "ducats" ? budget.unknownDucats : budget.unknownCredits;
    const total = id === "ducats" ? budget.totalDucats : budget.totalCredits;
    const bal = budget[id];
    const short = id === "ducats" ? budget.ducatShortfall : budget.creditShortfall;
    return (
      <div className="rounded-[var(--radius-md)] border border-border bg-bg-raised p-3" data-baro-budget={id}>
        <span className="text-xs text-text-muted">{tr(id === "ducats" ? "common.ducats" : "common.credits")}</span>
        <p className="mb-1 mt-2 text-xl font-semibold">
          {unknown ? <span className="text-text-muted">{tr("common.unknown")}</span> : <Readout currency={id === "ducats" ? "ducat" : "credits"} value={total} locale={locale} size="lg" />}
        </p>
        <p className="m-0 text-xs text-text-secondary">{tr("baro.balance", { value: num(bal) })}</p>
        {short !== null && <p className={`mb-0 mt-1 text-xs ${short > 0 ? "text-warning" : "text-success"}`}>{tr("baro.shortfall", { value: short.toLocaleString(locale) })}</p>}
      </div>
    );
  };

  const onQuantity = (uniqueName: string, input: HTMLInputElement) => {
    const parsed = Number(input.value);
    if (!Number.isFinite(parsed)) return;
    const clamped = Math.max(0, Math.min(99, Math.floor(parsed)));
    input.value = String(clamped);
    if (!setQuantity(uniqueName, clamped)) input.value = String(wishlist[uniqueName] ?? 0);
  };
  const selectFilter = (next: Filter) => {
    setFilter(next);
    setLimit(PAGE);
  };

  return (
    <div className="@container flex flex-col gap-4 pb-4" data-baro-planner>
      <div className="flex flex-wrap items-baseline gap-x-3">
        <h2 className="m-0 text-lg font-semibold text-text-heading">{tr("baro.planner")}</h2>
        <span className="text-sm text-text-muted">{active ? tr("baro.currentAt", { location: current?.location || tr("common.unknown") }) : tr("baro.away")}</span>
      </div>
      {history && (
        <p className="m-0 text-xs text-text-muted" data-baro-coverage>
          {coverage ? tr("baro.coverage", { date: coverage }) : tr("baro.coverageEmpty")}
        </p>
      )}
      {inventoryLoaded && !inventory && (
        <p className="m-0 text-sm text-warning" data-baro-no-inventory>
          {tr("baro.inventoryUnavailable")}
        </p>
      )}
      {alerts === "1" && arrivals > 0 && (
        <p className="m-0 rounded-[var(--radius-md)] border border-border bg-accent-glow p-3 text-sm text-text-primary" role="status" data-baro-arrivals>
          {tr("baro.wishlistAvailable", { count: arrivals })}
        </p>
      )}
      <Card>
        <CardHeader title={tr("baro.currentBasket")} meta={tr("baro.wishedQuantity", { count: wishCount })} />
        <CardBody className="flex flex-col gap-3">
          <div className="grid grid-cols-1 gap-3 @xl:grid-cols-2">
            {budgetCard("ducats")}
            {budgetCard("credits")}
          </div>
          <p className="m-0 text-xs text-text-muted">{tr("baro.budgetScope")}</p>
          {budget.unknownCosts > 0 && (
            <p className="m-0 text-sm text-warning" data-baro-unknown-costs>
              {tr("baro.unknownCosts", { count: budget.unknownCosts })}
            </p>
          )}
          {budget.unavailableItems > 0 && (
            <p className="m-0 text-sm text-text-secondary" data-baro-future-wishes>
              {tr("baro.futureWishes", { count: budget.unavailableItems })}
            </p>
          )}
        </CardBody>
      </Card>

      <div className="flex flex-wrap items-center gap-3">
        <div className="flex flex-wrap gap-1.5">
          {FILTERS.map((key) => (
            <Chip key={key} pressed={filter === key} data-baro-filter={key} onClick={() => selectFilter(key)}>
              {tr(FILTER_LABELS[key])}
            </Chip>
          ))}
        </div>
        <SearchInput
          label={tr("common.searchPlaceholder")}
          placeholder={tr("common.searchPlaceholder")}
          className="w-64 max-w-full"
          data-search-focus
          value={search}
          onChange={(e) => {
            setSearch(e.target.value);
            setLimit(PAGE);
          }}
        />
        <Checkbox
          label={tr("baro.unownedOnly")}
          data-baro-unowned
          checked={unowned}
          disabled={!inventory}
          onChange={(e) => {
            setUnowned(e.target.checked);
            setLimit(PAGE);
          }}
        />
      </div>
      {filter === "history" && (
        <div className="flex flex-wrap items-center gap-3">
          <label className="flex items-center gap-2 text-sm text-text-secondary">
            {tr("baro.recordedVisit")}
            <Select
              value={selectedVisit}
              data-baro-visit
              onChange={(e) => {
                setSelectedVisit(e.target.value);
                setLimit(PAGE);
              }}
              options={[["", tr("baro.allRecordedVisits")], ...visits.map((entry) => [entry.id, `${new Date(entry.activation).toLocaleDateString(locale)}${entry.node ? `, ${entry.node}` : ""}`] as const)]}
            />
          </label>
          {visit && <span className="text-xs text-text-muted">{tr("baro.visitEnded", { date: new Date(visit.expiry).toLocaleString(locale) })}</span>}
        </div>
      )}
      <Card>
        {shown.length === 0 ? (
          <EmptyState title={tr("baro.noMatches")} />
        ) : (
          <>
            <Table mode="stack" label={tr("baro.planner")}>
              <thead>
                <tr data-baro-table>
                  <th>{tr("common.item")}</th>
                  <th>{tr("baro.historyStatus")}</th>
                  <th className="ui-num">{tr("common.ducats")}</th>
                  <th className="ui-num">{tr("common.credits")}</th>
                  <th className="ui-num">{tr("baro.wishQuantity")}</th>
                </tr>
              </thead>
              <tbody>
                {shown.flatMap((row, index) => {
                  const recorded = filter === "history" && visit ? visitItems.get(row.uniqueName) : null;
                  const ducats = recorded ? recorded.ducats : row.ducats;
                  const credits = recorded ? recorded.credits : row.credits;
                  const header =
                    filter === "wishlist" && (index === 0 || shown[index - 1].available !== row.available) ? (
                      <tr key={`h-${row.uniqueName}`}>
                        <th colSpan={5} scope="colgroup" className="bg-bg-raised text-left text-xs font-semibold text-text-secondary">
                          {row.available ? tr("baro.availableWishes") : tr("baro.futureWishlist")}
                        </th>
                      </tr>
                    ) : null;
                  return [
                    header,
                    <tr key={row.uniqueName} data-baro-row={row.uniqueName}>
                      <td data-primary>
                        <div className="ui-item">
                          <span className="ui-thumb !h-10 !w-10">{row.imageUrl ? <img src={row.imageUrl} alt="" loading="lazy" /> : row.name[0]}</span>
                          <div className="flex min-w-0 flex-col gap-0.5" data-baro-name>
                            <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-left text-text-primary hover:text-accent hover:underline" onClick={() => openItemBy({ uniqueName: row.uniqueName })}>
                              {row.name}
                            </button>
                            {inventory && row.owned && <span className="text-xs text-success">{tr("common.owned")}</span>}
                          </div>
                        </div>
                      </td>
                      <td data-label={tr("baro.historyStatus")}>
                        <BaroLastSeen uniqueName={row.uniqueName} available={row.available} lastRecorded={row.lastSeen} tr={tr} locale={locale} />
                        {(recorded || (!row.available && row.lastSeen !== null)) && <div className="mt-1 text-xs text-text-muted">{tr("baro.historicalPrice")}</div>}
                      </td>
                      <td className="ui-num" data-label={tr("common.ducats")}>
                        {ducats === null ? num(ducats) : <Readout currency="ducat" value={ducats} locale={locale} />}
                      </td>
                      <td className="ui-num" data-label={tr("common.credits")}>
                        {credits === null ? num(credits) : <Readout currency="credits" value={credits} locale={locale} />}
                      </td>
                      <td className="ui-num" data-label={tr("baro.wishQuantity")}>
                        <Input
                          key={`${row.uniqueName}-${row.quantity}`}
                          type="number"
                          min={0}
                          max={99}
                          step={1}
                          className="!w-16 text-right"
                          defaultValue={row.quantity}
                          aria-label={tr("baro.quantityFor", { name: row.name })}
                          data-baro-quantity={row.uniqueName}
                          onChange={(e) => onQuantity(row.uniqueName, e.currentTarget)}
                        />
                      </td>
                    </tr>,
                  ];
                })}
              </tbody>
            </Table>
            {shown.length < filtered.length && (
              <div className="p-3 text-center">
                <Button size="sm" data-baro-show-more onClick={() => setLimit(limit + PAGE)}>
                  {tr("baro.showMore", { remaining: filtered.length - shown.length })}
                </Button>
              </div>
            )}
          </>
        )}
      </Card>
      <Card>
        <CardHeader title={tr("baro.addWish")} />
        <CardBody className="flex flex-col gap-3">
          <Checkbox label={tr("baro.wishlistAlerts")} checked={alerts === "1"} data-baro-arrival-alerts onChange={(e) => setAlerts(e.target.checked ? "1" : "0")} />
          <p className="m-0 text-xs text-text-muted">{tr("baro.addWishHint")}</p>
          {full && (
            <p className="m-0 text-sm text-warning" role="status" data-baro-wishlist-full>
              {tr("baro.wishlistFull")}
            </p>
          )}
          <SearchInput label={tr("common.searchPlaceholder")} placeholder={tr("common.searchPlaceholder")} className="w-96 max-w-full" data-search-focus value={addSearch} onChange={(e) => setAddSearch(e.target.value)} />
          <div className="grid grid-cols-1 gap-2 @3xl:grid-cols-2" data-baro-add-results>
            {candidates.map(({ key, label }) => (
              <div key={key} className="flex items-center justify-between gap-3 rounded-[var(--radius-md)] border border-border p-2">
                <span className="text-sm text-text-primary">{label}</span>
                <Button size="sm" data-baro-add={key} disabled={full} aria-label={tr("baro.addItem", { name: label })} onClick={() => setQuantity(key, 1)}>
                  {tr("settings.fissureAdd")}
                </Button>
              </div>
            ))}
          </div>
        </CardBody>
      </Card>
    </div>
  );
}
