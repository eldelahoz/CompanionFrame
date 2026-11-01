import { ChevronLeft, ChevronRight, Link2, Pencil } from "lucide-react";
import { useEffect, useState } from "react";
import { workbenchCloseMatch, workbenchMatchTrade, type LedgerImportPreview, type LedgerPage, type LedgerPatch, type WorkbenchTradeMatch } from "../../lib/ipc";
import { askConfirm } from "../../stores/confirm";
import type { MessageKey, Translator } from "../../lib/i18n";
import { tradeItemLabel } from "../../lib/stats/tradeAnalytics";
import type { TradeEvent, TradeItem, TradeType } from "../../types/stats";
import { Badge, Button, Card, CardHeader, EmptyState, Field, IconButton, Input, Modal, Readout, SearchInput, Select, Skeleton, Table, type Tone } from "../ui";

const TYPE_KEYS: Record<TradeType, MessageKey> = { sale: "stats.filterSale", purchase: "stats.filterPurchase", trade: "stats.filterTrade" };
const TYPE_TONE: Record<TradeType, Tone> = { sale: "success", purchase: "danger", trade: "neutral" };

export function LedgerTable(props: {
  page: LedgerPage | null;
  loading: boolean;
  search: string;
  typeFilter: TradeType | "all";
  offset: number;
  limit: number;
  onSearch: (value: string) => void;
  onTypeFilter: (value: TradeType | "all") => void;
  onOffset: (value: number) => void;
  onEdit: (event: TradeEvent) => void;
  onMatch: (event: TradeEvent) => void;
  tr: Translator;
  locale: string;
}) {
  const { page, loading, search, typeFilter, offset, limit, tr, locale } = props;
  const rows = page?.events ?? [];
  const total = page?.total ?? 0;
  const itemLabel = (item: TradeItem): string => {
    const label = tradeItemLabel(item);
    const qualifier = label.secondary ?? (label.rank == null ? null : tr("browse.rankValue", { value: label.rank }));
    const name = qualifier ? `${label.primary} (${qualifier})` : label.primary || "?";
    return item.count > 1 ? `${item.count}x ${name}` : name;
  };
  const day = (iso: string) => {
    const d = new Date(iso);
    return Number.isNaN(d.getTime()) ? iso : d.toLocaleDateString(locale, { year: "numeric", month: "short", day: "numeric" });
  };
  return (
    <Card className="flex min-w-0 flex-1 flex-col" data-analysis-table>
      <CardHeader
        title={tr("analysis.ledger")}
        meta={<span data-analysis-table-count>{tr("analysis.rowsRange", { first: total === 0 ? 0 : offset + 1, last: Math.min(offset + rows.length, total), total })}</span>}
        tools={
          <>
            <IconButton label={tr("analysis.prevPage")} disabled={offset <= 0} onClick={() => props.onOffset(offset - limit)}>
              <ChevronLeft />
            </IconButton>
            <IconButton label={tr("common.next")} disabled={offset + limit >= total} onClick={() => props.onOffset(offset + limit)}>
              <ChevronRight />
            </IconButton>
          </>
        }
      />
      <div className="flex flex-wrap items-center gap-2 px-4 pb-3">
        <SearchInput className="min-w-[14rem] flex-1" label={tr("analysis.searchPlaceholder")} value={search} placeholder={tr("analysis.searchPlaceholder")} onChange={(e) => props.onSearch(e.target.value)} />
        <Select
          aria-label={tr("common.type")}
          value={typeFilter}
          options={[["all", tr("common.all")] as const, ...(Object.keys(TYPE_KEYS) as TradeType[]).map((t) => [t, tr(TYPE_KEYS[t])] as const)]}
          onChange={(e) => props.onTypeFilter(e.target.value as TradeType | "all")}
        />
      </div>
      {loading ? (
        <div className="flex flex-col gap-2 px-4 pb-4" aria-busy="true">
          <Skeleton height="1.6rem" />
          <Skeleton height="1.6rem" />
          <Skeleton height="1.6rem" />
        </div>
      ) : rows.length === 0 ? (
        <div data-analysis-table-empty>
          <EmptyState title={tr("analysis.noRows")} />
        </div>
      ) : (
        <Table mode="stack" label={tr("analysis.ledger")}>
          <thead>
            <tr>
              <th>{tr("arbi.col.date")}</th>
              <th>{tr("common.type")}</th>
              <th>{tr("common.item")}</th>
              <th>{tr("analysis.colPartner")}</th>
              <th className="ui-num">{tr("common.platinum")}</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {rows.map((event) => (
              <tr key={event.id} data-analysis-row={event.id}>
                <td className="whitespace-nowrap text-text-muted" data-label={tr("arbi.col.date")}>{day(event.date)}</td>
                <td data-label={tr("common.type")}>
                  <Badge tone={TYPE_TONE[event.type]}>{tr(TYPE_KEYS[event.type])}</Badge>
                </td>
                <td className="max-w-[24rem]" data-primary>
                  <span className="line-clamp-2">{event.items?.length ? event.items.map(itemLabel).join(", ") : "-"}</span>
                </td>
                <td className="max-w-[10rem] truncate text-text-secondary" data-label={tr("analysis.colPartner")}>{event.partner || "-"}</td>
                <td className="ui-num" data-label={tr("common.platinum")}>
                  <Readout currency="platinum" value={Math.round(event.platChange ?? 0)} locale={locale} sign />
                </td>
                <td data-actions>
                  <span className="flex items-center justify-end gap-2">
                    {event.source === "gdpr" && <Badge>{tr("analysis.source.gdpr")}</Badge>}
                    {event.type !== "trade" && !event.wfmClosed && (
                      <IconButton label={tr("analysis.matchListings")} onClick={() => props.onMatch(event)}>
                        <Link2 />
                      </IconButton>
                    )}
                    <IconButton label={tr("market.edit")} onClick={() => props.onEdit(event)}>
                      <Pencil />
                    </IconButton>
                  </span>
                </td>
              </tr>
            ))}
          </tbody>
        </Table>
      )}
    </Card>
  );
}

