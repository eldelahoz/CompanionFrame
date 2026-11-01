import { ArrowDown, ArrowUp, Check, Copy, RefreshCw, SearchX, TriangleAlert } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AYATAN_SCULPTURES, ayatanSculptureBySlug } from "../../../config/shared/ayatanEndo";
import { titleFromSlug } from "../../../config/shared/wfm";
import { formatUnitPlatinum } from "../../../config/shared/wfmOrders";
import { nextColumnSort } from "../../lib/columnSort";
import type { MessageKey, Translator } from "../../lib/i18n";
import { fetchItemOrderBookBySlug } from "../../lib/wfm/orderBook";
import {
  ayatanOrderWhisper,
  ayatanSortStart,
  buildAyatanListings,
  EMPTY_AYATAN_BOOK,
  formatLoadAge,
  loadAyatanViewPrefs,
  saveAyatanViewPrefs,
  settleAyatanBook,
  sortAyatanListings,
  summarizeAyatanBooks,
  type AyatanBookState,
  type AyatanListing,
  type AyatanSortKey,
  type AyatanStarsFilter,
} from "../../lib/wfm/ayatanListings";
import { sellerStatusLabelKey, type SellerStatusFilter } from "../../lib/wfm/orderRows";
import ItemImage from "../ItemImage";
import { Badge, Button, Card, Checkbox, Chip, EmptyState, Field, IconButton, Input, Readout, Skeleton, Table, type Tone } from "../ui";

export interface BrowseItem {
  name: string;
  slug: string;
  thumb: string | null;
  gameRef: string | null;
  maxRank: number | null;
}

interface Props {
  catalog: readonly BrowseItem[];
  onOpenItem: (slug: string) => void;
  onFeedback: (key: MessageKey) => void;
  tr: Translator;
  locale: string;
}

const AUTO_REFRESH_MS = 45_000;
const MAX_ROWS = 50;
const CONCURRENCY = 4;

const COLUMNS: ReadonlyArray<{ key: AyatanSortKey; labelKey: MessageKey; alignEnd: boolean }> = [
  { key: "sculpture", labelKey: "browse.ayatan.col.sculpture", alignEnd: false },
  { key: "qty", labelKey: "browse.col.qty", alignEnd: false },
  { key: "user", labelKey: "browse.col.user", alignEnd: false },
  { key: "status", labelKey: "browse.status", alignEnd: false },
  { key: "unitPrice", labelKey: "browse.col.unitPrice", alignEnd: true },
  { key: "endoPerPlat", labelKey: "browse.col.endoPerPlat", alignEnd: true },
];
const STATUS_OPTIONS: ReadonlyArray<readonly [SellerStatusFilter, MessageKey]> = [
  ["ingame", "common.inGame"],
  ["onsite", "browse.onSite"],
  ["all", "common.all"],
];
const STARS_OPTIONS: ReadonlyArray<readonly [AyatanStarsFilter, MessageKey]> = [
  ["any", "filters.any"],
  ["full", "browse.ayatan.starsFull"],
  ["none", "common.none"],
];

export const statusTone = (status: string | null): Tone => (status === "ingame" ? "success" : status === "online" ? "info" : "neutral");

