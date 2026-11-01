import { X } from "lucide-react";
import { useMemo, useState } from "react";
import { subtypeChoicesOf } from "../../../config/shared/wfmOrders";
import type { Translator } from "../../lib/i18n";
import { wfmOrderCreate, wfmOrderUpdate, type CatalogItem, type MyOrder } from "../../lib/ipc";
import { hasRanks, type CatalogIndex } from "../../lib/wfm/catalog";
import { Button, Checkbox, Field, IconButton, Input, Modal, Readout, SearchInput, Segmented, Select } from "../ui";

interface Props {
  /** Null creates a new order. */
  order: MyOrder | null;
  defaultType: "sell" | "buy";
  catalog: CatalogIndex | null;
  /** Prefill for a new order started from the Browse tab. */
  draft?: { item: CatalogItem; orderType: "sell" | "buy"; rank: number | null; subtype: string | null };
  onClose: () => void;
  onSaved: () => void;
  tr: Translator;
}

export default function OrderModal({ order, defaultType, catalog, draft, onClose, onSaved, tr }: Props) {
  const [query, setQuery] = useState("");
  const [item, setItem] = useState<CatalogItem | null>(draft?.item ?? null);
  const [type, setType] = useState(order?.orderType ?? draft?.orderType ?? defaultType);
  const [platinum, setPlatinum] = useState(order?.platinum ?? 0);
  const [quantity, setQuantity] = useState(order?.quantity ?? 1);
  const [rank, setRank] = useState(draft?.rank ?? 0);
  const [visible, setVisible] = useState(order?.visible ?? true);
  const [subtypes, setSubtypes] = useState<readonly string[] | null>(null);
  const [subtype, setSubtype] = useState(draft?.subtype ?? "");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const matches = useMemo(() => (catalog && !item && query.trim() ? catalog.search(query, 8) : []), [catalog, item, query]);

  const submit = async (e?: React.FormEvent) => {
    e?.preventDefault();
    if (!order && !item) return setError(tr("orderModal.selectItemRequired"));
    if (platinum < 1) return setError(tr("orderModal.priceMin"));
    if (quantity < 1) return setError(tr("orderModal.quantityMin"));
    setError("");
    setBusy(true);
    try {
      if (order) await wfmOrderUpdate(order.id, { platinum, quantity, visible } as never);
      else
        await wfmOrderCreate({
          itemId: item!.id,
          orderType: type,
          platinum,
          quantity,
          visible,
          modRank: hasRanks(item!) ? rank : null,
          perTrade: null,
          subtype: subtype || null,
        });
      onSaved();
    } catch (err) {
      const choices = subtypeChoicesOf(err);
      if (choices) {
        setSubtypes(choices);
        setSubtype(choices[0]);
        setError(tr("orderModal.subtypeRequired"));
      } else setError((err as Error).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      title={tr(order ? "orderModal.editOrder" : "orderModal.newOrder")}
      tr={tr}
      onClose={onClose}
      width="28rem"
      footer={
        <>
          <Button onClick={onClose}>{tr("common.cancel")}</Button>
          <Button variant="primary" loading={busy} disabled={busy} onClick={() => void submit()}>
            {tr(order ? "orderModal.saveChanges" : "orderModal.createOrder")}
          </Button>
        </>
      }
    >
      <form data-order-modal className="flex flex-col gap-4" onSubmit={submit}>
        {order ? (
          <strong>{order.itemName}</strong>
        ) : item ? (
          <div className="flex items-center justify-between gap-2">
            <strong>{item.name}</strong>
            <IconButton label={tr("orderModal.clearItem")} onClick={() => setItem(null)}>
              <X />
            </IconButton>
          </div>
        ) : (
          <div className="flex flex-col gap-1">
            <SearchInput label={tr("orderModal.searchItemsPlaceholder")} value={query} onChange={(e) => setQuery(e.target.value)} placeholder={tr("orderModal.searchItemsPlaceholder")} autoFocus />
            {matches.map((m) => (
              <Button key={m.id} variant="ghost" size="sm" className="justify-start" onClick={() => setItem(m)}>
                {m.name}
              </Button>
            ))}
          </div>
        )}
        {!order && (
          <Segmented
            label={tr("common.type")}
            active={type}
            onSelect={setType}
            items={[
              { key: "sell", label: tr("orderModal.sell") },
              { key: "buy", label: tr("orderModal.buy") },
            ]}
          />
        )}
        <div className="grid grid-cols-2 gap-3">
          <Field label={tr("common.price")}>
            <Readout currency="platinum" value={null}>
              <Input type="number" min={1} value={platinum || ""} placeholder={tr("orderModal.pricePlaceholder")} onChange={(e) => setPlatinum(Number(e.target.value))} />
            </Readout>
          </Field>
          <Field label={tr("common.quantity")}>
            <Input type="number" min={1} value={quantity || ""} onChange={(e) => setQuantity(Number(e.target.value))} />
          </Field>
        </div>
        {!order && item && hasRanks(item) && (
          <Field label={tr("common.rank")}>
            <Input type="number" min={0} max={item.maxRank ?? 0} value={rank} onChange={(e) => setRank(Number(e.target.value))} />
          </Field>
        )}
        {subtypes && (
          <Field label={tr("orderModal.variant")}>
            <Select value={subtype} options={subtypes.map((v) => [v, v] as const)} onChange={(e) => setSubtype(e.target.value)} />
          </Field>
        )}
        <Checkbox label={tr("orderModal.visibleOnSite")} checked={visible} onChange={(e) => setVisible(e.target.checked)} />
        {error && (
          <div className="text-sm text-danger" role="alert">
            {error}
          </div>
        )}
      </form>
    </Modal>
  );
}
