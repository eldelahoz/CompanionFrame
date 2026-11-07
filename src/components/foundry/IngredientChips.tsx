import { Check, CircleDot, X } from "lucide-react";
import { useLayoutEffect, useRef, useState } from "react";
import ItemImage from "../ItemImage";
import { formatNumber } from "../../lib/format";
import { CHIP_GAP_REM, CHIP_REM, fitChips } from "../../lib/foundryChips";
import type { Translator } from "../../lib/i18n";

export type ChipState = "owned" | "blueprint" | "missing";
export interface Chip {
  key: string;
  uniqueName: string;
  name: string;
  image: string | null;
  have: number;
  need: number;
  state: ChipState;
}

/** One row of ingredient chips: as many as fit the card, then a "+N" chip. Each chip has the same reserved size. */
export default function IngredientChips({ chips, tr }: { chips: readonly Chip[]; tr: Translator }) {
  const row = useRef<HTMLDivElement>(null);
  const [fit, setFit] = useState({ width: 0, rem: 16 });
  const measure = () => {
    const el = row.current;
    if (!el) return;
    const rem = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
    setFit((f) => (f.width === el.clientWidth && f.rem === rem ? f : { width: el.clientWidth, rem }));
  };
  useLayoutEffect(measure); // every render: a zoom or font size change moves the rem without resizing the row
  useLayoutEffect(() => {
    if (!row.current || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(measure);
    ro.observe(row.current);
    return () => ro.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- measure only reads refs and sets state
  }, []);

  const shown = fitChips(chips.length, fit.width, CHIP_REM * fit.rem, CHIP_GAP_REM * fit.rem);
  const hidden = chips.slice(shown);
  const label = (c: Chip) => `${c.name}: ${tr(c.state === "owned" ? "common.owned" : c.state === "blueprint" ? "common.blueprintOwnedNotBuilt" : "common.missing")}`;
  return (
    <div ref={row} className="fd-chips">
      {chips.slice(0, shown).map((c) => (
        <div key={c.key} className="fd-chip" title={label(c)} data-ingredient={c.uniqueName} data-part-state={c.state}>
          <span className="ms-thumb">
            <ItemImage src={c.image} alt={c.name} cls="max-h-full max-w-full object-contain" tr={tr} />
          </span>
          <span className="fd-chip-count tabular-nums">
            {formatNumber(c.have)}/{formatNumber(c.need)}
          </span>
          {c.state === "owned" ? <Check className="text-success" aria-label={tr("common.owned")} /> : c.state === "blueprint" ? <CircleDot className="text-warning" aria-label={tr("common.blueprintOwnedNotBuilt")} /> : <X className="text-danger" aria-label={tr("common.missing")} />}
        </div>
      ))}
      {hidden.length > 0 && (
        <div className="fd-chip fd-chip--more" title={hidden.map(label).join("\n")} data-ingredients-more>
          +{hidden.length}
        </div>
      )}
    </div>
  );
}
