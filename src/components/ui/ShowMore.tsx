import { useEffect, useRef, useState } from "react";
import type { Translator } from "../../lib/i18n";
import { Button } from "./Button";

/** Rows rendered at first and added per step; the rest of a long list stays out of the DOM until scrolled to. */
const BATCH = 60;

/** How many of `total` rows to render; back to one batch whenever `resetKey` (the filter/sort/tab) changes. */
export function useBatch(resetKey: string, step = BATCH) {
  const [limit, setLimit] = useState(step);
  const [seen, setSeen] = useState(resetKey);
  if (seen !== resetKey) {
    setSeen(resetKey);
    setLimit(step);
  }
  return { limit, more: () => setLimit((n) => n + step) };
}

/** Loads the next batch when scrolled near; the button is the keyboard and no-observer fallback. */
export function ShowMore({ shown, total, onMore, tr, step = BATCH }: { shown: number; total: number; onMore: () => void; tr: Translator; step?: number }) {
  const end = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (shown >= total || !end.current || typeof IntersectionObserver === "undefined") return;
    const io = new IntersectionObserver((e) => e[0]?.isIntersecting && onMore(), { rootMargin: "600px" });
    io.observe(end.current);
    return () => io.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- a new batch re-observes, so a still-visible end loads the next one
  }, [shown, total]);
  return (
    <div ref={end} className="inv-more" data-inventory-more>
      <span>{tr("inventory.showing", { shown, total })}</span>
      {shown < total && (
        <Button size="sm" onClick={onMore}>
          {tr("inventory.showMore", { count: Math.min(step, total - shown) })}
        </Button>
      )}
    </div>
  );
}
