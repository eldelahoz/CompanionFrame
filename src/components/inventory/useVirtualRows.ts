import { elementScroll, useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useLayoutEffect, useState, type RefObject } from "react";

/** Used until a row has been measured; the real size comes from the CSS (fixed card and row heights). */
const DEFAULT_SIZE = { cards: 266, list: 41, relics: 240, mastery: 264, foundry: 200 };
export type RowKind = keyof typeof DEFAULT_SIZE;
/** The last measured row size per kind: a view that comes back estimates exactly, so the scroll offset App restores lands on the same rows. */
const measured = new Map<string, number>();

const scroller = () => document.getElementById("content");

/**
 * Windows `count` equal-height rows over App's main scroll element (`#content`), not a scroll box of its own.
 * `anchor` is the element the first row starts at; its offset inside the scroller is the virtualizer's `scrollMargin`.
 */
export function useVirtualRows(count: number, kind: RowKind, anchor: RefObject<HTMLElement | null>, gap = 0) {
  const [margin, setMargin] = useState(0);
  const measureMargin = () => {
    const sc = scroller();
    const el = anchor.current;
    if (sc && el) setMargin(Math.round(el.getBoundingClientRect().top - sc.getBoundingClientRect().top + sc.scrollTop));
  };
  useLayoutEffect(measureMargin); // every render: a banner or a wrapped toolbar above moves the list
  useEffect(() => {
    const view = anchor.current?.closest(".inventory-view") ?? anchor.current?.parentElement;
    if (!view || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(measureMargin);
    ro.observe(view);
    return () => ro.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- measureMargin only reads refs and sets state
  }, []);
  return useVirtualizer({
    count,
    getScrollElement: scroller,
    estimateSize: () => measured.get(kind) ?? DEFAULT_SIZE[kind],
    measureElement: (el, entry) => {
      const h = Math.round(entry?.borderBoxSize?.[0]?.blockSize ?? el.getBoundingClientRect().height);
      if (h > 0) measured.set(kind, h);
      return h || (measured.get(kind) ?? DEFAULT_SIZE[kind]);
    },
    // The virtualizer writes its own offset (0) to the scroller on mount, which would undo the offset App restores for a returning view.
    scrollToFn: (offset, options, instance) => options.adjustments !== undefined && elementScroll(offset, options, instance),
    scrollMargin: margin,
    overscan: 4,
    gap,
    initialRect: { width: 1280, height: 800 }, // until the scroller is observed (and where there is none, as in tests)
  });
}
