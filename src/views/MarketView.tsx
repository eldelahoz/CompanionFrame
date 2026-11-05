import { ArrowDown, ArrowUp, LogOut, Plus, RefreshCw, ShoppingCart, Star, WandSparkles } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { normalizePerTrade } from "../../config/shared/wfmOrders";
import HeaderTabs from "../components/HeaderTabs";
import MarketAlerts from "../components/market/alerts/MarketAlerts";
import MarketContracts from "../components/market/MarketContracts";
import RepriceOrdersModal from "../components/market/RepriceOrdersModal";
import MarketBrowseView, { type OrderDraft } from "../components/market/MarketBrowseView";
import MarketOrderRow from "../components/market/MarketOrderRow";
import MarketBulkBar, { type BulkRun } from "../components/market/MarketBulkBar";
import { sortOrders, type OrderSortDir, type OrderSortKey } from "../lib/market/sortOrders";
import { wfmLookup } from "../lib/market/wfmLookup";
import { usePersisted } from "../lib/usePersisted";
import { applyIds, useRangeSelect } from "../lib/useRangeSelect";
import { useMarketArt } from "../lib/useMarketArt";
import OrderModal from "../components/market/OrderModal";
import SignInForm from "../components/market/SignInForm";
import WfmPresenceBar, { DOT } from "../components/market/WfmPresenceBar";
import { Button, Card, EmptyState, IconButton, PageHeader, Skeleton, Table } from "../components/ui";
import { askConfirm } from "../stores/confirm";
import { addToast } from "../stores/toasts";
import type { Translator } from "../lib/i18n";
import { onWfmOrderClosed, wfmMyReputation, wfmOrderClose, wfmOrderDelete, wfmOrderUpdate, wfmOrdersMy, type ItemDatabase, type MyOrder, type MyOrders } from "../lib/ipc";
import type { WfmPresenceState, PresenceOptions, WfmStatus, WfmSession } from "../lib/ipc";
import { orderInventoryMatch } from "../lib/marketOrderInventory";
import { useInventory, useParsedInventory } from "../stores/inventory";
import { useWfmCatalog } from "../stores/market";
import type { ItemDbEntry } from "../types/inventory";

type Tab = "sell" | "buy" | "rivens" | "browse" | "alerts";
const POLL_MS = 30_000;
const SORTS = (["name", "platinum", "quantity", "date", "visible"] as const).flatMap((k) => (["asc", "desc"] as const).map((d) => `${k}:${d}` as const));

interface Props {
  tr: Translator;
  /** The item database, to judge whether a listing is still backed by the inventory. */
  items: ItemDatabase;
  /** Number and date formatting of the Browse tab. */
  locale?: string;
  session: WfmSession | null;
  signIn: (email: string, password: string) => Promise<void>;
  signInWebview?: () => Promise<void>;
  signOut: () => Promise<void>;
  presence: { state: WfmPresenceState; options: PresenceOptions; setStatus: (s: WfmStatus) => void; saveOptions: (p: Partial<PresenceOptions>) => void };
}