/** The warframe.market listings a stored trade would close, each closed by an explicit click (never automatic). */
export function MatchModal({ event, onClose, onClosed, tr }: { event: TradeEvent; onClose: () => void; onClosed?: () => void; tr: Translator }) {
  const [matches, setMatches] = useState<WorkbenchTradeMatch[] | null>(null);
  const [closed, setClosed] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    workbenchMatchTrade({ partner: event.partner ?? "", platChange: event.platChange, type: event.type, items: event.items.map(({ displayName, count, direction }) => ({ displayName, count, direction })) })
      .then((m) => live && setMatches(m))
      .catch((e: Error) => live && (setError(e.message), setMatches([])));
    return () => void (live = false);
  }, [event]);
  const close = async (m: WorkbenchTradeMatch) => {
    if (!(await askConfirm(tr("analysis.matchCloseConfirm", { item: m.itemName })))) return;
    setError(null);
    try {
      if (!(await workbenchCloseMatch(m, event.id))) return setError(tr("analysis.matchCloseFailed"));
      setClosed((c) => new Set(c).add(m.orderId));
      onClosed?.();
    } catch (e) {
      setError((e as Error).message);
    }
  };
  return (
    <Modal title={tr("analysis.matchTitle")} sub={tr("analysis.matchHint")} tr={tr} onClose={onClose} width="36rem" footer={<Button onClick={onClose}>{tr("common.close")}</Button>}>
      <div data-analysis-match className="flex flex-col gap-2">
        {matches === null ? (
          <Skeleton height="1.6rem" />
        ) : matches.length === 0 ? (
          <p className="m-0 text-sm text-text-muted" data-analysis-match-none>{tr("analysis.matchNone")}</p>
        ) : (
          matches.map((m) => (
            <div key={m.orderId} className="flex items-center gap-3 border-t border-border py-2 text-sm" data-analysis-match-row={m.orderId}>
              <span className="min-w-0 flex-1 truncate">{m.quantity > 1 ? `${m.quantity}x ` : ""}{m.itemName}</span>
              <Readout currency="platinum" value={m.platinum} />
              {closed.has(m.orderId) ? <Badge tone="success">{tr("analysis.matchClosed")}</Badge> : <Button size="sm" onClick={() => void close(m)}>{tr("analysis.matchClose")}</Button>}
            </div>
          ))
        )}
        {error && <p className="m-0 text-xs text-danger" data-analysis-match-error>{error}</p>}
      </div>
    </Modal>
  );
}

