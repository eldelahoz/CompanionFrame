import type { ReactNode } from "react";

export interface StatItem {
  key: string;
  label: ReactNode;
  value: ReactNode;
  hint?: ReactNode;
  tone?: "default" | "success" | "warning" | "danger";
}

/** KPI strip: one card, cells divided by hairlines, reflowing with the container width. */
export function StatStrip({ items, className = "" }: { items: ReadonlyArray<StatItem>; className?: string }) {
  return (
    <div className={`ui-card ui-stats ${className}`} data-summary-strip="grid">
      {items.map((s) => (
        <div key={s.key} className={`ui-stat ${s.tone && s.tone !== "default" ? `ui-stat--${s.tone}` : ""}`} data-summary-item={s.key}>
          <span className="ui-stat-label">{s.label}</span>
          <span className="ui-stat-value">{s.value}</span>
          {s.hint && <span className="ui-stat-hint">{s.hint}</span>}
        </div>
      ))}
    </div>
  );
}

export interface SummaryItem {
  key: string;
  label: string;
  value: string | number;
  tone?: "default" | "success" | "warning" | "danger";
  subtext?: string;
}

/** Legacy name of StatStrip used by the analysis views. */
export const SummaryGrid = ({ items }: { items: SummaryItem[] }) => <StatStrip items={items.map((i) => ({ key: i.key, label: i.label, value: i.value, hint: i.subtext, tone: i.tone }))} />;
