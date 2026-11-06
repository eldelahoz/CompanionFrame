import { Check, Copy, RefreshCw, Search, SearchX, Star, TriangleAlert, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { createViewState } from "../../stores/viewState";
import { ayatanSculptureBySlug } from "../../../config/shared/ayatanEndo";
import { formatWfmAssetUrl, titleFromSlug } from "../../../config/shared/wfm";
import { formatUnitPlatinum, WFM_MOD_VARIANTS } from "../../../config/shared/wfmOrders";
import type { MessageKey, Translator } from "../../lib/i18n";
import { wfmItem, type CatalogItem } from "../../lib/ipc";
import { type CatalogIndex } from "../../lib/wfm/catalog";
import { useMarketArt } from "../../lib/useMarketArt";
import { ayatanEndoInfo, ayatanOrderWhisper } from "../../lib/wfm/ayatanListings";
import { fetchItemOrderBookBySlug, type ItemOrderBook, type OrderBookEntry } from "../../lib/wfm/orderBook";
import { matchesSellerStatus, sellerStatusLabelKey, type SellerStatusFilter } from "../../lib/wfm/orderRows";
import ItemThumb from "./ItemThumb";
import ItemImage from "../ItemImage";
import { Badge, Button, Card, Chip, EmptyState, Field, IconButton, Input, Readout, Segmented, Select, Skeleton, Table, Tabs } from "../ui";
import AyatanEndoBrowse, { statusTone, type BrowseItem } from "./AyatanEndoBrowse";
import MarketBrowseStats from "./MarketBrowseStats";

export interface OrderDraft {
  item: CatalogItem;
  orderType: "sell" | "buy";
  rank: number | null;
  subtype: string | null;
}

interface Props {
  tr: Translator;
  locale: string;
  catalog: CatalogIndex | null;
  loggedIn: boolean;
  onPostOrder: (draft: OrderDraft) => void;
}

type Side = "sell" | "buy";
type RankFilter = "all" | "maxed";
const LIST_ID = "browse-suggestion-list";
const AUTO_REFRESH_MS = 45_000;
const FEEDBACK_TTL_MS = 2_500;
const MAX_SUGGESTIONS = 8;
const MAX_ROWS = 50;
const SAVED_KEY = "wf_market_browse_saved";

// The item DB's public art first; warframe.market's own thumb (Cloudflare-gated) is only the fallback.
const toBrowseItem = (c: CatalogItem, art: string | null): BrowseItem => ({ name: c.name || titleFromSlug(c.slug), slug: c.slug, thumb: art ?? formatWfmAssetUrl(c.thumb || c.icon), gameRef: c.gameRef, maxRank: c.maxRank });

function readSaved(): string[] {
  try {
    const v = JSON.parse(localStorage.getItem(SAVED_KEY) ?? "[]");
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
  } catch {
    return [];
  }
}

export function formatUpdatedLabel(tr: Translator, timestamp: number | null | undefined, nowMs: number): string {
  if (!timestamp || timestamp <= 0) return tr("common.updatedRecently");
  const sec = Math.max(0, Math.floor((nowMs - timestamp) / 1000));
  if (sec < 5) return tr("common.updatedJustNow");
  if (sec < 60) return tr("common.updatedSAgo", { sec });
  const min = Math.floor(sec / 60);
  return min < 60 ? tr("common.updatedMAgo", { min }) : tr("common.updatedHAgo", { hr: Math.floor(min / 60) });
}

export function filterRows(entries: OrderBookEntry[], status: SellerStatusFilter, rank: RankFilter, maxRank: number, min: number | null, max: number | null): OrderBookEntry[] {
  return entries.filter(
    (e) =>
      matchesSellerStatus(e.status, status) &&
      !(rank === "maxed" && maxRank > 0 && (e.rank ?? 0) < maxRank) &&
      !(min != null && min > 0 && e.unitPlatinum < min) &&
      !(max != null && max > 0 && e.unitPlatinum > max),
  );
}

/** The view comes back as the user left it (memory only); the book is refetched, not kept. */
const browse = createViewState({
  query: "",
  selected: null as BrowseItem | null,
  view: "orders" as "orders" | "stats",
  side: "sell" as Side,
  status: "ingame" as SellerStatusFilter,
  rankFilter: "all" as RankFilter,
  subtype: "regular",
  minPrice: null as number | null,
  maxPrice: null as number | null,
  ayatan: false,
});

export default function MarketBrowseView({ tr, locale, catalog, loggedIn, onPostOrder }: Props) {
  const [query, setQuery] = browse.useField("query");
  const [showSuggestions, setShowSuggestions] = useState(false);
  const [active, setActive] = useState(0);
  const [selected, setSelected] = browse.useField("selected");
  const [book, setBook] = useState<ItemOrderBook | null>(null);
  const [loading, setLoading] = useState(false);
  const [failed, setFailed] = useState(false);
  const [noData, setNoData] = useState(false);
  const [reload, setReload] = useState(0);
  const [now, setNow] = useState(Date.now());
  const [feedback, setFeedback] = useState<MessageKey | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const [saved, setSaved] = useState(readSaved);
  const [ayatan, setAyatan] = browse.useField("ayatan");
  const [view, setView] = browse.useField("view");
  const [side, setSide] = browse.useField("side");
  // Default to in-game sellers - the only ones you can actually trade with.
  const [status, setStatus] = browse.useField("status");
  const [rankFilter, setRankFilter] = browse.useField("rankFilter");
  const [subtype, setSubtype] = browse.useField("subtype");
  const [subtypeOptions, setSubtypeOptions] = useState<string[]>([]);
  const [tax, setTax] = useState<number | null>(null);
  const [minPrice, setMinPrice] = browse.useField("minPrice");
  const [maxPrice, setMaxPrice] = browse.useField("maxPrice");
  const [rowLimit, setRowLimit] = useState(MAX_ROWS);
  const feedbackTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const copiedTimer = useRef<ReturnType<typeof setTimeout>>(undefined);

  const art = useMarketArt();
  const items = useMemo(() => (catalog?.items ?? []).map((c) => toBrowseItem(c, art(c.slug))).sort((a, b) => a.name.localeCompare(b.name)), [catalog, art]);
  const suggestions = useMemo(() => (catalog?.search(query, MAX_SUGGESTIONS) ?? []).map((c) => toBrowseItem(c, art(c.slug))), [catalog, query, art]);
  const open = showSuggestions && suggestions.length > 0;
  const sculpture = ayatanSculptureBySlug(selected?.slug);

  const allEntries = [...(book?.sell ?? []), ...(book?.buy ?? [])];
  const ranked = (selected?.maxRank ?? 0) > 0 || allEntries.some((e) => e.rank != null);
  const maxRank = selected?.maxRank ?? allEntries.reduce((m, e) => Math.max(m, e.rank ?? 0), 0);
  // Derived from the catalog only: tying it to the loaded book would refetch every time the book is cleared.
  const fetchRank = rankFilter === "maxed" && (selected?.maxRank ?? 0) > 0 ? selected!.maxRank : null;
  const filtered = filterRows((side === "sell" ? book?.sell : book?.buy) ?? [], status, rankFilter, maxRank, minPrice, maxPrice);
  const rows = filtered.slice(0, rowLimit);

  const say = (key: MessageKey) => {
    setFeedback(key);
    clearTimeout(feedbackTimer.current);
    feedbackTimer.current = setTimeout(() => setFeedback(null), FEEDBACK_TTL_MS);
  };
  useEffect(
    () => () => {
      clearTimeout(feedbackTimer.current);
      clearTimeout(copiedTimer.current);
    },
    [],
  );

  // Order book: load on selection / filter change, retry once on a transient error, then refresh on a timer.
  useEffect(() => {
    if (!selected) return;
    let live = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    setBook(null);
    setFailed(false);
    setNoData(false);
    setLoading(true);
    void (async () => {
      let result = await fetchItemOrderBookBySlug(selected.slug, { rank: fetchRank, subtype });
      if (live && result.status === "error") result = await fetchItemOrderBookBySlug(selected.slug, { rank: fetchRank, subtype });
      if (!live) return;
      setLoading(false);
      timer = setTimeout(() => setReload((n) => n + 1), AUTO_REFRESH_MS);
      if (result.status === "ok") {
        setBook(result.data);
        setNow(Date.now());
      } else if (result.status === "not_found") setNoData(true);
      else setFailed(true);
    })();
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [selected, fetchRank, subtype, reload]);

  useEffect(() => {
    if (!book) return;
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [book]);

  // Trading tax and mod variants come with the item details; a failure just hides them.
  useEffect(() => {
    if (!selected) return;
    let live = true;
    setTax(null);
    setSubtypeOptions([]);
    wfmItem(selected.slug).then(
      (item) => {
        if (!live || !item) return;
        setTax(item.tradingTax != null && item.tradingTax >= 0 ? item.tradingTax : null);
        if (WFM_MOD_VARIANTS.every((v) => item.subtypes.includes(v))) setSubtypeOptions([...WFM_MOD_VARIANTS]);
      },
      () => {},
    );
    return () => {
      live = false;
    };
  }, [selected]);

  const pick = (item: BrowseItem) => {
    setAyatan(false);
    setSelected(item);
    setQuery(item.name);
    setShowSuggestions(false);
    setRankFilter("all");
    setSubtype("regular");
    setView("orders");
    setRowLimit(MAX_ROWS);
    setReload((n) => n + 1);
  };
  const openSculpture = (slug: string) => pick(items.find((i) => i.slug === slug) ?? { name: titleFromSlug(slug), slug, thumb: null, gameRef: null, maxRank: null });
  const toggleAyatan = () => {
    if (!ayatan) {
      setSelected(null);
      setBook(null);
      setLoading(false);
      setQuery("");
    }
    setAyatan((a) => !a);
  };
  const choose = <T,>(set: (v: T) => void, v: T) => (set(v), setRowLimit(MAX_ROWS));

  const isSaved = !!selected && saved.some((n) => n.toLowerCase() === selected.name.toLowerCase());
  const persist = (next: string[]) => {
    setSaved(next);
    try {
      localStorage.setItem(SAVED_KEY, JSON.stringify(next));
    } catch {
      // the list just does not survive a restart
    }
  };
  const applySaved = (name: string) => {
    const hit = items.find((i) => i.name.toLowerCase() === name.toLowerCase());
    if (hit) pick(hit);
    else setQuery(name);
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    // While an IME composes, these keys belong to the candidate window.
    if (e.nativeEvent.isComposing) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (suggestions.length === 0) return;
      setShowSuggestions(true);
      setActive((a) => (a + (e.key === "ArrowDown" ? 1 : -1) + suggestions.length) % suggestions.length);
    } else if ((e.key === "Enter" || (e.key === "Tab" && !e.shiftKey)) && open) {
      // warframe.market opens the highlighted row on Tab.
      e.preventDefault();
      pick(suggestions[active] ?? suggestions[0]);
    } else if (e.key === "Escape") setShowSuggestions(false);
  };

  const whisper = (entry: OrderBookEntry): string => {
    if (!selected) return "";
    const rankSuffix = ranked && entry.rank != null ? ` (Rank ${entry.rank})` : "";
    const variant = subtype === "atragraph" ? " (Atragraph)" : "";
    return ayatanOrderWhisper(tr, side, entry, `${selected.name}${variant}${rankSuffix}`, sculpture ?? null);
  };
  const copy = async (entry: OrderBookEntry, key: string) => {
    try {
      if (!navigator.clipboard?.writeText) return say("common.clipboardUnavailableInThisEnvironment");
      await navigator.clipboard.writeText(whisper(entry));
      setCopied(key);
      clearTimeout(copiedTimer.current);
      copiedTimer.current = setTimeout(() => setCopied(null), 1600);
      say("browse.whisperCopied");
    } catch {
      say("common.failedToCopyWhisper");
    }
  };

  const post = (orderType: Side) => {
    const item = selected && catalog?.bySlug(selected.slug);
    if (!loggedIn) return say("browse.signInRequired");
    if (!item) return say("browse.orderPrepFailed");
    onPostOrder({ item, orderType, rank: fetchRank, subtype: subtypeOptions.length > 0 ? subtype : null });
  };

  const seg = <T extends string>(label: string, current: T, options: ReadonlyArray<readonly [T, string]>, set: (v: T) => void) => (
    <Field label={label}>
      <Segmented label={label} active={current} onSelect={set} items={options.map(([key, text]) => ({ key, label: text }))} />
    </Field>
  );

  return (
    <div className="flex flex-col gap-4" data-tour="market-browse">
      <div className="mx-auto flex w-[min(640px,100%)] items-center gap-2">
        <div className="relative min-w-0 flex-1">
          <div className="ui-search w-full">
            <Search aria-hidden="true" />
            <input
              className="ui-input"
              type="text"
              placeholder={tr("browse.searchPlaceholder")}
              value={query}
              onChange={(e) => (setQuery(e.target.value), setShowSuggestions(true), setActive(0))}
              onFocus={() => setShowSuggestions(true)}
              onKeyDown={onKeyDown}
              // mousedown on a suggestion fires before this, so picks still land
              onBlur={() => setTimeout(() => setShowSuggestions(false), 120)}
              role="combobox"
              aria-expanded={open}
              aria-controls={LIST_ID}
              aria-activedescendant={open ? `${LIST_ID}-${active}` : undefined}
              aria-label={tr("browse.searchPlaceholder")}
              data-search-focus
            />
          </div>
          {open && (
            <div className="ui-card absolute inset-x-0 top-full z-30 mt-1 max-h-[22rem] overflow-y-auto p-1" style={{ boxShadow: "var(--shadow-2)" }} role="listbox" id={LIST_ID}>
              {suggestions.map((s, i) => (
                <button
                  key={s.slug}
                  type="button"
                  className={`flex w-full cursor-pointer items-center gap-3 rounded-[var(--radius-md)] border-0 px-2 py-2 text-left text-sm text-text-primary hover:bg-bg-hover ${i === active ? "bg-bg-hover" : "bg-transparent"}`}
                  role="option"
                  id={`${LIST_ID}-${i}`}
                  aria-selected={i === active}
                  onMouseEnter={() => setActive(i)}
                  onMouseDown={(e) => (e.preventDefault(), pick(s))}
                >
                  <ItemThumb name={s.name} slug={s.slug} src={s.thumb} tr={tr} focusable={false} />
                  <span className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap">{s.name}</span>
                </button>
              ))}
            </div>
          )}
        </div>
        <IconButton label={isSaved ? tr("browse.alreadySaved") : tr("browse.saveItem")} title={isSaved ? tr("browse.alreadySaved") : tr("browse.saveItem")} aria-pressed={isSaved} disabled={!selected} onClick={() => selected && !isSaved && persist([...saved, selected.name])}>
          <Star />
        </IconButton>
      </div>

      <div className="mx-auto flex w-[min(640px,100%)] flex-wrap items-center gap-2">
        <Chip pressed={ayatan} title={tr("browse.ayatan.hint")} data-browse-ayatan-preset onClick={toggleAyatan}>
          {tr("browse.ayatan.preset")}
        </Chip>
        {saved.length > 0 && (
          <>
            <span className="ml-2 text-xs text-text-muted">{tr("common.saved")}</span>
            {saved.map((name) => (
              <span key={name} className="inline-flex items-center gap-1">
                <Chip pressed={selected?.name.toLowerCase() === name.toLowerCase()} title={tr("browse.openItem")} onClick={() => applySaved(name)}>
                  {name}
                </Chip>
                <IconButton label={tr("browse.removeSavedItem")} title={tr("browse.removeSavedItem")} onClick={() => persist(saved.filter((n) => n !== name))}>
                  <X />
                </IconButton>
              </span>
            ))}
          </>
        )}
      </div>

      {feedback && (
        <div className="mx-auto w-[min(640px,100%)]" role="status">
          <Badge tone="accent" className="w-full justify-center py-2">
            {tr(feedback)}
          </Badge>
        </div>
      )}

      {ayatan ? (
        <AyatanEndoBrowse catalog={items} onOpenItem={openSculpture} onFeedback={say} tr={tr} locale={locale} />
      ) : !selected ? (
        <Card className="mx-auto w-[min(640px,100%)]">
          <EmptyState icon={<Search />} title={tr("browse.emptyHint")} />
        </Card>
      ) : (
        <>
          <Card className="flex flex-wrap items-center gap-4 p-4">
            <span className="ui-thumb ui-thumb--lg">
              <ItemImage src={selected.thumb} alt={selected.name} cls="max-h-full max-w-full" tr={tr} />
            </span>
            <div className="min-w-0 flex-1">
              <h3 className="m-0 text-xl font-semibold">{selected.name}</h3>
              <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-text-secondary">
                {tax != null && <Readout currency="credits" label={tr("browse.tradingTax")} value={tax} locale={locale} />}
                {ranked && <span>{tr("browse.maxRank", { rank: maxRank })}</span>}
                <a className="link-btn" href={`https://warframe.market/items/${selected.slug}`} target="_blank" rel="noreferrer noopener">
                  {tr("common.openOnWarframeMarket")}
                </a>
                {book && !loading && <span className="text-text-muted">{formatUpdatedLabel(tr, book.timestamp, now)}</span>}
              </div>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <Button variant="primary" size="sm" onClick={() => post("sell")}>
                {tr("common.postWts")}
              </Button>
              <Button size="sm" onClick={() => post("buy")}>
                {tr("common.postWtb")}
              </Button>
              <IconButton label={tr("common.refresh")} onClick={() => setReload((n) => n + 1)}>
                <RefreshCw />
              </IconButton>
            </div>
          </Card>

          <Tabs
            label={tr("browse.tabOrders")}
            active={view}
            onSelect={(k) => (k === "stats" && subtype === "atragraph" ? undefined : setView(k))}
            items={[
              { key: "orders", label: tr("browse.tabOrders") },
              { key: "stats", label: tr("browse.tabStatistics") },
            ]}
          />

          {view === "stats" && subtype !== "atragraph" ? (
            <MarketBrowseStats slug={selected.slug} tr={tr} locale={locale} />
          ) : (
            <>
              <div className="flex flex-wrap items-end gap-x-4 gap-y-3">
                {subtypeOptions.length > 0 && (
                  <Field label={tr("orderModal.variant")}>
                    <Select
                      id="browse-variant"
                      data-browse-variant
                      value={subtype}
                      options={subtypeOptions.map((o) => [o, o === "regular" ? tr("orderModal.regular") : "Atragraph"] as const)}
                      onChange={(e) => (setSubtype(e.target.value), setView("orders"))}
                    />
                  </Field>
                )}
                {seg<Side>(tr("common.orderType"), side, [["sell", tr("browse.sellers")], ["buy", tr("browse.buyers")]], (v) => choose(setSide, v))}
                {seg<SellerStatusFilter>(tr("browse.status"), status, [["ingame", tr("common.inGame")], ["onsite", tr("browse.onSite")], ["all", tr("common.all")]], (v) => choose(setStatus, v))}
                {ranked && seg<RankFilter>(tr("common.rank"), rankFilter, [["all", tr("common.all")], ["maxed", tr("browse.maxed")]], (v) => choose(setRankFilter, v))}
                <Field label={tr("common.price")}>
                  <span className="flex items-center gap-2">
                    <Input className="w-24" type="number" min="0" placeholder={tr("common.min")} aria-label={tr("common.min")} value={minPrice ?? ""} onChange={(e) => choose(setMinPrice, e.target.value === "" ? null : Number(e.target.value))} />
                    <span className="text-text-muted">-</span>
                    <Input className="w-24" type="number" min="0" placeholder={tr("common.max")} aria-label={tr("common.max")} value={maxPrice ?? ""} onChange={(e) => choose(setMaxPrice, e.target.value === "" ? null : Number(e.target.value))} />
                  </span>
                </Field>
              </div>

              {loading ? (
                <Card className="flex flex-col gap-3 p-4" aria-busy="true" aria-label={tr("common.loadingListings")}>
                  <Skeleton width="55%" height="1.1rem" />
                  <Skeleton width="40%" height="1.1rem" />
                  <Skeleton width="48%" height="1.1rem" />
                </Card>
              ) : failed ? (
                <Card>
                  <div role="alert">
                    <EmptyState icon={<TriangleAlert />} title={tr("common.failedToLoadListingsTryAgain")} />
                  </div>
                </Card>
              ) : noData || !book ? (
                <Card>
                  <EmptyState icon={<SearchX />} title={tr("browse.noListings")} />
                </Card>
              ) : rows.length === 0 ? (
                <Card>
                  <EmptyState icon={<SearchX />} title={tr(side === "sell" ? "browse.noOrdersSell" : "browse.noOrdersBuy")} />
                </Card>
              ) : (
                <>
                  <Card className="overflow-hidden">
                    <Table mode="scroll" label={tr("browse.tabOrders")}>
                      <thead>
                        <tr>
                          <th>{tr("browse.col.qty")}</th>
                          <th>{tr("browse.col.user")}</th>
                          <th>{tr("browse.status")}</th>
                          {ranked && <th className="ui-num">{tr("common.rank")}</th>}
                          <th className="ui-num">{tr("browse.col.unitPrice")}</th>
                          {sculpture && <th className="ui-num">{tr("browse.col.endoPerPlat")}</th>}
                          <th className="ui-num">{tr(side === "sell" ? "browse.col.buy" : "browse.col.sell")}</th>
                        </tr>
                      </thead>
                      <tbody data-browse-table>
                        {rows.map((e, i) => {
                          const key = `${e.userName}:${e.platinum}:${i}`;
                          const endo = sculpture ? ayatanEndoInfo(sculpture, e) : null;
                          return (
                            <tr key={key}>
                              <td className="text-text-secondary">x{e.quantity}</td>
                              <td>
                                <div className="flex items-center gap-2">
                                  <span className="ui-thumb ui-thumb--sm relative">
                                    {e.userName.slice(0, 1).toUpperCase()}
                                    {e.avatar && <img className="absolute inset-0 h-full w-full object-cover" src={formatWfmAssetUrl(e.avatar) ?? undefined} alt="" loading="lazy" onError={(ev) => (ev.currentTarget.style.display = "none")} />}
                                  </span>
                                  <a className="link-btn font-semibold" href={`https://warframe.market/profile/${encodeURIComponent(e.userName)}`} target="_blank" rel="noreferrer noopener" title={tr("browse.openProfile")}>
                                    {e.userName}
                                  </a>
                                </div>
                              </td>
                              <td>
                                <Badge tone={statusTone(e.status)}>{tr(sellerStatusLabelKey(e.status))}</Badge>
                              </td>
                              {ranked && <td className="ui-num text-text-secondary">{e.rank != null ? `R${e.rank}` : "-"}</td>}
                              <td className="ui-num" data-browse-unit-plat={formatUnitPlatinum(e.unitPlatinum)} title={e.perTrade > 1 ? tr("orderbook.perTradeTitle", { platinum: e.platinum, count: e.perTrade }) : undefined}>
                                <Readout currency="platinum" value={null} chip>
                                  <b>{formatUnitPlatinum(e.unitPlatinum)}</b>
                                </Readout>
                                {e.perTrade > 1 && (
                                  <span className="block text-xs font-normal text-text-muted" data-browse-per-trade={e.perTrade}>
                                    {tr("orderbook.perTrade", { count: e.perTrade, platinum: e.platinum })}
                                  </span>
                                )}
                              </td>
                              {sculpture && (
                                <td className="ui-num" data-browse-endo-per-plat={endo?.endoPerPlat ?? ""}>
                                  {endo ? <Readout currency="endo" value={endo.endoPerPlat} locale={locale} /> : "-"}
                                  {endo && <span className="block text-xs font-normal text-text-muted">{tr("browse.endoValue", { endo: endo.endo.toLocaleString(locale) })}</span>}
                                </td>
                              )}
                              <td className="ui-num">
                                <Button size="sm" variant={copied === key ? "primary" : "default"} className="min-w-[104px]" icon={copied === key ? <Check /> : <Copy />} title={whisper(e)} onClick={() => void copy(e, key)}>
                                  {copied === key ? tr("common.copied") : tr("browse.copyWhisper")}
                                </Button>
                              </td>
                            </tr>
                          );
                        })}
                      </tbody>
                    </Table>
                  </Card>
                  <div className="text-center text-xs text-text-muted">
                    {tr(side === "sell" ? "browse.showingSell" : "browse.showingBuy", { shown: rows.length, total: filtered.length })}
                    {filtered.length > rows.length && (
                      <>
                        {" "}
                        <button className="link-btn" onClick={() => setRowLimit((n) => n + MAX_ROWS)}>
                          {tr("browse.showMore", { n: Math.min(MAX_ROWS, filtered.length - rows.length) })}
                        </button>
                      </>
                    )}
                  </div>
                </>
              )}
            </>
          )}
        </>
      )}
    </div>
  );
}
