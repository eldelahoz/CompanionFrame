import type { ReactNode } from "react";

interface Props {
  options: Array<{ key: string; label: string; icon?: ReactNode; count?: number }>;
  activeKey: string;
  onSelect: (key: string) => void;
  /** Names the tab list for screen readers. */
  label?: string;
}

/** View-level tabs (underline, accent marker). Same recipe as ui/Tabs; kept for the ported views' data-tour hooks. */
export default function HeaderTabs({ options, activeKey, onSelect, label }: Props) {
  return (
    <div className="ui-tabs border-b-0" role="tablist" aria-label={label}>
      {options.map((o) => (
        <button
          key={o.key}
          type="button"
          role="tab"
          aria-selected={activeKey === o.key}
          data-active={activeKey === o.key || undefined}
          data-tour-tab={o.key}
          className="ui-tab"
          onClick={() => onSelect(o.key)}
        >
          {o.icon}
          {o.label}
          {o.count !== undefined && <span className="ui-tab-count">{o.count}</span>}
        </button>
      ))}
    </div>
  );
}
