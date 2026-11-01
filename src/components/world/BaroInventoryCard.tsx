import type { TraderItem } from "../../bindings";
import type { Translator } from "../../lib/i18n";
import { Check } from "lucide-react";
import { Readout } from "../ui";
import type { ItemDbEntry } from "../../lib/ipc";

interface Props {
  entry: TraderItem;
  dbEntry: ItemDbEntry | undefined;
  owned?: boolean;
  tr: Translator;
  locale: string;
}

export default function BaroInventoryCard({ entry, dbEntry, owned = false, tr, locale }: Props) {
  const isMod = dbEntry?.category === "Mod";
  const imgUrl = dbEntry?.imageUrl || entry.imageOverride;
  const name = entry.item || tr("common.unknown");
  const title =
    name +
    (entry.ducats ? `, ${tr("world.baro.ducatsShort", { count: entry.ducats })}` : "") +
    (entry.credits ? ` / ${tr("world.baro.creditsShort", { amount: entry.credits.toLocaleString(locale) })}` : "");
  const shape = isMod ? "h-[8.75rem] w-[6.25rem] rounded-[var(--radius-md)] border-0 bg-transparent" : "h-[7.5rem] w-[7.5rem] rounded-[var(--radius-lg)] border bg-bg-raised";
  const border = isMod ? (owned ? "shadow-[0_0_0_2px_var(--success)]" : "") : owned ? "border-success" : "border-border";
  return (
    <div className="flex shrink-0 flex-col items-center gap-1 text-inherit" title={title} data-baro-item={entry.uniqueName}>
      <div className={`relative flex items-center justify-center overflow-hidden ${shape} ${border}`}>
        {imgUrl ? (
          <img className="h-full w-full object-contain" src={imgUrl} alt={name} loading="lazy" />
        ) : (
          <span className="text-3xl font-bold text-text-secondary opacity-40">{name[0]}</span>
        )}
        {owned && (
          <span className="pointer-events-none absolute bottom-1 right-1 flex h-5 w-5 items-center justify-center rounded-full bg-success text-text-on-accent">
            <Check className="h-3.5 w-3.5" aria-label={tr("common.owned")} />
          </span>
        )}
      </div>
      <span className={`truncate text-center text-xs text-text-secondary ${isMod ? "max-w-[6.25rem]" : "max-w-[7.5rem]"}`}>{name}</span>
      <span className="flex flex-wrap justify-center gap-1">
        {entry.ducats ? <Readout currency="ducat" chip value={entry.ducats} locale={locale} /> : null}
        {entry.credits ? <Readout currency="credits" chip value={entry.credits} locale={locale} /> : null}
      </span>
    </div>
  );
}
