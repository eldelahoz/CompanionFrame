import { X } from "lucide-react";
import type { ReactNode } from "react";
import type { Translator } from "../../lib/i18n";
import ModalShell from "../ModalShell";
import { IconButton } from "./Button";

/**
 * Dialog with title, scrolling body and an optional footer. Centred on desktop; a full-height bottom sheet in
 * narrow or short windows (components.css). Focus trap, Escape and backdrop close come from ModalShell.
 */
export function Modal({
  title,
  sub,
  icon,
  onClose,
  tr,
  footer,
  width = "46rem",
  children,
}: {
  title: string;
  sub?: ReactNode;
  icon?: ReactNode;
  onClose: () => void;
  tr: Translator;
  footer?: ReactNode;
  width?: string;
  children: ReactNode;
}) {
  return (
    <ModalShell ariaLabel={title} tr={tr} onClose={onClose}>
      <div className="detail-panel flex flex-col overflow-hidden" style={{ width }}>
        <header className="ui-modal-head">
          {icon}
          <div className="min-w-0">
            <h2>{title}</h2>
            {sub && <div className="text-xs text-text-muted">{sub}</div>}
          </div>
          <IconButton label={tr("common.close")} onClick={onClose}>
            <X />
          </IconButton>
        </header>
        <div className="ui-modal-body flex-1">{children}</div>
        {footer && <footer className="ui-modal-foot">{footer}</footer>}
      </div>
    </ModalShell>
  );
}