const nonNegative = (text: string): number | undefined => {
  const n = Number(text);
  return text.trim() !== "" && Number.isFinite(n) && n >= 0 ? n : undefined;
};

export function RowEditor({ event, saving, error, onSave, onClose, tr }: { event: TradeEvent; saving: boolean; error: string | null; onSave: (patch: LedgerPatch) => void; onClose: () => void; tr: Translator }) {
  const [plat, setPlat] = useState(String(event.platChange ?? 0));
  const [partner, setPartner] = useState(event.partner ?? "");
  const [type, setType] = useState<TradeType>(event.type);
  const [credits, setCredits] = useState(event.credits == null ? "" : String(event.credits));
  const [tax, setTax] = useState(event.tradeTax == null ? "" : String(event.tradeTax));
  const platN = nonNegative(plat);
  const creditsN = nonNegative(credits);
  const taxN = nonNegative(tax);
  const creditsBad = credits.trim() !== "" && creditsN === undefined;
  const taxBad = tax.trim() !== "" && taxN === undefined;
  const blocked = saving || platN === undefined || creditsBad || taxBad;
  const invalid = <span className="text-danger">{tr("analysis.invalidNumber")}</span>;
  const submit = () => {
    if (blocked) return;
    // Only changed fields go in the patch, so the backend never rewrites untouched values.
    // shortcut: a cleared credits/tax stays stored (the patch has no way to unset it), add a double-option when needed.
    const patch: LedgerPatch = {};
    if (platN !== event.platChange) patch.platChange = platN;
    if (partner.trim() !== (event.partner ?? "")) patch.partner = partner.trim();
    if (type !== event.type) patch.type = type;
    if (creditsN !== undefined && creditsN !== event.credits) patch.credits = creditsN;
    if (taxN !== undefined && taxN !== event.tradeTax) patch.tradeTax = taxN;
    onSave(patch);
  };
  return (
    <Modal
      title={tr("analysis.editTitle")}
      sub={tr("analysis.editHint")}
      tr={tr}
      onClose={onClose}
      width="32rem"
      footer={
        <>
          <Button onClick={onClose}>{tr("common.cancel")}</Button>
          <Button variant="primary" data-analysis-row-editor-save loading={saving} disabled={blocked} onClick={submit}>
            {saving ? tr("common.saving") : tr("common.save")}
          </Button>
        </>
      }
    >
      <div data-analysis-row-editor={event.id} className="flex flex-col gap-4">
        <Field label={tr("common.platinum")}>
          <Input type="number" min="0" value={plat} onChange={(e) => setPlat(e.target.value)} />
          {platN === undefined && invalid}
        </Field>
        <Field label={tr("analysis.colPartner")}>
          <Input value={partner} onChange={(e) => setPartner(e.target.value)} />
        </Field>
        <Field label={tr("common.type")}>
          <Select value={type} options={(Object.keys(TYPE_KEYS) as TradeType[]).map((t) => [t, tr(TYPE_KEYS[t])] as const)} onChange={(e) => setType(e.target.value as TradeType)} />
        </Field>
        <div className="grid grid-cols-2 gap-4">
          <Field label={tr("common.credits")} className="" >
            <span data-analysis-field="credits" className="contents">
              <Input type="number" min="0" value={credits} onChange={(e) => setCredits(e.target.value)} />
              {creditsBad && invalid}
            </span>
          </Field>
          <Field label={tr("analysis.tradeTax")}>
            <span data-analysis-field="tax" className="contents">
              <Input type="number" min="0" value={tax} onChange={(e) => setTax(e.target.value)} />
              {taxBad && invalid}
            </span>
          </Field>
        </div>
        {error && <p className="m-0 text-xs text-danger" data-analysis-row-editor-error>{error}</p>}
      </div>
    </Modal>
  );
}

