import { CircleAlert, CircleCheck, CircleX, Info, X } from "lucide-react";
import type { CSSProperties, ReactNode } from "react";

type ToastTone = "info" | "success" | "warning" | "error";
const TOAST_ICON = { info: Info, success: CircleCheck, warning: CircleAlert, error: CircleX };

/** Transient notice; ToastHost stacks these bottom-right. */
export function Toast({ tone, title, message, onDismiss, dismissLabel }: { tone: ToastTone; title: ReactNode; message: ReactNode; onDismiss: () => void; dismissLabel: string }) {
  const Icon = TOAST_ICON[tone];
  return (
    <article className={`ui-toast ui-toast--${tone} pointer-events-auto`} role={tone === "error" ? "alert" : "status"}>
      <span className="ui-toast-icon">
        <Icon aria-hidden="true" />
      </span>
      <div className="min-w-0 flex-1">
        <strong>{title}</strong>
        <p>{message}</p>
      </div>
      <button type="button" className="ui-icon-btn -mt-0.5" aria-label={dismissLabel} onClick={onDismiss}>
        <X />
      </button>
    </article>
  );
}

/** Empty or zero-result state that teaches the next step. */
export function EmptyState({ icon, title, children, action }: { icon?: ReactNode; title: ReactNode; children?: ReactNode; action?: ReactNode }) {
  return (
    <div className="ui-empty">
      {icon && <span className="ui-empty-icon">{icon}</span>}
      <h3>{title}</h3>
      {children && <p>{children}</p>}
      {action}
    </div>
  );
}

/** Loading placeholder shaped like the content; never a spinner in the middle of content. */
export const Skeleton = ({ width = "100%", height, className = "" }: { width?: CSSProperties["width"]; height?: CSSProperties["height"]; className?: string }) => (
  <span className={`ui-skeleton ${className}`} style={{ width, height }} aria-hidden="true" />
);
