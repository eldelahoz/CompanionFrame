import { useEffect, useRef, useState } from "react";
import type { MessageKey, Translator } from "../../lib/i18n";
import {
  LEDGER_INPUT_DEBOUNCE_MS,
  RANGE_PRESETS,
  TRADE_ITEM_KINDS,
  UNCATEGORIZED,
  formatPct,
  resolveRangePreset,
  type CostBasisResult,
  type DateRange,
  type ItemRollup,
  type PartnerRollup,
  type PlatFlow,
  type RangePreset,
  type TradeItemKind,
  type TypeRollup,
  type YearComparison,
} from "../../lib/stats/tradeAnalytics";
import { Badge, Card, CardHeader, Chip, Input, Readout, StatStrip, Table, type StatItem } from "../ui";

/** Kind ids are internal, so the panels always render these labels instead. */
const KIND_KEYS: Record<TradeItemKind, MessageKey> = {
  riven: "common.rivens",
  set: "analysis.kind.set",
  prime: "analysis.kind.prime",
  mod: "inventory.tab.mods",
  arcane: "inventory.tab.arcanes",
  relic: "common.relics",
  resource: "nav.resources",
  other: "arbi.type.other",
};

const Empty = ({ text }: { text: string }) => <p className="m-0 py-4 text-center text-sm text-text-muted">{text}</p>;
const plat = (value: number, locale: string, extra: { sign?: boolean; className?: string } = {}) => <Readout currency="platinum" value={Math.round(value)} locale={locale} {...extra} />;

const PRESET_KEYS: Record<RangePreset, MessageKey> = {
  all: "analysis.range.all",
  "30d": "analysis.range.30d",
  "90d": "analysis.range.90d",
  "365d": "analysis.range.365d",
  ytd: "analysis.range.ytd",
  lastYear: "analysis.range.lastYear",
  custom: "common.custom",
};

export function DateRangeBar({ preset, range, onChange, tr }: { preset: RangePreset; range: DateRange; onChange: (p: RangePreset, r: DateRange) => void; tr: Translator }) {
  const [from, setFrom] = useState(range.from ?? "");
  const [to, setTo] = useState(range.to ?? "");
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  // A preset rewrite of `range` snaps both inputs back to the new bounds.
  useEffect(() => {
    setFrom(range.from ?? "");
    setTo(range.to ?? "");
  }, [range.from, range.to]);
  useEffect(() => () => void (timer.current && clearTimeout(timer.current)), []);
  const edit = (nextFrom: string, nextTo: string) => {
    setFrom(nextFrom);
    setTo(nextTo);
    if (timer.current) clearTimeout(timer.current);
    // Editing a bound always means a custom window.
    timer.current = setTimeout(() => onChange("custom", { ...(nextFrom ? { from: nextFrom } : {}), ...(nextTo ? { to: nextTo } : {}) }), LEDGER_INPUT_DEBOUNCE_MS);
  };
  return (
    <div className="flex flex-wrap items-center gap-2" role="group" aria-label={tr("analysis.range.label")} data-analysis-range data-analysis-range-current={preset}>
      {RANGE_PRESETS.map((option) => (
        <span key={option} data-analysis-range-preset={option}>
          <Chip
            pressed={preset === option}
            onClick={() => {
              // A pending bound edit would otherwise land after the preset and undo it.
              if (timer.current) clearTimeout(timer.current);
              onChange(option, resolveRangePreset(option, new Date(), range));
            }}
          >
            {tr(PRESET_KEYS[option])}
          </Chip>
        </span>
      ))}
      <label className="ui-field ui-field--inline" data-analysis-range-from>
        {tr("analysis.range.from")}
        <Input type="date" value={from} onChange={(e) => edit(e.target.value, to)} />
      </label>
      <label className="ui-field ui-field--inline" data-analysis-range-to>
        {tr("analysis.range.to")}
        <Input type="date" value={to} onChange={(e) => edit(from, e.target.value)} />
      </label>
    </div>
  );
}