export default function AyatanEndoBrowse({ catalog, onOpenItem, onFeedback, tr, locale }: Props) {
  const [books, setBooks] = useState<Record<string, AyatanBookState>>({});
  const [prefs, setPrefs] = useState(loadAyatanViewPrefs);
  const [rowLimit, setRowLimit] = useState(MAX_ROWS);
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const [now, setNow] = useState(Date.now());
  const generation = useRef(0);
  const refreshTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const copiedTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const menuRoot = useRef<HTMLDivElement>(null);

  const bySlug = useMemo(() => new Map(catalog.map((c) => [c.slug, c])), [catalog]);
  const labelOf = useCallback((slug: string) => bySlug.get(slug)?.name ?? titleFromSlug(slug), [bySlug]);

  useEffect(() => saveAyatanViewPrefs(prefs), [prefs]);

  const loadAll = useCallback(async () => {
    const token = ++generation.current;
    clearTimeout(refreshTimer.current);
    setBooks((b) => Object.fromEntries(AYATAN_SCULPTURES.map(({ slug }) => [slug, { ...(b[slug] ?? EMPTY_AYATAN_BOOK), loading: true }])));
    // A failed book waits for the next refresh: an instant retry after a 429 only adds load.
    const queue = AYATAN_SCULPTURES.map((s) => s.slug);
    const worker = async () => {
      for (let slug = queue.shift(); slug && token === generation.current; slug = queue.shift()) {
        const result = await fetchItemOrderBookBySlug(slug);
        if (token !== generation.current) return;
        setBooks((b) => ({ ...b, [slug!]: settleAyatanBook(b[slug!] ?? EMPTY_AYATAN_BOOK, result) }));
      }
    };
    await Promise.all(Array.from({ length: CONCURRENCY }, worker));
    if (token === generation.current) refreshTimer.current = setTimeout(() => void loadAll(), AUTO_REFRESH_MS);
  }, []);

  useEffect(() => {
    void loadAll();
    const clock = setInterval(() => setNow(Date.now()), 1000);
    return () => {
      generation.current += 1;
      clearTimeout(refreshTimer.current);
      clearTimeout(copiedTimer.current);
      clearInterval(clock);
    };
  }, [loadAll]);

  useEffect(() => {
    if (!menuOpen) return;
    const outside = (e: PointerEvent) => !menuRoot.current?.contains(e.target as Node) && setMenuOpen(false);
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setMenuOpen(false);
    window.addEventListener("pointerdown", outside);
    window.addEventListener("keydown", esc);
    return () => {
      window.removeEventListener("pointerdown", outside);
      window.removeEventListener("keydown", esc);
    };
  }, [menuOpen]);

  const loadedBooks = AYATAN_SCULPTURES.flatMap((sculpture) => {
    const sell = books[sculpture.slug]?.sell ?? [];
    return sell.length > 0 ? [{ sculpture, sell }] : [];
  });
  const listings = useMemo(() => sortAyatanListings(buildAyatanListings(loadedBooks, prefs), prefs, labelOf), [books, prefs, labelOf]); // eslint-disable-line react-hooks/exhaustive-deps
  const rows = listings.slice(0, rowLimit);
  const load = summarizeAyatanBooks(AYATAN_SCULPTURES.map(({ slug }) => books[slug] ?? EMPTY_AYATAN_BOOK));
  const best = listings.reduce<number | null>((m, r) => Math.max(m ?? 0, r.endoPerPlat), null);
  const sculptureOptions = [...AYATAN_SCULPTURES].map(({ slug }) => ({ slug, label: labelOf(slug) })).sort((a, b) => a.label.localeCompare(b.label));

  const patch = (p: Partial<typeof prefs>) => (setPrefs((x) => ({ ...x, ...p })), setRowLimit(MAX_ROWS));
  const sortBy = (key: AyatanSortKey) => patch(nextColumnSort(prefs, key, ayatanSortStart));
  const toggleSculpture = (slug: string) => patch({ sculptures: prefs.sculptures.includes(slug) ? prefs.sculptures.filter((s) => s !== slug) : [...prefs.sculptures, slug] });

  const whisperFor = (row: AyatanListing) => ayatanOrderWhisper(tr, "sell", row.entry, labelOf(row.slug), ayatanSculptureBySlug(row.slug));
  const copy = async (row: AyatanListing, key: string) => {
    try {
      if (!navigator.clipboard?.writeText) return onFeedback("common.clipboardUnavailableInThisEnvironment");
      await navigator.clipboard.writeText(whisperFor(row));
      setCopiedKey(key);
      clearTimeout(copiedTimer.current);
      copiedTimer.current = setTimeout(() => setCopiedKey(null), 1600);
      onFeedback("browse.whisperCopied");
    } catch {
      onFeedback("common.failedToCopyWhisper");
    }
  };

  const noticeText = load.notice
    ? tr(load.notice.key, { count: load.notice.count, total: load.notice.total, age: load.notice.since != null ? formatLoadAge(load.notice.since, now, locale) : "" })
    : null;

  return (
    <div className="flex flex-col gap-4" data-ayatan-endo-browse>
      <Card className="flex flex-wrap items-center gap-x-6 gap-y-3 px-4 py-3" data-ayatan-header>
        <div className="min-w-[16rem] flex-1">
          <h3 className="m-0 text-lg font-semibold">{tr("browse.ayatan.preset")}</h3>
          <p className="m-0 mt-1 max-w-[34rem] text-xs text-text-secondary">{tr("browse.ayatan.explainer")}</p>
        </div>
        <dl className="m-0 flex flex-wrap items-center gap-x-5 gap-y-2" data-ayatan-summary>
          <div className="grid gap-1">
            <dt className="text-xs text-text-muted">{tr("browse.ayatan.bestEndoPerPlat")}</dt>
            <dd className="m-0 font-semibold" data-ayatan-best={best ?? ""}>
              {best != null ? <Readout currency="endo" value={best} locale={locale} chip /> : "-"}
            </dd>
          </div>
          <div className="grid gap-1 border-l border-border pl-5">
            <dt className="text-xs text-text-muted">{tr("browse.ayatan.orderCount")}</dt>
            <dd className="m-0 font-semibold tabular-nums" data-ayatan-order-count={listings.length}>
              {listings.length.toLocaleString(locale)}
            </dd>
          </div>
          <div className="grid gap-1 border-l border-border pl-5">
            <dt className="text-xs text-text-muted">{tr("browse.ayatan.updated")}</dt>
            <dd className={`m-0 font-semibold ${load.notice?.since != null ? "text-warning" : ""}`} data-ayatan-updated={load.updatedAt ?? ""}>
              {load.updatedAt != null ? formatLoadAge(load.updatedAt, now, locale) : "-"}
            </dd>
          </div>
        </dl>
        <IconButton label={tr("common.refresh")} data-ayatan-refresh onClick={() => void loadAll()}>
          <RefreshCw />
        </IconButton>
      </Card>

      <div className="flex flex-wrap items-end gap-x-4 gap-y-3" data-ayatan-filters>
        <Field label={tr("browse.status")}>
          <div className="flex gap-1">
            {STATUS_OPTIONS.map(([value, key]) => (
              <Chip key={value} pressed={prefs.status === value} data-ayatan-status={value} onClick={() => patch({ status: value })}>
                {tr(key)}
              </Chip>
            ))}
          </div>
        </Field>
        <div className="relative" ref={menuRoot}>
          <Field label={<span id="ayatan-sculpture-filter-label">{tr("browse.ayatan.col.sculpture")}</span>}>
            <button
              type="button"
              className="ui-select min-w-[11rem] max-w-[16rem] cursor-pointer truncate text-left"
              aria-haspopup="true"
              aria-expanded={menuOpen}
              aria-controls="ayatan-sculpture-menu"
              data-ayatan-sculpture-filter
              onClick={() => setMenuOpen((o) => !o)}
            >
              {prefs.sculptures.length === 0 ? tr("common.all") : prefs.sculptures.length === 1 ? labelOf(prefs.sculptures[0]) : tr("common.selected", { count: prefs.sculptures.length })}
            </button>
          </Field>
          {menuOpen && (
            <div id="ayatan-sculpture-menu" role="group" aria-labelledby="ayatan-sculpture-filter-label" className="ui-card absolute left-0 top-full z-20 mt-1 grid max-h-[22rem] w-max min-w-full gap-1 overflow-y-auto p-2" style={{ boxShadow: "var(--shadow-2)" }} data-ayatan-sculpture-menu>
              {/* unticking "All" has nothing to fall back to, so it stays ticked */}
              <Checkbox label={tr("common.all")} checked={prefs.sculptures.length === 0} data-ayatan-sculpture-option="all" onChange={() => patch({ sculptures: [] })} />
              {sculptureOptions.map((o) => (
                <Checkbox key={o.slug} label={o.label} checked={prefs.sculptures.includes(o.slug)} data-ayatan-sculpture-option={o.slug} onChange={() => toggleSculpture(o.slug)} />
              ))}
            </div>
          )}
        </div>
        <Field label={tr("browse.ayatan.starsFilter")}>
          <div className="flex gap-1">
            {STARS_OPTIONS.map(([value, key]) => (
              <Chip key={value} pressed={prefs.stars === value} data-ayatan-stars-filter={value} onClick={() => patch({ stars: value })}>
                {tr(key)}
              </Chip>
            ))}
          </div>
        </Field>
        <Field label={tr("browse.col.endoPerPlat")}>
          <Input className="w-24" type="number" min="0" placeholder={tr("common.min")} data-ayatan-min-endo value={prefs.minEndoPerPlat ?? ""} onChange={(e) => patch({ minEndoPerPlat: e.target.value === "" ? null : Number(e.target.value) })} />
        </Field>
      </div>

      {noticeText && (
        <div className="text-xs text-danger" data-ayatan-notice={load.notice?.key}>
          {noticeText}
        </div>
      )}

      {!load.hasData && load.pending > 0 ? (
        <Card className="flex flex-col gap-3 p-4" aria-busy="true" aria-label={tr("common.loadingListings")}>
          <Skeleton width="55%" height="1.1rem" />
          <Skeleton width="40%" height="1.1rem" />
        </Card>
      ) : !load.hasData && load.failed > 0 ? (
        <Card>
          <EmptyState icon={<TriangleAlert />} title={tr("common.failedToLoadListingsTryAgain")} />
        </Card>
      ) : rows.length === 0 ? (
        <Card data-ayatan-empty>
          <EmptyState icon={<SearchX />} title={tr("browse.noOrdersSell")} />
        </Card>
      ) : (
        <>
          <Card className="overflow-hidden">
            <Table mode="scroll" label={tr("browse.ayatan.preset")}>
              <thead>
                <tr>
                  {COLUMNS.map((c) => {
                    const active = prefs.sortBy === c.key;
                    return (
                      <th key={c.key} className={c.alignEnd ? "ui-num" : ""} data-ayatan-column={c.key} aria-sort={active ? (prefs.sortDirection === "asc" ? "ascending" : "descending") : "none"}>
                        <button type="button" className="inline-flex cursor-pointer items-center gap-1 border-0 bg-transparent p-0 font-[inherit] text-inherit hover:text-text-primary" data-ayatan-sort={c.key} onClick={() => sortBy(c.key)}>
                          {tr(c.labelKey)}
                          {active && (prefs.sortDirection === "asc" ? <ArrowUp className="h-3 w-3" /> : <ArrowDown className="h-3 w-3" />)}
                        </button>
                      </th>
                    );
                  })}
                  <th className="ui-num">{tr("browse.col.buy")}</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((row, i) => {
                  const key = `${row.slug}:${row.entry.userName}:${row.entry.platinum}:${i}`;
                  const ref = bySlug.get(row.slug);
                  const label = labelOf(row.slug);
                  return (
                    <tr key={key} data-ayatan-row data-ayatan-slug={row.slug}>
                      <td>
                        <button type="button" className="flex cursor-pointer items-center gap-2 border-0 bg-transparent p-0 text-left text-text-primary hover:text-accent" title={tr("browse.openItem")} data-ayatan-open={row.slug} onClick={() => onOpenItem(row.slug)}>
                          <span className="ui-thumb">
                            <ItemImage src={ref?.thumb ?? null} alt={label} cls="max-h-full max-w-full" tr={tr} />
                          </span>
                          <span className="grid">
                            <span className="font-semibold">{label}</span>
                            <span className="text-xs text-text-muted" data-ayatan-stars={row.stars.amber + row.stars.cyan}>
                              {tr("browse.ayatan.stars", { filled: row.stars.amber + row.stars.cyan, total: row.sockets })}
                            </span>
                          </span>
                        </button>
                      </td>
                      <td className="text-text-secondary">x{row.entry.quantity}</td>
                      <td>
                        <a className="link-btn font-semibold" href={`https://warframe.market/profile/${encodeURIComponent(row.entry.userName)}`} target="_blank" rel="noreferrer noopener" title={tr("browse.openProfile")}>
                          {row.entry.userName}
                        </a>
                      </td>
                      <td>
                        <Badge tone={statusTone(row.entry.status)}>{tr(sellerStatusLabelKey(row.entry.status))}</Badge>
                      </td>
                      <td className="ui-num" data-ayatan-unit-plat={formatUnitPlatinum(row.entry.unitPlatinum)}>
                        <Readout currency="platinum" value={null}>
                          <b>{formatUnitPlatinum(row.entry.unitPlatinum)}</b>
                        </Readout>
                        {row.entry.perTrade > 1 && <span className="block text-xs font-normal text-text-muted">{tr("orderbook.perTrade", { count: row.entry.perTrade, platinum: row.entry.platinum })}</span>}
                      </td>
                      <td className="ui-num" data-ayatan-endo-per-plat={row.endoPerPlat}>
                        <Readout currency="endo" value={row.endoPerPlat} locale={locale} chip />
                        <span className="block text-xs font-normal text-text-muted">{tr("browse.endoValue", { endo: row.endo.toLocaleString(locale) })}</span>
                      </td>
                      <td className="ui-num">
                        <Button size="sm" variant={copiedKey === key ? "primary" : "default"} className="min-w-[104px]" icon={copiedKey === key ? <Check /> : <Copy />} title={whisperFor(row)} onClick={() => void copy(row, key)}>
                          {copiedKey === key ? tr("common.copied") : tr("browse.copyWhisper")}
                        </Button>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </Table>
          </Card>
          <div className="text-center text-xs text-text-muted">
            {tr("browse.showingSell", { shown: rows.length, total: listings.length })}
            {listings.length > rows.length && (
              <>
                {" "}
                <button className="link-btn" onClick={() => setRowLimit((n) => n + MAX_ROWS)}>
                  {tr("browse.showMore", { n: Math.min(MAX_ROWS, listings.length - rows.length) })}
                </button>
              </>
            )}
          </div>
        </>
      )}
    </div>
  );
}
