import ItemImage from "./ItemImage";
import type { Translator } from "../lib/i18n";

interface Props {
  /** Value for data-item-tile; e2e locates tiles by it. */
  tileKey: string;
  label: string;
  /** Pre-translated, so the locale change re-renders through the parent. */
  count: string;
  /** Neutral tints the count alone; danger paints the whole tile. */
  tone?: "neutral" | "danger";
  /** Neutral only: false turns the count red. */
  enough?: boolean;
  imageUrl?: string | null;
  /** Item type to expose on data-item-tile-missing, null when it is covered. */
  missingKey?: string | null;
  /** Absent for the credits tile, which has nothing to open. */
  onOpen?: (() => void) | null;
  ariaLabel?: string | null;
  tr: Translator;
}

const TILE_CLASS =
  "item-tile flex w-24 flex-col items-center gap-1 rounded-lg border p-1 text-center transition-colors duration-150";

export default function ItemTile({
  tileKey,
  label,
  count,
  tone = "neutral",
  enough = true,
  imageUrl = null,
  missingKey = null,
  onOpen = null,
  ariaLabel = null,
  tr,
}: Props) {
  const danger = tone === "danger";
  // color-mix has no utility form for the danger tint, so it is spelled as arbitrary values.
  const toneClass = danger
    ? "border-[color-mix(in_oklab,var(--danger)_40%,transparent)] bg-[color-mix(in_oklab,var(--danger)_12%,transparent)] text-[color-mix(in_oklab,var(--danger)_88%,white)]"
    : "border-transparent bg-transparent text-inherit" +
      (onOpen ? " hover:border-accent-dim hover:bg-surface-hover" : "");
  const body = (
    <>
      <span className="flex h-14 w-14 shrink-0 items-center justify-center">
        <ItemImage src={imageUrl} alt={label} cls="max-h-14 max-w-14 object-contain" tr={tr} />
      </span>
      <span className={`line-clamp-2 break-words text-xs leading-tight ${danger ? "text-text-primary" : "text-text-secondary"}`}>
        {label}
      </span>
      <span
        className={`text-xs leading-tight ${danger ? "font-semibold tabular-nums" : enough ? "text-text-secondary" : "text-danger"}`}
        data-item-tile-missing={missingKey ?? undefined}
      >
        {count}
      </span>
    </>
  );

  return onOpen ? (
    <button
      type="button"
      className={`${TILE_CLASS} ${toneClass} cursor-pointer focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent`}
      title={label}
      aria-label={ariaLabel ?? undefined}
      data-item-tile={tileKey}
      onClick={onOpen}
    >
      {body}
    </button>
  ) : (
    <div className={`${TILE_CLASS} ${toneClass}`} title={label} data-item-tile={tileKey}>
      {body}
    </div>
  );
}
