import { useMemo } from "react";
import { NIGHTWAVE_PERMANENT_SHOP } from "../../../config/shared/nightwaveShop";
import type { Translator } from "../../lib/i18n";
import type { WorldState } from "../../lib/ipc";
import { activeWindow } from "../../lib/format";
import { itemLabel } from "../../lib/itemLabel";
import { buildBaroOwnedSet } from "../../lib/world/baroPlanner";
import { openItemBy } from "../../stores/modals";
import { useInventory } from "../../stores/inventory";
import { useItemDatabase } from "../../stores/items";
import { useClock } from "../../stores/world";
import { worldView } from "../../stores/worldView";
import ItemImage from "../ItemImage";
import { Input } from "../ui";

interface Props {
  wd: WorldState;
  tr: Translator;
  locale: string;
}

const REFERENCE = NIGHTWAVE_PERMANENT_SHOP;
const SOURCE = REFERENCE.sources[0];

/** Permanent Nightwave Cred offerings with prices; the rotating stock is not in the worldstate. */
export default function NightwaveShop({ wd, tr, locale }: Props) {
  const db = useItemDatabase();
  const { data: inventory } = useInventory();
  const now = useClock(30_000);
  const [search, setSearch] = worldView.useField("nightwaveSearch");
  const owned = useMemo(() => buildBaroOwnedSet(inventory), [inventory]);
  const season = wd.nightwave;
  const verified = season?.affiliationTag === REFERENCE.seasonAffiliationTag && activeWindow(season.activation, season.expiry, now);

  const tiles = useMemo(() => {
    const needle = search.trim().toLocaleLowerCase(locale);
    return REFERENCE.items
      .map((offer) => {
        const entry = db[offer.uniqueName];
        const stand = "imageOf" in offer && offer.imageOf ? db[offer.imageOf] : undefined;
        const label = entry ? itemLabel(offer.uniqueName, entry) : offer.name.replace(/^[\d,]+\s*x\s+/i, "");
        return { offer, label, entry, imageUrl: entry?.imageUrl ?? stand?.imageUrl ?? null, owned: owned.has(offer.uniqueName) };
      })
      .filter((t) => !needle || `${t.offer.name} ${t.label}`.toLocaleLowerCase(locale).includes(needle));
  }, [db, owned, search, locale]);

  if (!verified) return <p className="m-0 text-sm text-text-secondary" data-nightwave-shop-unavailable>{tr("world.nightwaveShopUnavailable")}</p>;

  return (
    <div data-nightwave-shop>
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <p className="m-0 text-sm font-semibold text-text-primary">
          {REFERENCE.seasonName}, {tr("world.nightwavePermanent")}
        </p>
        <Input
          type="search"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          data-nightwave-search
          aria-label={tr("common.searchPlaceholder")}
          placeholder={tr("common.searchPlaceholder")}
          className="!w-56 max-w-full"
        />
      </div>
      <div className="flex max-h-96 flex-wrap gap-2 overflow-y-auto p-1">
        {tiles.map(({ offer, label, entry, imageUrl, owned: have }) => {
          const cost = offer.creds === null ? tr("world.nightwaveUnknownCost") : tr("world.nightwaveCredCost", { count: offer.creds });
          const qty = offer.quantity > 1 ? ` × ${offer.quantity.toLocaleString(locale)}` : "";
          return (
            <button
              key={offer.name}
              type="button"
              disabled={!entry}
              title={`${label}${qty}, ${cost}`}
              data-nightwave-offer={offer.name}
              onClick={() => openItemBy({ uniqueName: offer.uniqueName })}
              className="flex w-20 shrink-0 cursor-pointer flex-col items-center gap-0.5 border-0 bg-transparent p-0 text-inherit disabled:cursor-default"
            >
              <span className={`relative flex h-20 w-20 items-center justify-center overflow-hidden rounded-[var(--radius-md)] border bg-bg-raised ${have ? "border-success" : "border-border"}`}>
                <ItemImage src={imageUrl} alt={label} cls="h-full w-full object-contain" tr={tr} />
                <span className="absolute left-1 top-1 rounded-[var(--radius-sm)] bg-bg-deep/80 px-1 text-xs font-semibold tabular-nums text-text-primary" data-nightwave-cost>
                  {offer.creds ?? "?"}
                </span>
                {qty && <span className="absolute bottom-1 right-1 rounded-[var(--radius-sm)] bg-bg-deep/80 px-1 text-xs text-text-secondary">{qty.trim()}</span>}
              </span>
              <span className="max-w-20 truncate text-xs text-text-secondary">{label}</span>
            </button>
          );
        })}
      </div>
      <p className="mb-1 mt-3 text-xs text-text-secondary" data-nightwave-rotation-unavailable>{tr("world.nightwaveRotatingUnavailable")}</p>
      <div className="flex flex-wrap items-center gap-3 text-xs text-text-muted">
        <span>{tr("world.nightwaveVerifiedAt", { date: new Date(REFERENCE.updatedAt).toLocaleDateString(locale) })}</span>
        <a className="text-accent hover:underline" href={SOURCE} target="_blank" rel="noreferrer noopener">{tr("common.source")}</a>
      </div>
    </div>
  );
}