export function Summary({ flow, basis, best, tr, locale }: { flow: PlatFlow; basis: CostBasisResult; best: ItemRollup | null; tr: Translator; locale: string }) {
  const marginPct = formatPct(basis.estimatedMarginPct, locale);
  const bestName = !best ? "" : best.rank == null ? best.name : `${best.name} (${tr("browse.rankValue", { value: best.rank })})`;
  const items: StatItem[] = [
    { key: "platIn", label: tr("analysis.platIn"), value: plat(flow.platIn, locale, { className: "ui-readout--lg" }), tone: "success", hint: tr("analysis.salesCount", { count: flow.sales }) },
    { key: "platOut", label: tr("analysis.platOut"), value: plat(flow.platOut, locale, { className: "ui-readout--lg" }), tone: "danger", hint: tr("analysis.purchasesCount", { count: flow.purchases }) },
    { key: "net", label: tr("analysis.netPlat"), value: plat(flow.net, locale, { className: "ui-readout--lg" }), tone: flow.net >= 0 ? "success" : "danger", hint: tr("analysis.activeDays", { count: flow.activeDays }) },
    { key: "volume", label: tr("analysis.volume"), value: plat(flow.volume, locale, { className: "ui-readout--lg" }), hint: tr("analysis.tradeCount", { count: flow.events }) },
    { key: "margin", label: tr("analysis.estMargin"), value: plat(basis.estimatedMargin, locale, { className: "ui-readout--lg" }), tone: basis.estimatedMargin >= 0 ? "success" : "danger", hint: marginPct ?? tr("analysis.marginPctUnknown") },
    {
      key: "best",
      label: tr("analysis.bestSeller"),
      value: <span className="text-lg font-semibold">{best ? bestName : tr("common.none")}</span>,
      hint: best ? tr("analysis.bestSellerDetail", { units: best.units, plat: Math.round(best.platinum).toLocaleString(locale) }) : tr("analysis.noSalesYet"),
    },
  ];
  return (
    <div className="flex flex-col gap-2" data-analysis-summary>
      <StatStrip items={items} />
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 px-1 text-xs text-text-muted" data-analysis-estimate-note>
        <Badge tone="warning">{tr("analysis.estimatedBadge")}</Badge>
        <span>{tr("analysis.estimatedNote")}</span>
        {basis.unpricedUnits > 0 && <span data-analysis-unpriced-units>{tr("analysis.unpricedUnits", { count: basis.unpricedUnits, plat: Math.round(basis.unpricedRevenue).toLocaleString(locale) })}</span>}
        {basis.heldUnits > 0 && <span>{tr("analysis.heldUnits", { count: basis.heldUnits, plat: Math.round(basis.heldCost).toLocaleString(locale) })}</span>}
        {basis.swappedUnits > 0 && <span>{tr("analysis.swappedUnits", { count: basis.swappedUnits })}</span>}
      </div>
    </div>
  );
}

export function TopItems({ titleKey, rows, side, tr, locale }: { titleKey: MessageKey; rows: ItemRollup[]; side: "sold" | "bought"; tr: Translator; locale: string }) {
  const max = rows.reduce((m, r) => Math.max(m, r.platinum), 0);
  return (
    <Card className="min-w-0" data-analysis-top-items={side}>
      <CardHeader title={tr(titleKey)} meta={tr("analysis.allocationNote")} />
      <div className="ui-card-body">
        {rows.length === 0 ? (
          <Empty text={tr("analysis.noItems")} />
        ) : (
          <ol className="m-0 flex list-none flex-col gap-3 p-0">
            {rows.map((row) => (
              <li key={row.key} className="flex min-w-0 flex-col gap-1" data-analysis-item-row={row.key}>
                <div className="flex min-w-0 items-baseline justify-between gap-2">
                  <span className="flex min-w-0 items-baseline gap-2">
                    <span className="truncate text-sm" title={row.name}>{row.name}</span>
                    {row.secondary && <span className="truncate text-xs text-text-muted">{row.secondary}</span>}
                    {row.rank != null && <span className="shrink-0 text-xs text-text-muted">{tr("browse.rankValue", { value: row.rank })}</span>}
                  </span>
                  {plat(row.platinum, locale)}
                </div>
                <div className="flex items-center gap-2">
                  <span className="block h-1 min-w-0 flex-1 rounded-full bg-bg-hover">
                    <span className={`block h-full rounded-full ${side === "sold" ? "bg-success" : "bg-danger"}`} style={{ width: `${max > 0 ? Math.max(2, (row.platinum / max) * 100) : 0}%` }} />
                  </span>
                  <span className="shrink-0 text-xs tabular-nums text-text-muted">
                    {tr("analysis.unitsAndAvg", { units: row.units, avg: row.avgUnitPlat == null ? "-" : Math.round(row.avgUnitPlat).toLocaleString(locale) })}
                  </span>
                </div>
              </li>
            ))}
          </ol>
        )}
      </div>
    </Card>
  );
}

