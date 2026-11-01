import type { HTMLAttributes, ReactNode } from "react";

export function Card({ className = "", children, as: Tag = "section", ...rest }: HTMLAttributes<HTMLElement> & { as?: "section" | "div" | "article" }) {
  return (
    <Tag className={`ui-card ${className}`} {...rest}>
      {children}
    </Tag>
  );
}

/** Card title row: heading, optional meta text and tool buttons on the right. */
export function CardHeader({ title, meta, tools }: { title: ReactNode; meta?: ReactNode; tools?: ReactNode }) {
  return (
    <header className="ui-card-head">
      <h2>{title}</h2>
      {meta && <span className="ui-card-meta">{meta}</span>}
      {tools && <div className="ui-card-tools">{tools}</div>}
    </header>
  );
}

export const CardBody = ({ className = "", children }: { className?: string; children: ReactNode }) => <div className={`ui-card-body ${className}`}>{children}</div>;

/** Legacy surface used across the ported views: a Card without the header machinery. */
export const Panel = ({ className = "", children }: { className?: string; children: ReactNode }) => <div className={`ui-card ${className}`}>{children}</div>;

export const PanelHeading = ({ title, desc }: { title: string; desc?: string }) => (
  <>
    <h3 className="m-0 text-sm font-semibold text-text-primary">{title}</h3>
    {desc && <p className="mb-3 mt-1 text-xs text-text-muted">{desc}</p>}
  </>
);

/** Page title row with optional subtitle and right-aligned actions. */
export function PageHeader({ title, sub, actions }: { title: ReactNode; sub?: ReactNode; actions?: ReactNode }) {
  return (
    <div className="ui-page-head">
      <h1>{title}</h1>
      {sub && <span className="ui-page-sub">{sub}</span>}
      {actions && <div className="ui-page-actions">{actions}</div>}
    </div>
  );
}
