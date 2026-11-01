import type { Translator } from "../lib/i18n";
import { GameIcon } from "./icons";
import { Readout } from "./ui";

type MetricValue = number | string | null | undefined;
type MetricState = "has-value" | "loading" | "no-data";

interface Props {
  platinum?: MetricValue;
  ducats?: MetricValue;
  ratio?: MetricValue;
  state?: MetricState;
  size?: "default" | "compact";
  justify?: "start" | "end";
  tr: Translator;
}

const has = (v: MetricValue) => v !== null && v !== undefined && v !== "";

/** Platinum, ducat and ducats-per-platinum Readout chips for an item or relic. */
export default function MarketMetricStrip({ platinum, ducats, ratio, state = "no-data", size = "default", justify = "start", tr }: Props) {
  const label = (v: MetricValue) => (has(v) ? String(v) : state === "loading" ? "…" : "—");
  const muted = (v: MetricValue) => (has(v) ? "" : "text-text-muted");
  return (
    <div className={`flex flex-wrap items-center gap-1.5 ${justify === "end" ? "justify-end" : "justify-start"} ${size === "compact" ? "text-sm" : ""}`}>
      <span title={tr("common.platinum")}>
        <Readout chip currency="platinum" value={null} className={muted(platinum)}>
          <span>{label(platinum)}</span>
        </Readout>
      </span>
      <span title={tr("common.ducats")}>
        <Readout chip currency="ducat" value={null} className={muted(ducats)}>
          <span>{label(ducats)}</span>
        </Readout>
      </span>
      <span className={`ui-readout ui-readout--chip ${muted(ratio)}`} title={tr("market.ducatsPerPlatinum")}>
        <GameIcon name="ducat" />
        <span aria-hidden="true" className="text-text-muted">/</span>
        <GameIcon name="platinum" />
        <span>{label(ratio)}</span>
      </span>
    </div>
  );
}