export function Partners({ rows, tr, locale }: { rows: PartnerRollup[]; tr: Translator; locale: string }) {
  const max = rows.reduce((m, r) => Math.max(m, r.total), 0);
  return (
    <Card className="min-w-0" data-analysis-partners>
      <CardHeader title={tr("analysis.topPartners")} />
      {rows.length === 0 ? (
        <Empty text={tr("analysis.noPartners")} />
      ) : (
        <Table mode="scroll" label={tr("analysis.topPartners")}>
          <thead>
            <tr>
              <th>{tr("analysis.colPartner")}</th>
              <th className="ui-num">{tr("stats.filterSale")}</th>
              <th className="ui-num">{tr("stats.filterPurchase")}</th>
              <th className="ui-num">{tr("common.total")}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.partner} data-analysis-partner={row.partner || "?"}>
                <td data-primary>
                  <span className="flex min-w-0 flex-col gap-1">
                    <span className="truncate" title={row.partner}>{row.partner || tr("common.unknown")}</span>
                    <span className="h-0.5 rounded-full bg-accent" style={{ width: `${max > 0 ? Math.max(3, (row.total / max) * 100) : 0}%` }} aria-hidden="true" />
                  </span>
                </td>
                <td className="ui-num" data-label={tr("stats.filterSale")}>
                  {plat(row.salesPlat, locale)} <span className="text-text-muted">{tr("analysis.unitsShort", { count: row.sales })}</span>
                </td>
                <td className="ui-num" data-label={tr("stats.filterPurchase")}>
                  {plat(row.purchasesPlat, locale)} <span className="text-text-muted">{tr("analysis.unitsShort", { count: row.purchases })}</span>
                </td>
                <td className="ui-num" data-label={tr("common.total")}>{plat(row.total, locale)}</td>
              </tr>
            ))}
          </tbody>
        </Table>
      )}
    </Card>
  );
}

type YearLine = { key: string; labelKey: MessageKey; current: number; previous: number; pct: number | null; money: boolean };

export function YearCompare({ comparison: c, tr, locale }: { comparison: YearComparison; tr: Translator; locale: string }) {
  const lines: YearLine[] = [
    { key: "platIn", labelKey: "analysis.platIn", current: c.current.platIn, previous: c.previous.platIn, pct: null, money: true },
    { key: "platOut", labelKey: "analysis.platOut", current: c.current.platOut, previous: c.previous.platOut, pct: null, money: true },
    { key: "net", labelKey: "analysis.netPlat", current: c.current.net, previous: c.previous.net, pct: c.netDeltaPct, money: true },
    { key: "volume", labelKey: "analysis.volume", current: c.current.volume, previous: c.previous.volume, pct: c.volumeDeltaPct, money: true },
    { key: "trades", labelKey: "analysis.tradesLabel", current: c.current.events, previous: c.previous.events, pct: null, money: false },
  ];
  const value = (n: number, money: boolean) => (money ? plat(n, locale) : n.toLocaleString(locale));
  return (
    <Card className="min-w-0" data-analysis-year>
      <CardHeader title={tr("analysis.yearCompare")} />
      {!c.hasPrevious && <p className="m-0 px-4 text-xs text-text-muted" data-analysis-year-empty>{tr("analysis.noPreviousYear", { year: c.previousYear })}</p>}
      <Table mode="scroll" label={tr("analysis.yearCompare")}>
        <thead>
          <tr>
            <th />
            <th className="ui-num">
              {c.currentYear} <span className="font-normal text-text-muted">{tr("analysis.yearToDate")}</span>
            </th>
            <th className="ui-num">
              {c.previousYear} <span className="font-normal text-text-muted">{tr("analysis.sameSpan")}</span>
            </th>
            <th className="ui-num">{tr("common.change")}</th>
          </tr>
        </thead>
        <tbody>
          {lines.map((line) => {
            const pct = formatPct(line.pct, locale);
            return (
              <tr key={line.key} data-analysis-year-row={line.key}>
                <td data-primary>{tr(line.labelKey)}</td>
                <td className="ui-num" data-label={String(c.currentYear)}>{value(line.current, line.money)}</td>
                <td className="ui-num text-text-muted" data-label={String(c.previousYear)}>{value(line.previous, line.money)}</td>
                <td className={`ui-num ${line.pct == null ? "text-text-muted" : line.pct >= 0 ? "text-success" : "text-danger"}`} data-label={tr("common.change")}>
                  {pct ?? "-"}
                </td>
              </tr>
            );
          })}
        </tbody>
      </Table>
    </Card>
  );
}

// One colour per builtin kind so the donut, the legend and the table agree; user-made categories cycle PALETTE.
const KIND_COLORS: Record<TradeItemKind, string> = {
  riven: "var(--accent)",
  prime: "var(--warning)",
  set: "var(--info)",
  mod: "var(--success)",
  arcane: "var(--danger)",
  relic: "var(--accent-dim)",
  resource: "var(--text-secondary)",
  other: "var(--text-muted)",
};
const PALETTE = ["var(--accent)", "var(--info)", "var(--success)", "var(--warning)", "var(--danger)", "var(--accent-dim)"];
// r is picked so the circumference is 100, which makes every dash a percentage.
const RADIUS = 15.915494;
const isKind = (kind: string): kind is TradeItemKind => (TRADE_ITEM_KINDS as readonly string[]).includes(kind);