export default function MarketView({ tr, items, locale = "en", session, signIn, signInWebview, signOut, presence }: Props) {
  const [tab, setTab] = useState<Tab>("sell");
  const [orders, setOrders] = useState<MyOrders>({ sell: [], buy: [] });
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState<ReadonlySet<string>>(new Set());
  const [modal, setModal] = useState<{ order: MyOrder | null; draft?: OrderDraft } | null>(null);
  const [repricing, setRepricing] = useState<MyOrder[] | null>(null);
  const [sort, setSort] = usePersisted("wf_market_orders_sort", SORTS, "date:desc");
  const [sortKey, sortDir] = sort.split(":") as [OrderSortKey, OrderSortDir];
  const [bulkRun, setBulkRun] = useState<BulkRun | null>(null);
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
  const { catalog } = useWfmCatalog();
  const art = useMarketArt();
  const loggedIn = session?.logged_in === true;
  const { data: inventory } = useInventory();
  const parsed = useParsedInventory(inventory, items);
  const wfmItems = useMemo(() => wfmLookup(catalog), [catalog]);

  const fetchOrders = useCallback(async (background = false) => {
    if (!background) setLoading(true);
    try {
      setOrders(await wfmOrdersMy());
      setError("");
    } catch (e) {
      if (!background) setError((e as Error).message);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => setSelected(new Set()), [tab]);
  // a refetch can drop orders: forget their ids
  useEffect(() => {
    const live = new Set([...orders.sell, ...orders.buy].map((o) => o.id));
    setSelected((cur) => (cur.size === 0 || [...cur].every((id) => live.has(id)) ? cur : new Set([...cur].filter((id) => live.has(id)))));
  }, [orders]);

  useEffect(() => {
    if (!loggedIn) return setOrders({ sell: [], buy: [] });
    void fetchOrders();
    const t = setInterval(() => !document.hidden && void fetchOrders(true), POLL_MS);
    return () => clearInterval(t);
  }, [loggedIn, fetchOrders]);

  // Once per sign-in, not on the poll; a failed lookup just hides the figure.
  const [reputation, setReputation] = useState<number | null>(null);
  useEffect(() => {
    if (!loggedIn) return setReputation(null);
    let dead = false;
    wfmMyReputation().then((r) => !dead && setReputation(r), () => !dead && setReputation(null));
    return () => {
      dead = true;
    };
  }, [loggedIn]);

  // A trade match closed units elsewhere: patch just that row; the poll reconciles the rest.
  useEffect(() => {
    if (!loggedIn) return;
    let off: (() => void) | undefined;
    let dead = false;
    void onWfmOrderClosed(({ order_id, quantity }) => {
      const lower = (list: MyOrder[]) =>
        list.flatMap((o) => (o.id !== order_id ? [o] : o.quantity - quantity > 0 ? [{ ...o, quantity: o.quantity - quantity }] : []));
      setOrders((cur) => ({ sell: lower(cur.sell), buy: lower(cur.buy) }));
    }).then((u) => (dead ? u() : (off = u)));
    return () => {
      dead = true;
      off?.();
    };
  }, [loggedIn]);

  // One write per order at a time; the lock also covers the refetch that follows.
  const guarded = async (id: string, run: () => Promise<void>) => {
    if (busy.has(id)) return;
    setBusy((b) => new Set(b).add(id));
    try {
      await run();
    } catch (e) {
      addToast({ level: "error", message: (e as Error).message });
    } finally {
      setBusy((b) => {
        const n = new Set(b);
        n.delete(id);
        return n;
      });
    }
  };

  const del = async (id: string) => {
    if (!(await askConfirm(tr("market.confirmDeleteOrder")))) return;
    await guarded(id, async () => {
      await wfmOrderDelete(id);
      addToast({ level: "success", message: tr("market.orderDeleted") });
      await fetchOrders(true);
    });
  };
  const closeOne = async (o: MyOrder) => {
    const size = normalizePerTrade(o.perTrade);
    const bought = o.orderType === "buy";
    const params = { count: size, item: o.itemName, price: o.platinum };
    const msg = tr(size > 1 ? (bought ? "market.confirmOrderBoughtTrade" : "market.confirmOrderSoldTrade") : bought ? "market.confirmOrderBought" : "market.confirmOrderSold", params);
    if (!(await askConfirm(msg))) return;
    // the API reports no remainder, so refetch
    void guarded(o.id, async () => {
      await wfmOrderClose(o.id, size);
      await fetchOrders(true);
    });
  };
  const patch = (o: MyOrder, updates: { platinum?: number; quantity?: number; visible?: boolean }) =>
    guarded(o.id, async () => {
      await wfmOrderUpdate(o.id, updates as never);
      await fetchOrders(true);
    });

  // Stop (or leaving the view) ends a bulk run before its next order; the one in flight finishes.
  const stopBulk = useRef(false);
  const gone = useRef(false);
  useEffect(() => {
    gone.current = false;
    return () => {
      gone.current = true;
      stopBulk.current = true;
    };
  }, []);
  const patchLocal = (id: string, fn: (o: MyOrder) => MyOrder | null) => {
    const apply = (l: MyOrder[]) => l.flatMap((o) => (o.id !== id ? [o] : fn(o) ?? []));
    setOrders((cur) => ({ sell: apply(cur.sell), buy: apply(cur.buy) }));
  };
  const bulk = async (action: BulkRun["action"], targets: MyOrder[], run: (o: MyOrder) => Promise<void>) => {
    stopBulk.current = false;
    setBulkRun({ action, done: 0, total: targets.length });
    try {
      // one at a time: warframe.market rate limits
      for (const [i, o] of targets.entries()) {
        if (stopBulk.current) break;
        await guarded(o.id, () => run(o));
        setBulkRun({ action, done: i + 1, total: targets.length });
      }
    } finally {
      setBulkRun(null);
      // the one source of truth, unless the view was left
      if (!gone.current) await fetchOrders(true);
    }
  };
  const setAllVisible = (visible: boolean) =>
    bulk(
      visible ? "show" : "hide",
      list.filter((o) => selected.has(o.id) && o.visible !== visible),
      async (o) => {
        await wfmOrderUpdate(o.id, { visible } as never);
        patchLocal(o.id, (x) => ({ ...x, visible }));
      },
    );
  const delSelected = async () => {
    const targets = list.filter((o) => selected.has(o.id));
    if (!(await askConfirm(tr("market.confirmDeleteOrders", { count: targets.length })))) return;
    await bulk("delete", targets, async (o) => {
      await wfmOrderDelete(o.id);
      patchLocal(o.id, () => null);
      setSelected((cur) => applyIds(cur, [o.id], false));
    });
  };
  // Shift+click takes every row between the last click and this one, in the sorted order.
  const toggleSelected = (id: string, shift: boolean) => {
    const on = !selected.has(id);
    const ids = range(id, shift);
    setSelected((cur) => applyIds(cur, ids, on));
  };

  const tabs = [
    { key: "sell", label: tr("market.tab.sell"), count: loggedIn ? orders.sell.length : undefined },
    { key: "buy", label: tr("market.tab.buy"), count: loggedIn ? orders.buy.length : undefined },
    { key: "rivens", label: tr("common.rivens") },
    { key: "browse", label: tr("market.tab.browse") },
    { key: "alerts", label: tr("common.alerts") },
  ];
  const isOrders = tab === "sell" || tab === "buy";
  const list = useMemo(() => (isOrders ? sortOrders(orders[tab], sortKey, sortDir) : []), [isOrders, orders, tab, sortKey, sortDir]);
  const range = useRangeSelect(list.map((o) => o.id));
  const pickedCount = list.filter((o) => selected.has(o.id)).length;
  const sortHead = (key: OrderSortKey, label: string) => (
    <th aria-sort={sortKey === key ? (sortDir === "asc" ? "ascending" : "descending") : "none"}>
      <button type="button" aria-label={tr("market.sortBy", { column: label })} onClick={() => setSort(sortKey === key ? `${key}:${sortDir === "asc" ? "desc" : "asc"}` : `${key}:${key === "name" ? "asc" : "desc"}`)}>
        {label}
        {sortKey === key && (sortDir === "asc" ? <ArrowUp className="size-3" aria-hidden /> : <ArrowDown className="size-3" aria-hidden />)}
      </button>
    </th>
  );

  return (
    <div>
      <PageHeader
        title={tr("common.market")}
        sub={
          loggedIn && session?.user_name ? (
            <span className="titlebar-pill" data-market-user>
              <span className={`status-dot ${DOT[presence.state.status ?? "invisible"]}`} />@{session.user_name}
              {reputation !== null && (
                <span className="inline-flex items-center gap-0.5 tabular-nums text-text-secondary" title={tr("market.reputation")} aria-label={tr("market.reputation")} data-market-reputation>
                  · <Star className="size-3" aria-hidden />
                  {reputation}
                </span>
              )}
            </span>
          ) : undefined
        }
        actions={
          loggedIn ? (
            <Button variant="ghost" size="sm" icon={<LogOut />} onClick={() => void signOut()}>
              {tr("market.signOut")}
            </Button>
          ) : undefined
        }
      />
      <div className="mb-4 border-b border-border">
        <HeaderTabs label={tr("common.market")} options={tabs} activeKey={tab} onSelect={(k) => setTab(k as Tab)} />
      </div>
      {tab === "browse" ? (
        // works without a session: only posting an order needs one
        <MarketBrowseView tr={tr} locale={locale} catalog={catalog} loggedIn={loggedIn} onPostOrder={(draft) => setModal({ order: null, draft })} />
      ) : tab === "alerts" ? (
        // public market reads only: no session needed
        <MarketAlerts tr={tr} locale={locale} />
      ) : session === null ? null : !loggedIn ? (
        // read-only: signing in is the only action offered
        <SignInForm onSignIn={signIn} onSignInWebview={signInWebview} tr={tr} />
      ) : tab === "rivens" ? (
        <MarketContracts tr={tr} />
      ) : (
        <>
          <Card className="mb-4 px-4 py-3">
            <WfmPresenceBar {...presence} tr={tr} />
          </Card>
          {!session.persistable && (
            <p className="mb-3 text-sm text-warning" data-market-session-not-saved>
              {tr("market.sessionNotSaved")}
            </p>
          )}
          <div className="mb-3 flex flex-wrap items-center gap-2">
            <h2 className="m-0 mr-auto text-lg font-semibold" data-market-orders-heading>
              {tr("market.myOrders")}
            </h2>
            {tab === "sell" && (
              <Button size="sm" icon={<WandSparkles />} data-market-reprice disabled={list.length === 0} onClick={() => setRepricing(orders.sell)}>
                {tr("market.repriceSelected")}
              </Button>
            )}
            <IconButton label={tr("common.refresh")} onClick={() => fetchOrders()}>
              <RefreshCw />
            </IconButton>
            <Button size="sm" variant="primary" icon={<Plus />} data-market-new-order onClick={() => setModal({ order: null })}>
              {tr("market.newOrder")}
            </Button>
          </div>
          {error && (
            <p className="text-danger" role="alert">
              {error}
            </p>
          )}
          {loading ? (
            <Card className="flex flex-col gap-4 p-4" aria-busy="true" aria-label={tr("market.loadingOrders")}>
              {[60, 45, 52].map((w) => (
                <Skeleton key={w} width={`${w}%`} height="1.1rem" />
              ))}
            </Card>
          ) : list.length === 0 ? (
            <Card>
              <EmptyState
                icon={<ShoppingCart />}
                title={tr("market.noOrdersPrefix", { tab: tr(tab === "sell" ? "market.orderTypeLower.sell" : "market.orderTypeLower.buy") }) + " " + tr("market.newOrder") + " " + tr("market.noOrdersSuffix")}
                action={
                  <Button size="sm" variant="primary" icon={<Plus />} onClick={() => setModal({ order: null })}>
                    {tr("market.newOrder")}
                  </Button>
                }
              />
            </Card>
          ) : (
            <Card className="overflow-hidden">
              <Table mode="stack" label={tr("market.myOrders")}>
                <thead>
                  <tr>
                    <th>
                      <input
                        type="checkbox"
                        checked={pickedCount === list.length}
                        ref={(el) => void (el && (el.indeterminate = pickedCount > 0 && pickedCount < list.length))}
                        onChange={() => setSelected(pickedCount === list.length ? new Set() : new Set(list.map((o) => o.id)))}
                        aria-label={tr("market.selectAllOrders")}
                      />
                    </th>
                    {sortHead("name", tr("common.item"))}
                    {sortHead("platinum", tr("common.platinum"))}
                    {sortHead("quantity", tr("market.col.quantity"))}
                    {sortHead("date", tr("market.col.listed"))}
                    {sortHead("visible", tr("market.visibleColumn"))}
                    <th>
                      <span className="sr-only">{tr("common.actions")}</span>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {list.map((o) => (
                    <MarketOrderRow
                      // remount on server change so the inline drafts follow a close or reprice
                      key={`${o.id}:${o.platinum}:${o.quantity}`}
                      order={o}
                      art={art(o.itemSlug)}
                      busy={busy.has(o.id)}
                      selected={selected.has(o.id)}
                      onSelect={toggleSelected}
                      locale={locale}
                      onEdit={(x) => setModal({ order: x })}
                      onDelete={del}
                      onCloseOne={closeOne}
                      onToggleVisible={(x) => patch(x, { visible: !x.visible })}
                      onInlineSave={(x, u) => patch(x, u)}
                      // no parsed inventory is no proof, so no markers rather than all
                      inventoryMatch={parsed.length > 0 ? orderInventoryMatch({ ...o, itemUrlName: o.itemSlug }, parsed, wfmItems, items as unknown as Record<string, ItemDbEntry>) : null}
                      tr={tr}
                    />
                  ))}
                </tbody>
              </Table>
            </Card>
          )}
        </>
      )}
      {(pickedCount > 0 || bulkRun) && (
        <MarketBulkBar
          count={pickedCount}
          total={list.length}
          canReprice={tab === "sell"}
          busy={busy.size > 0}
          run={bulkRun}
          onAll={() => setSelected(new Set(list.map((o) => o.id)))}
          onHide={() => void setAllVisible(false)}
          onShow={() => void setAllVisible(true)}
          onReprice={() => setRepricing(list.filter((o) => selected.has(o.id)))}
          onDelete={() => void delSelected()}
          onCancel={() => (bulkRun ? (stopBulk.current = true) : setSelected(new Set()))}
          tr={tr}
        />
      )}
      {repricing && (
        <RepriceOrdersModal
          orders={repricing}
          ownUserName={session?.user_name ?? null}
          tr={tr}
          onClose={() => setRepricing(null)}
          onApplied={() => void fetchOrders(true)}
        />
      )}
      {modal && (
        <OrderModal
          order={modal.order}
          defaultType={tab === "buy" ? "buy" : "sell"}
          catalog={catalog}
          draft={modal.draft}
          onClose={() => setModal(null)}
          onSaved={() => {
            setModal(null);
            void fetchOrders(true);
          }}
          tr={tr}
        />
      )}
    </div>
  );
}
