import { Meter } from "./ui";

interface Props {
  label: string;
  done: number;
  total: number;
  inProgress?: number;
  display: "bars" | "rings";
  tone?: "success" | "info" | "accent";
}

const STROKE = { success: "stroke-success", info: "stroke-info", accent: "stroke-accent" } as const;

export default function MasteryBreakdownRow({ label, done, total, inProgress = 0, display, tone = "success" }: Props) {
  const percent = total > 0 ? Math.max(0, Math.min(100, (done / total) * 100)) : 0;
  const progress = total > 0 ? (inProgress / total) * 100 : 0;
  const rings = display === "rings";

  return (
    <div className={rings ? "ms-ring-cell" : "ms-bar-row"} data-mastery-breakdown-row={label} data-display={display}>
      <span className={rings ? "ms-ring-label" : "ms-bar-label"} data-breakdown-category>
        {label}
      </span>
      <span className="ms-bar-count" data-breakdown-count>
        {done}/{total}
        <small className={rings ? "sr-only" : ""}> ({percent.toFixed(1)}%)</small>
      </span>
      <div className={rings ? "order-first" : "ms-bar-track"} data-breakdown-progress>
        {rings ? (
          <svg className="block h-20 w-20" viewBox="0 0 60 60" aria-hidden="true">
            <circle cx="30" cy="30" r="25" fill="none" className="stroke-surface-hover" strokeWidth="4" />
            <circle cx="30" cy="30" r="25" fill="none" className={STROKE[tone]} strokeWidth="4" pathLength="100" strokeDasharray="100" strokeDashoffset={100 - percent} strokeLinecap="round" transform="rotate(-90 30 30)" />
            <text x="30" y="34" textAnchor="middle" className="fill-text-primary" fontSize="11" fontWeight="600">
              {percent.toFixed(1)}%
            </text>
          </svg>
        ) : (
          <Meter value={percent} extra={progress} tone={tone} />
        )}
      </div>
    </div>
  );
}
