import { popoutOpen, type PopoutTarget } from "../../lib/ipc";
import type { Translator } from "../../lib/i18n";

interface Props {
  target: PopoutTarget;
  tr: Translator;
  /** The host owns the look: a view header and the section toolbar size the button differently. */
  className: string;
  iconSize?: number;
  "data-layout-popout"?: string;
}

export default function OpenInWindowButton({ target, tr, className, iconSize = 14, ...rest }: Props) {
  const open = () => void popoutOpen(target).catch((e) => console.warn("[Popout] open failed:", e));
  return (
    <button type="button" aria-label={tr("common.openInWindow")} title={tr("common.openInWindow")} className={className} onClick={open} {...rest}>
      <svg viewBox="0 0 16 16" width={iconSize} height={iconSize} fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M9.5 2.5h4v4" />
        <path d="M13.5 2.5 8 8" />
        <path d="M12.5 9.5V13H3V3.5h3.5" />
      </svg>
    </button>
  );
}
