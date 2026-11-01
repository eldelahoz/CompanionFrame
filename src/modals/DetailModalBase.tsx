import type { ReactNode } from "react";
import ModalShell from "../components/ModalShell";
import type { Translator } from "../lib/i18n";

interface Props {
  ariaLabel: string;
  tr: Translator;
  onClose: () => void;
  overlayClass?: string;
  containerClass?: string;
  panelClass?: string;
  sideState?: "none" | "component" | "reward";
  /** false renders the children straight into the shell (the component popup brings its own panel). */
  wrapPanel?: boolean;
  /** The panel beside the main one (selected component). */
  sidePanel?: ReactNode;
  children: ReactNode;
}

export default function DetailModalBase({ ariaLabel, tr, onClose, overlayClass, containerClass = "", panelClass = "", sideState = "none", wrapPanel = true, sidePanel, children }: Props) {
  return (
    <ModalShell ariaLabel={ariaLabel} tr={tr} onClose={onClose} overlayClass={overlayClass}>
      {wrapPanel ? (
        <div className={`detail-dual-container ${containerClass} ${sideState === "component" ? "has-comp" : ""} ${sideState === "reward" ? "has-reward" : ""}`}>
          <div className={`detail-panel ${panelClass}`}>{children}</div>
          {sidePanel}
        </div>
      ) : (
        children
      )}
    </ModalShell>
  );
}