const KIND_LABEL: Record<string, MessageKey> = {
  duplicate: "analysis.rowKind.duplicate",
  unresolved: "analysis.rowKind.unresolved",
  rejected: "analysis.rowKind.rejected",
  parsed: "analysis.rowKind.parsed",
};
const KIND_TONE: Record<string, Tone> = { parsed: "success", rejected: "danger" };
const COUNTS: { key: keyof LedgerImportPreview["counts"]; labelKey: MessageKey }[] = [
  { key: "parsed", labelKey: "analysis.importParsed" },
  { key: "duplicates", labelKey: "analysis.importDuplicates" },
  { key: "unresolved", labelKey: "analysis.importUnresolved" },
  { key: "rejected", labelKey: "analysis.importRejected" },
];

/** Step one picks a file by path (no native picker is wired yet); step two previews and applies the staged batch. */
export function ImportDialog({ preview, busy, error, onPreview, onApply, onClose, tr }: { preview: LedgerImportPreview | null; busy: boolean; error: string | null; onPreview: (path: string) => void; onApply: () => void; onClose: () => void; tr: Translator }) {
  const [path, setPath] = useState("");
  // Apply writes unresolved rows too: the trade happened, only its item name is outside the catalog.
  const applicable = preview ? preview.counts.parsed + preview.counts.unresolved : 0;
  return (
    <Modal
      title={tr("analysis.importTitle")}
      sub={preview ? tr("analysis.importPreviewFor", { file: preview.fileName }) : undefined}
      tr={tr}
      onClose={onClose}
      width="44rem"
      footer={
        <>
          {preview && applicable <= 0 && <span className="mr-auto text-xs text-text-muted">{tr("analysis.importNothing")}</span>}
          <Button onClick={onClose}>{tr("common.cancel")}</Button>
          {preview && (
            <Button variant="primary" data-analysis-import-apply loading={busy} disabled={busy || applicable <= 0} onClick={onApply}>
              {busy ? tr("common.saving") : tr("analysis.importApply", { count: applicable })}
            </Button>
          )}
        </>
      }
    >
      <div data-analysis-import-dialog className="flex flex-col gap-4">
        {!preview ? (
          <form className="flex gap-2" onSubmit={(e) => { e.preventDefault(); if (path.trim()) onPreview(path.trim()); }}>
            <Input className="min-w-0 flex-1" value={path} placeholder="GDPR export (.json / .csv) path" aria-label="GDPR export path" onChange={(e) => setPath(e.target.value)} />
            <Button type="submit" variant="primary" loading={busy} disabled={busy || !path.trim()}>{tr("analysis.import")}</Button>
          </form>
        ) : (
          <>
            <div className="flex flex-wrap gap-6" data-analysis-import-counts>
              {COUNTS.map(({ key, labelKey }) => (
                <div key={key} className="flex flex-col gap-1">
                  <span className="text-xs text-text-muted">{tr(labelKey)}</span>
                  <b className="text-xl tabular-nums">{preview.counts[key]}</b>
                </div>
              ))}
            </div>
            {preview.rows.length > 0 && (
              <section>
                <h3 className="m-0 mb-2 text-sm font-semibold">{tr("analysis.importSample")}</h3>
                <div className="flex flex-col">
                  {preview.rows.map((row, i) => (
                    <div key={`${row.date}-${i}`} className="flex flex-col gap-1 border-t border-border py-2">
                      <div className="flex items-baseline gap-2 text-xs">
                        <span className="shrink-0 tabular-nums text-text-muted">{row.date}</span>
                        <Badge tone={KIND_TONE[row.kind] ?? "warning"}>{tr(KIND_LABEL[row.kind] ?? "analysis.rowKind.parsed")}</Badge>
                        <span className="min-w-0 truncate" title={row.summary}>{row.summary}</span>
                      </div>
                      {row.reason && <span className="text-xs text-text-muted">{row.reason}</span>}
                    </div>
                  ))}
                </div>
              </section>
            )}
          </>
        )}
        {error && <p className="m-0 text-xs text-danger" data-analysis-import-error>{error}</p>}
      </div>
    </Modal>
  );
}
