import { formatInt } from "../../lib/format";
import { useEffect, useRef, useState } from "react";
import { normCdf, scenarioTable, type ArbiVitusScenarioKey } from "../../../config/shared/arbiMath";
import type { MessageKey, Translator } from "../../lib/i18n";
import type { ArbiRun } from "../../lib/ipc";
import { updateArbiVitus } from "../../stores/arbiRuns";
import { Input, Panel, PanelHeading, Table } from "../ui";

const SCENARIO_KEYS: Record<ArbiVitusScenarioKey, MessageKey> = {
  worstCase: "arbi.vitus.scenario.worstCase",
  unlucky: "arbi.vitus.scenario.unlucky",
  belowAvg: "arbi.vitus.scenario.belowAvg",
  average: "arbi.vitus.scenario.average",
  aboveAvg: "arbi.vitus.scenario.aboveAvg",
  highRoll: "arbi.vitus.scenario.highRoll",
  godRoll: "arbi.vitus.scenario.godRoll",
};

// [percentile above which it applies, colour, label]; first match wins.
const LUCK: Array<[number, string, MessageKey]> = [
  [0.99, "var(--rarity-rare)", "arbi.vitus.scenario.godRoll"],
  [0.9, "var(--success)", "arbi.vitus.scenario.highRoll"],
  [0.75, "var(--success)", "arbi.vitus.scenario.aboveAvg"],
  [0.25, "var(--text-secondary)", "arbi.vitus.scenario.average"],
  [0.1, "var(--warning)", "arbi.vitus.scenario.belowAvg"],
  [0.01, "var(--warning)", "arbi.vitus.scenario.unlucky"],
  [-1, "var(--danger)", "arbi.vitus.scenario.worstCase"],
];

const toAmount = (value: string): number | null => {
  const v = parseInt(value, 10);
  return Number.isFinite(v) && v >= 0 ? v : null;
};

export default function ArbiVitusPanel({ run, tr }: { run: ArbiRun; tr: Translator }) {
  const [input, setInput] = useState(run.vitusActual !== null ? String(run.vitusActual) : "");
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const stats = run.stats;

  // A different run (previous/next) brings its own saved amount.
  useEffect(() => setInput(run.vitusActual !== null ? String(run.vitusActual) : ""), [run.id]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => () => clearTimeout(timer.current), []);

  const rows = stats ? scenarioTable({ mean: stats.expectedVitusMean, std: stats.expectedVitusStd }) : [];
  const actual = toAmount(input);

  let luck: { text: string; color: string } | null = null;
  if (stats && actual !== null && stats.expectedVitusStd > 0) {
    const p = normCdf(actual, stats.expectedVitusMean, stats.expectedVitusStd);
    // `>` for the middle bands and `>=` for the top two.
    const [, color, key] = LUCK.find(([min], i) => (i < 3 ? p >= min : p > min))!;
    const level = tr(key);
    luck = { color, text: p > 0.5 ? tr("arbi.vitus.top", { level, pct: (100 - p * 100).toFixed(1) }) : tr("arbi.vitus.bottom", { level, pct: (p * 100).toFixed(1) }) };
  }

  const onChange = (value: string) => {
    setInput(value);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => void updateArbiVitus(run.id, toAmount(value)), 600);
  };

  return (
    <Panel className="flex flex-col p-5">
      <PanelHeading title={tr("arbi.vitus.title")} desc={tr("arbi.vitus.desc")} />
      <div className="mb-3 flex flex-wrap items-center gap-3 rounded-[var(--radius-md)] border border-border bg-bg-raised px-3 py-2">
        <label className="text-sm text-text-secondary" htmlFor="arbi-actual-vitus">
          {tr("arbi.vitus.actual")}
        </label>
        <Input id="arbi-actual-vitus" type="number" min={0} className="w-24 text-center" value={input} onChange={(e) => onChange(e.target.value)} />
        {luck ? (
          // the luck colour is a status tone (see LUCK), not decoration
          <span className="text-sm font-semibold" style={{ color: luck.color }}>
            {luck.text}
          </span>
        ) : (
          <span className="text-xs text-text-muted">{tr("arbi.vitus.enterAmount")}</span>
        )}
      </div>
      <Table label={tr("arbi.vitus.title")}>
        <thead>
          <tr>
            <th>{tr("common.chance")}</th>
            <th className="ui-num">{tr("arbi.vitus.total")}</th>
            <th>{tr("arbi.vitus.luckLevel")}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.key}>
              <td className="text-text-secondary">{row.prob}</td>
              <td className="ui-num font-semibold text-text-primary">{formatInt(row.total)}</td>
              <td className="text-text-secondary">{tr(SCENARIO_KEYS[row.key])}</td>
            </tr>
          ))}
        </tbody>
      </Table>
    </Panel>
  );
}
