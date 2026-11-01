import { ChevronDown } from "lucide-react";
import type { ReactNode } from "react";
import { Card } from "../ui";

/** A World section: a Card whose header toggles the body. Tools sit beside the chevron and stay visible when collapsed. */
export default function WorldCard({ title, collapsed = false, onToggle, tools, flush = false, children, ...rest }: { title: ReactNode; collapsed?: boolean; onToggle?: () => void; tools?: ReactNode; flush?: boolean; children: ReactNode } & Record<`data-${string}`, string | undefined>) {
  return (
    <Card {...rest}>
      <header className="ui-card-head">
        {onToggle ? (
          <button type="button" className="flex min-w-0 flex-1 cursor-pointer items-center gap-2 border-0 bg-transparent p-0 text-left" aria-expanded={!collapsed} onClick={onToggle}>
            <ChevronDown className={`h-4 w-4 shrink-0 text-text-muted transition-transform ${collapsed ? "-rotate-90" : ""}`} aria-hidden="true" />
            <h2>{title}</h2>
          </button>
        ) : (
          <h2 className="flex-1">{title}</h2>
        )}
        {tools && <div className="ui-card-tools">{tools}</div>}
      </header>
      {!collapsed && <div className={flush ? "" : "ui-card-body"}>{children}</div>}
    </Card>
  );
}
