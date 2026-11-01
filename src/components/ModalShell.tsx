import { useEffect, useRef, type ReactNode } from "react";
import type { Translator } from "../lib/i18n";

// Focus can land on body mid-dialog (a clicked button disables itself), so the overlay's own keydown never
// fires then. A window listener catches that Escape; the stack keeps it to the topmost open shell.
const escapeStack: Array<{ current: () => void }> = [];

const FOCUSABLE = ["a[href]", "button:not([disabled])", "input:not([disabled])", "select:not([disabled])", "textarea:not([disabled])", '[tabindex]:not([tabindex="-1"])'].join(",");

interface Props {
  /** Accessible name for the dialog, typically the item/entity name. */
  ariaLabel: string;
  tr: Translator;
  /** Escape, backdrop click. */
  onClose: () => void;
  /** Extra class on the outer overlay (e.g. "comp-overlay"). */
  overlayClass?: string;
  /** Preferred element to focus on open; falls back to the first tabbable. */
  initialFocus?: () => HTMLElement | null;
  children: ReactNode;
}

export default function ModalShell({ ariaLabel, tr, onClose, overlayClass = "", initialFocus, children }: Props) {
  const overlay = useRef<HTMLDivElement>(null);
  const close = useRef(onClose);
  close.current = onClose;

  const focusables = () => Array.from(overlay.current?.querySelectorAll<HTMLElement>(FOCUSABLE) ?? []).filter((el) => !el.hasAttribute("disabled") && el.offsetParent !== null);

  useEffect(() => {
    escapeStack.push(close);
    const previous = document.activeElement as HTMLElement | null;
    const preferred = initialFocus?.() ?? null;
    if (preferred) {
      preferred.focus();
      if (preferred instanceof HTMLInputElement) preferred.select();
    } else (focusables()[0] ?? overlay.current)?.focus();
    const onWindowKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && escapeStack[escapeStack.length - 1] === close) close.current();
    };
    window.addEventListener("keydown", onWindowKey);
    return () => {
      window.removeEventListener("keydown", onWindowKey);
      escapeStack.splice(escapeStack.indexOf(close), 1);
      try {
        previous?.focus();
      } catch {
        /* element may be gone */
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- focus once, on open
  }, []);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      onClose();
      return;
    }
    if (e.key !== "Tab") return;
    const list = focusables();
    if (list.length === 0) {
      e.preventDefault();
      overlay.current?.focus();
      return;
    }
    const active = document.activeElement as HTMLElement | null;
    if (e.shiftKey) {
      if (active === list[0] || !overlay.current?.contains(active)) {
        e.preventDefault();
        list[list.length - 1].focus();
      }
    } else if (active === list[list.length - 1]) {
      e.preventDefault();
      list[0].focus();
    }
  };

  return (
    <div ref={overlay} className={`detail-overlay ${overlayClass}`} role="dialog" aria-modal="true" aria-label={ariaLabel} tabIndex={-1} onKeyDown={onKeyDown}>
      <button type="button" className="detail-backdrop" aria-label={tr("common.closeDialog")} onClick={onClose} />
      {children}
    </div>
  );
}
