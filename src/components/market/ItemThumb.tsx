import { useState, type FocusEvent, type KeyboardEvent, type PointerEvent } from "react";
import { createPortal } from "react-dom";
import type { Translator } from "../../lib/i18n";
import ItemImage from "../ItemImage";

const PREVIEW = 176;
const GAP = 8;

interface Props {
  name: string;
  /** warframe.market slug; its cached art wins over `src`. */
  slug?: string | null;
  src?: string | null;
  fallback?: string | null;
  tr: Translator;
  /** false where focus lives elsewhere (a combobox list): hover only. */
  focusable?: boolean;
}

/** Hover / keyboard-focus handlers plus the portaled large preview (mounted only then, so the big image loads lazily). */
export function useItemPreview({ name, slug, src, fallback, tr }: Omit<Props, "focusable">) {
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  const show = (el: HTMLElement) => {
    const r = el.getBoundingClientRect();
    // beside the thumb, flipped left when it would leave the window; clamped vertically
    const x = r.right + GAP + PREVIEW + 24 > window.innerWidth ? Math.max(GAP, r.left - GAP - PREVIEW - 24) : r.right + GAP;
    setAt({ x, y: Math.min(Math.max(GAP, r.top + r.height / 2 - (PREVIEW + 48) / 2), window.innerHeight - PREVIEW - 48 - GAP) });
  };
  const bind = {
    onPointerEnter: (e: PointerEvent<HTMLElement>) => show(e.currentTarget),
    onPointerLeave: () => setAt(null),
    onFocus: (e: FocusEvent<HTMLElement>) => e.currentTarget.matches(":focus-visible") && show(e.currentTarget),
    onBlur: () => setAt(null),
    onKeyDown: (e: KeyboardEvent<HTMLElement>) => e.key === "Escape" && setAt(null),
  };
  const node =
    at &&
    createPortal(
      <div className="ui-thumb-preview" role="tooltip" data-item-preview style={{ left: at.x, top: at.y }}>
        <div style={{ width: PREVIEW, height: PREVIEW }}>
          <ItemImage slug={slug} src={src} fallback={fallback} alt={name} tr={tr} cls="h-full w-full" />
        </div>
        <div className="mt-2 text-center text-sm font-semibold">{name}</div>
      </div>,
      document.body,
    );
  return { bind, node };
}

/** Small item art; hover or keyboard focus opens a larger preview. */
export default function ItemThumb({ focusable = true, ...props }: Props) {
  const { name, slug, src, fallback, tr } = props;
  const { bind, node } = useItemPreview(props);
  return (
    <>
      <span className="ui-thumb" data-item-thumb tabIndex={focusable ? 0 : undefined} aria-label={focusable ? name : undefined} {...bind}>
        <ItemImage slug={slug} src={src} fallback={fallback} alt="" tr={tr} cls="h-full w-full" />
      </span>
      {node}
    </>
  );
}
