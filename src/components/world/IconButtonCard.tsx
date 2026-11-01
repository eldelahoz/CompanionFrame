import ItemImage from "../ItemImage";
import type { Translator } from "../../lib/i18n";

interface Props {
  name: string;
  imageUrl?: string | null;
  owned?: boolean;
  /** Fed to the Helminth: still owned, but marked apart from a plain owned frame. */
  subsumed?: boolean;
  onClick: () => void;
  tr: Translator;
}

/** Clickable item icon with an owned/subsumed frame, used by the circuit, vendor and nightwave strips. */
export default function IconButtonCard({ name, imageUrl, owned = false, subsumed = false, onClick, tr }: Props) {
  const frame = subsumed ? "border-info" : owned ? "border-success" : "border-border";
  return (
    <button
      type="button"
      className="flex w-20 shrink-0 cursor-pointer flex-col items-center gap-0.5 border-0 bg-transparent p-0 text-inherit"
      title={tr("world.viewDetails", { name })}
      data-subsumed={subsumed ? "true" : undefined}
      onClick={onClick}
    >
      <span className={`flex h-20 w-20 items-center justify-center overflow-hidden rounded-[var(--radius-md)] border bg-bg-raised ${frame}`}>
        <ItemImage src={imageUrl} alt={name} cls="h-full w-full object-contain" tr={tr} />
      </span>
      <span className="max-w-20 truncate text-xs text-text-secondary">{name}</span>
    </button>
  );
}