export function TypePanel({ rows, tr, locale }: { rows: TypeRollup[]; tr: Translator; locale: string }) {
  let spare = 0;
  const table = rows.map((row) =>
    isKind(row.kind)
      ? { ...row, label: tr(KIND_KEYS[row.kind]), color: KIND_COLORS[row.kind] }
      : { ...row, label: row.kind === UNCATEGORIZED ? tr("analysis.uncategorized") : row.kind, color: PALETTE[spare++ % PALETTE.length] },
  );
  const total = table.reduce((sum, r) => sum + Math.max(0, r.revenue), 0);
  const pctLabel = (v: number) => `${v.toLocaleString(locale, { maximumFractionDigits: 1 })}%`;
  let used = 0;
  const slices =
    total <= 0
      ? []
      : table
          .filter((r) => r.revenue > 0)
          .map((r) => {
            const pct = (r.revenue / total) * 100;
            // 25 starts the arc at twelve o'clock instead of three.
            const slice = { kind: r.kind, label: r.label, color: r.color, pct, offset: 25 - used };
            used += pct;
            return slice;
          });
  return (
    <Card className="min-w-0" data-analysis-types>
      <CardHeader title={tr("analysis.byType")} />
      {table.length === 0 ? (
        <Empty text={tr("analysis.noCategories")} />
      ) : (
        <div className="flex min-w-0 flex-wrap items-start gap-4 px-4 pb-4">
          {slices.length > 0 && (
            <div className="flex w-40 shrink-0 flex-col gap-2" data-analysis-type-donut>
              <span className="text-xs text-text-muted">{tr("analysis.revenueShare")}</span>
              <svg viewBox="0 0 42 42" className="mx-auto h-28 w-28" aria-hidden="true">
                <circle cx="21" cy="21" r={RADIUS} fill="none" stroke="var(--bg-hover)" strokeWidth="6" />
                {slices.map((s) => (
                  <circle key={s.kind} cx="21" cy="21" r={RADIUS} fill="none" stroke={s.color} strokeWidth="6" strokeDasharray={`${s.pct} ${100 - s.pct}`} strokeDashoffset={s.offset}>
                    <title>{s.label} {pctLabel(s.pct)}</title>
                  </circle>
                ))}
              </svg>
              <ul className="m-0 flex list-none flex-col gap-1 p-0">
                {slices.map((s) => (
                  <li key={s.kind} className="flex min-w-0 items-center gap-2 text-xs">
                    <span className="h-2 w-2 shrink-0 rounded-full" style={{ background: s.color }} aria-hidden="true" />
                    <span className="truncate text-text-secondary">{s.label}</span>
                    <span className="ml-auto shrink-0 tabular-nums text-text-muted">{pctLabel(s.pct)}</span>
                  </li>
                ))}
              </ul>
            </div>
          )}
          <div className="min-w-[16rem] flex-1">
            <Table mode="scroll" label={tr("analysis.byType")}>
              <thead>
                <tr>
                  <th>{tr("common.type")}</th>
                  <th className="ui-num">{tr("analysis.colRevenue")}</th>
                  <th className="ui-num">{tr("analysis.colExpenses")}</th>
                  <th className="ui-num">{tr("analysis.colProfit")}</th>
                  <th className="ui-num">{tr("analysis.colMargin")}</th>
                  <th className="ui-num">{tr("analysis.colSold")}</th>
                  <th className="ui-num">{tr("analysis.colBought")}</th>
                </tr>
              </thead>
              <tbody>
                {table.map((row) => (
                  <tr key={row.kind} data-analysis-type={row.kind}>
                    <td>
                      <span className="flex min-w-0 items-center gap-2">
                        <span className="h-2 w-2 shrink-0 rounded-full" style={{ background: row.color }} aria-hidden="true" />
                        <span className="truncate" title={row.label}>{row.label}</span>
                      </span>
                    </td>
                    <td className="ui-num">{plat(row.revenue, locale)}</td>
                    <td className="ui-num">{plat(row.expenses, locale)}</td>
                    <td className="ui-num">{plat(row.profit, locale, { sign: true })}</td>
                    <td className="ui-num text-text-secondary">{row.marginPct == null ? "-" : pctLabel(row.marginPct)}</td>
                    <td className="ui-num text-text-muted">{row.soldUnits}</td>
                    <td className="ui-num text-text-muted">{row.boughtUnits}</td>
                  </tr>
                ))}
              </tbody>
            </Table>
          </div>
        </div>
      )}
    </Card>
  );
}
