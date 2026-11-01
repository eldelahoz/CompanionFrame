import { useCallback, useEffect, useState } from "react";
import type { Translator } from "../../lib/i18n";
import { rivensDecoded, wfmContractsMy, wfmRivenAuctionClose, wfmRivenAuctionUpdate, type DecodedRiven, type RivenAuctionUpdate, type WfmContract, type WfmContractsPage } from "../../lib/ipc";
import { contractInventoryMatch } from "../../lib/contractInventoryMatch";
import { FileX } from "lucide-react";
import { Button, Card, EmptyState, Field, Input, Modal, Skeleton, Table } from "../ui";
import { askConfirm } from "../../stores/confirm";
import { addToast } from "../../stores/toasts";
import MarketContractRow from "./MarketContractRow";

/** Everything the update call needs, from the row, so a price edit or a hide never wipes the other fields. */
export function contractUpdate(c: WfmContract, patch: { platinum?: number; visible?: boolean } = {}): RivenAuctionUpdate {
  const price = patch.platinum ?? c.platinum;
  const buyout = c.isDirectSell || c.buyoutPlatinum ? price : null;
  return {
    buyoutPrice: buyout,
    startingPrice: c.isDirectSell ? null : buyout == null ? price : c.startingPlatinum,
    minReputation: c.minimalReputation ?? 0,
    isPrivate: null,
    description: c.note ?? "",
    visible: patch.visible ?? c.visible,
  };
}

function PriceModal({ contract, onClose, onSaved, tr }: { contract: WfmContract; onClose: () => void; onSaved: () => void; tr: Translator }) {
  const [price, setPrice] = useState(contract.platinum);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const submit = async (e?: React.FormEvent) => {
    e?.preventDefault();
    if (!(price >= 1)) return setError(tr("rivens.detail.priceMin"));
    setBusy(true);
    try {
      await wfmRivenAuctionUpdate(contract.id, contractUpdate(contract, { platinum: price }));
      onSaved();
    } catch (err) {
      setError((err as Error).message);
      setBusy(false);
    }
  };
  return (
    <Modal
      title={tr("rivens.detail.editContract")}
      sub={contract.itemName}
      tr={tr}
      onClose={onClose}
      width="26rem"
      footer={
        <>
          <Button onClick={onClose}>{tr("common.cancel")}</Button>
          <Button variant="primary" loading={busy} disabled={busy} onClick={() => void submit()}>
            {tr("common.save")}
          </Button>
        </>
      }
    >
      <form data-contract-modal className="flex flex-col gap-3" onSubmit={submit}>
        <Field label={tr("common.price")}>
          <Input type="number" min={1} value={price || ""} onChange={(e) => setPrice(Number(e.target.value))} autoFocus />
        </Field>
        {error && (
          <p className="m-0 text-sm text-danger" role="alert">
            {error}
          </p>
        )}
      </form>
    </Modal>
  );
}

/** Own riven contracts. WFM may stop serving them (v1 only): the page then carries `unavailable` and the tab says why. */
export default function MarketContracts({ tr }: { tr: Translator }) {
  const [page, setPage] = useState<WfmContractsPage | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState<ReadonlySet<string>>(new Set());
  const [editing, setEditing] = useState<WfmContract | null>(null);
  // Until the riven list decodes nothing is proven dead, so no markers rather than all.
  const [owned, setOwned] = useState<DecodedRiven[]>([]);

  const load = useCallback(async () => {
    try {
      setPage(await wfmContractsMy());
      setError("");
    } catch (e) {
      setError((e as Error).message);
    }
  }, []);
  useEffect(() => {
    void load();
    void rivensDecoded().then((d) => setOwned(d.unveiled), () => {});
  }, [load]);

  const guarded = async (id: string, run: () => Promise<unknown>) => {
    if (busy.has(id)) return;
    setBusy((b) => new Set(b).add(id));
    try {
      await run();
      await load();
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

  if (error)
    return (
      <p className="text-danger" role="alert">
        {error}
      </p>
    );
  if (!page)
    return (
      <Card className="flex flex-col gap-4 p-4" aria-busy="true" aria-label={tr("market.loadingContracts")}>
        <Skeleton width="60%" height="1.1rem" />
        <Skeleton width="45%" height="1.1rem" />
      </Card>
    );
  if (page.unavailable)
    return (
      <p className="text-text-muted" data-contracts-unavailable>
        {page.unavailable}
      </p>
    );
  if (page.contracts.length === 0)
    return (
      <Card>
        <EmptyState icon={<FileX />} title={tr("market.noContracts")} />
      </Card>
    );
  return (
    <>
      <Card className="overflow-hidden">
        <Table mode="stack" label={tr("common.rivens")}>
          <thead>
            <tr>
              <th>{tr("common.rivens")}</th>
              <th>{tr("common.platinum")}</th>
              <th>MR</th>
              <th>{tr("common.type")}</th>
              <th>
                <span className="sr-only">{tr("common.actions")}</span>
              </th>
            </tr>
          </thead>
          <tbody>
        {page.contracts.map((c) => (
          <MarketContractRow
            key={`${c.id}:${c.platinum}:${c.visible}`}
            contract={c}
            inventoryMatch={owned.length > 0 ? contractInventoryMatch(c, owned) : null}
            busy={busy.has(c.id)}
            onEdit={setEditing}
            onToggleVisible={(x) => guarded(x.id, () => wfmRivenAuctionUpdate(x.id, contractUpdate(x, { visible: !x.visible })))}
            onRemove={async (x) => (await askConfirm(tr("market.riven.confirmRemove"))) && guarded(x.id, () => wfmRivenAuctionClose(x.id))}
            tr={tr}
          />
          ))}
          </tbody>
        </Table>
      </Card>
      {editing && (
        <PriceModal
          contract={editing}
          tr={tr}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            void load();
          }}
        />
      )}
    </>
  );
}
