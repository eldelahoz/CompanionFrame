import { titleCase } from "../../../config/shared/textNormalize";
import { activeWindow, parseIsoDate, timeTo } from "../../lib/format";
import type { MessageKey, Translator } from "../../lib/i18n";
import type { WorldState } from "../../lib/ipc";
import { buildCycleRows, buildFissureRows, buildWorldTimes, type FissureMode } from "../../lib/world/view";
import { withRowKeys } from "../../lib/widgets/registry";
import { settingBoolean, settingNumber, useWidgetSettings } from "../../stores/dashboard";
import CycleRow from "../world/CycleRow";
import { RelicIcon, type RelicTier } from "../icons";
import { Readout } from "../ui";
import WidgetFrame, { WIDGET_BODY, WIDGET_LIST, WIDGET_SUBTITLE } from "./WidgetFrame";

const RELIC_TIERS: ReadonlySet<string> = new Set(["lith", "meso", "neo", "axi", "requiem"]);

interface Props {
  wd: WorldState | null;
  loading: boolean;
  nowMs: number;
  nowCoarseMs: number;
  tr: Translator;
}

export function CyclesWidget({ wd, loading, nowMs, nowCoarseMs, tr }: Props) {
  // Same builders the World tab uses, so a cycle never reads differently here.
  const rows = wd ? buildCycleRows(wd, buildWorldTimes(wd, false, false, nowMs), nowCoarseMs, tr) : [];
  return (
    <WidgetFrame widgetId="widget.cycles" tr={tr} loading={loading && !wd} empty={rows.length === 0} emptyKey={wd ? "world.cycleDataUnavailable" : "world.unavailable"}>
      <div className="flex max-h-[21rem] flex-1 flex-col overflow-y-auto px-4 pb-2">
        {rows.map((row) => (
          <CycleRow key={row.key} name={titleCase(row.key)} planet={row.key} stateLabel={row.stateLabel} stateClass={row.stateClass} nextLabel={row.nextLabel} time={row.time} urgent={row.urgent} tr={tr} />
        ))}
      </div>
    </WidgetFrame>
  );
}

const MODE_LABEL_KEYS: Record<FissureMode, MessageKey> = { all: "common.all", normal: "common.normal", steel: "common.steelPath", railjack: "world.railjack" };

export function FissuresWidget({ wd, loading, nowMs, nowCoarseMs, tr, mode }: Props & { mode: FissureMode }) {
  const settings = useWidgetSettings("widget.fissures");
  const limit = settingNumber(settings, "limit", 5);
  const rows = buildFissureRows(wd?.fissures, mode, nowMs, nowCoarseMs).filter((row) => settingBoolean(settings, row.tierCls, true));
  const shown = withRowKeys(rows.slice(0, limit), (row) => `${row.node}|${row.tier}|${row.expiry}`);
  return (
    <WidgetFrame
      widgetId="widget.fissures"
      tr={tr}
      loading={loading && !wd}
      empty={shown.length === 0}
      emptyKey={wd ? "world.noFissuresAny" : "world.unavailable"}
      overflow={rows.length - shown.length}
      subtitle={<p className={WIDGET_SUBTITLE} data-widget-status>{tr(MODE_LABEL_KEYS[mode])}</p>}
    >
      <ul className={WIDGET_LIST}>
        {shown.map((f) => (
          <li key={f.rowKey} className="text-sm" data-fissure-tier={f.tierCls}>
            {RELIC_TIERS.has(f.tierCls) ? (
              <RelicIcon tier={f.tierCls as RelicTier} label={f.tier} />
            ) : (
              <span className={`shrink-0 rounded-[var(--radius-sm)] px-1.5 text-xs font-semibold world-badge-${f.tierCls}`}>{f.tier}</span>
            )}
            <span className="min-w-0 flex-1 truncate">
              <span className="font-medium text-text-primary">{f.missionType}</span>
              <span className="text-text-muted">, {f.node}</span>
            </span>
            <span className="shrink-0 tabular-nums text-text-secondary">{f.timeStr}</span>
          </li>
        ))}
      </ul>
    </WidgetFrame>
  );
}

export function BaroWidget({ wd, loading, nowMs, nowCoarseMs, tr }: Props) {
  const baro = wd?.voidTrader ?? null;
  const active = activeWindow(baro?.activation, baro?.expiry, nowCoarseMs);
  const time = timeTo(parseIsoDate(active ? baro?.expiry : baro?.activation), nowMs);
  const manifest = active ? (baro?.inventory ?? []) : [];
  const shown = withRowKeys(manifest.slice(0, 4), (e) => e.uniqueName || e.item);
  return (
    <WidgetFrame
      widgetId="widget.baro"
      tr={tr}
      loading={loading && !wd}
      empty={!baro?.activation && !baro?.expiry}
      emptyKey={wd ? "world.noData" : "world.unavailable"}
      overflow={manifest.length - shown.length}
      subtitle={baro?.location ? <p className={WIDGET_SUBTITLE} data-widget-status>{baro.location}</p> : null}
    >
      <p className={`${WIDGET_BODY} m-0 text-sm text-text-primary`}>{tr(active ? "world.baroLeavesIn" : "world.baroArrivesIn", { baro: time })}</p>
      {shown.length > 0 && (
        <ul className={WIDGET_LIST}>
          {shown.map((e) => (
            <li key={e.rowKey} className="text-sm">
              <span className="min-w-0 flex-1 truncate text-text-secondary">{e.item}</span>
              {!!e.ducats && <Readout currency="ducat" value={e.ducats} />}
              {!!e.credits && <Readout currency="credits" value={e.credits} />}
            </li>
          ))}
        </ul>
      )}
    </WidgetFrame>
  );
}
