import { askConfirm } from "../../stores/confirm";
import { useCallback, useEffect, useState } from "react";
import { onInventoryUpdated, statsGetDaily, statsGetSession, statsReset, type DailyStatEntry, type SessionStats } from "../../lib/ipc";
import type { Translator } from "../../lib/i18n";
import { STAT_RESOURCES, formatStatAmount, statResourceLabelKey } from "../../lib/stats/resources";
import { useChartResources } from "../../stores/statsDisplay";
import { ChartNoAxesColumn, ListChecks, RotateCcw } from "lucide-react";
import { Button, Card, EmptyState, Readout, StatStrip, Table, type Currency, type StatItem } from "../ui";
import StatResourcePicker from "./StatResourcePicker";

const RECENT_DAYS = 14;
const CURRENCY: Record<string, Currency> = { plat: "platinum", ducats: "ducat", credits: "credits", endo: "endo" };

/** Today's per-resource deltas from the inventory tracker, plus the recent days; the Stats view hosts it. */
export default function DailyStatsPanel({ tr, locale, refreshKey = 0 }: { tr: Translator; locale: string; refreshKey?: number }) {
  const [session, setSession] = useState<SessionStats | null>(null);
  const [days, setDays] = useState<DailyStatEntry[]>([]);
  const [picking, setPicking] = useState(false);
  const chosen = useChartResources();
  const shown = STAT_RESOURCES.filter((r) => chosen.includes(r.id));

  const load = useCallback(async () => {
    const [s, d] = await Promise.all([statsGetSession(), statsGetDaily()]);
    setSession(s);
    setDays(d);
  }, []);

  useEffect(() => {
    void load();
    // The backend records a payload before announcing it, so a refetch here is never stale.
    const off = onInventoryUpdated(() => void load());
    return () => void off.then((f) => f());
  }, [load, refreshKey]);

  const reset = async () => {
    if (!(await askConfirm(tr("stats.resetConfirm")))) return;
    await statsReset();
    await load();
  };

  const signed = (n: number, f: "plain" | "compact") => `${n > 0 ? "+" : ""}${formatStatAmount(n, f, locale)}`;
  const items: StatItem[] = shown.map((r) => {
    const s = session?.resources[r.id];
    const text = s?.current == null ? "-" : formatStatAmount(s.current, r.format, locale);
    return {
      key: r.id,
      label: tr(statResourceLabelKey(r.id)),
      value: CURRENCY[r.id] && s?.current != null ? <Readout currency={CURRENCY[r.id]} value={s.current} locale={locale}><span>{text}</span></Readout> : text,
      hint: s ? `${signed(s.delta, r.format)} ${tr("stats.today")}` : undefined,
      tone: !s || s.delta === 0 ? "default" : s.delta > 0 ? "success" : "danger",
    };
  });
  const recent = days.slice(-RECENT_DAYS).reverse();

  return (
    <div className="flex flex-col gap-3" data-daily-stats>
      <div className="flex items-center justify-between gap-2">
        <h2 className="m-0 text-md font-semibold text-text-primary">{tr("stats.dailyTitle")}</h2>
        <div className="flex gap-2">
          <Button size="sm" icon={<ListChecks />} onClick={() => setPicking(true)}>{tr("stats.chartResources")}</Button>
          <Button size="sm" variant="danger" icon={<RotateCcw />} onClick={() => void reset()}>{tr("common.reset")}</Button>
        </div>
      </div>
      {session && !session.hasData && days.length === 0 ? (
        <Card>
          <EmptyState icon={<ChartNoAxesColumn />} title={tr("stats.noData")} />
        </Card>
      ) : (
        <>
          <StatStrip items={items} />
          <Card className="overflow-hidden">
            <Table label={tr("stats.dailyTitle")}>
              <thead>
                <tr>
                  <th>{tr("arbi.col.date")}</th>
                  {shown.map((r) => (
                    <th key={r.id} className="ui-num">{tr(statResourceLabelKey(r.id))}</th>
                  ))}
                  <th className="ui-num">{tr("stats.relicsOpened")}</th>
                  <th className="ui-num">{tr("stats.dailyTrades")}</th>
                </tr>
              </thead>
              <tbody data-daily-stats-table-body>
                {recent.length === 0 && (
                  <tr><td className="text-text-muted" colSpan={shown.length + 3}>{tr("stats.noDays")}</td></tr>
                )}
                {recent.map((d) => (
                  <tr key={d.date}>
                    <td>{d.date}</td>
                    {shown.map((r) => {
                      const delta = d.resources[r.id]?.delta;
                      return (
                        <td key={r.id} className={`ui-num ${!delta ? "text-text-muted" : delta > 0 ? "text-success" : "text-danger"}`}>
                          {delta === undefined ? "-" : signed(delta, r.format)}
                        </td>
                      );
                    })}
                    <td className="ui-num">{d.relicsOpened}</td>
                    <td className="ui-num">{d.dailyTrades}</td>
                  </tr>
                ))}
              </tbody>
            </Table>
          </Card>
        </>
      )}
      {picking && <StatResourcePicker tr={tr} onClose={() => setPicking(false)} />}
    </div>
  );
}
