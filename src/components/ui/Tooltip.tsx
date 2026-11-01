import { useEffect, useState } from "react";

interface Tip {
  text: string;
  x: number;
  y: number;
  side: boolean;
}

/**
 * One tooltip for the whole window: any element with data-tip="…" shows it on hover and on keyboard focus.
 * Rendered position: fixed, so it escapes scrolling and overflow containers. Mount once (App).
 * Elements with data-tip-when="collapsed" only show it while their label is hidden (the sidebar rail).
 */
export function TooltipHost() {
  const [tip, setTip] = useState<Tip | null>(null);
  useEffect(() => {
    const show = (el: HTMLElement | null) => {
      const text = el?.dataset.tip;
      if (!el || !text) return setTip(null);
      if (el.dataset.tipWhen === "collapsed" && !el.closest("[data-collapsed='true']")) return setTip(null);
      const r = el.getBoundingClientRect();
      const side = el.dataset.tipSide === "right";
      setTip({ text, side, x: side ? r.right + 8 : r.left + r.width / 2, y: side ? r.top + r.height / 2 : r.bottom + 6 });
    };
    const over = (e: PointerEvent) => show((e.target as Element | null)?.closest?.<HTMLElement>("[data-tip]") ?? null);
    const focus = (e: FocusEvent) => {
      const el = (e.target as Element | null)?.closest?.<HTMLElement>("[data-tip]") ?? null;
      show(el?.matches(":focus-visible") ? el : null);
    };
    const hide = () => setTip(null);
    document.addEventListener("pointerover", over);
    document.addEventListener("focusin", focus);
    document.addEventListener("scroll", hide, true);
    document.addEventListener("keydown", hide);
    return () => {
      document.removeEventListener("pointerover", over);
      document.removeEventListener("focusin", focus);
      document.removeEventListener("scroll", hide, true);
      document.removeEventListener("keydown", hide);
    };
  }, []);
  if (!tip) return null;
  const style = tip.side ? { left: tip.x, top: tip.y, transform: "translateY(-50%)" } : { left: tip.x, top: Math.min(tip.y, window.innerHeight - 32), transform: "translateX(-50%)" };
  return (
    <div className="ui-tooltip" role="tooltip" style={style} data-tooltip>
      {tip.text}
    </div>
  );
}
