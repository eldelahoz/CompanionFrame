import type { ReactNode } from "react";

export function ToggleIcon({ collapsed }: { collapsed: boolean }) {
  return (
    <svg className={`world-toggle-icon${collapsed ? " collapsed" : ""}`} viewBox="0 0 12 12" aria-hidden="true">
      <polygon points="2,3 10,3 6,9" fill="currentColor" />
    </svg>
  );
}

interface Props {
  title: string;
  collapsed: boolean;
  onToggle: () => void;
  actions?: ReactNode;
  children: ReactNode;
}

export default function CollapsibleSection({ title, collapsed, onToggle, actions, children }: Props) {
  return (
    <>
      <div className="world-section-header">
        <button type="button" className="world-section-toggle" onClick={onToggle} aria-expanded={!collapsed}>
          <ToggleIcon collapsed={collapsed} />
          <span className="world-section-title">{title}</span>
        </button>
        {actions}
      </div>
      {!collapsed && children}
    </>
  );
}
