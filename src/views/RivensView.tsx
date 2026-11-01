import { Pin, PinOff, Search, Trash2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import HeaderTabs from "../components/HeaderTabs";
import { Button, Card, EmptyState, Field, IconButton, Input, PageHeader, Readout, SearchInput, Skeleton, Table } from "../components/ui";
import { formatNumber } from "../lib/format";
import type { Translator } from "../lib/i18n";
import {
  onRivenSessionSaved,
  rivenBestAttributes,
  rivensDelete,
  rivensGet,
  rivensPin,
  rivensSetNote,
  wfmRivenSearch,
  wfmRivenWeapons,
  type DecodedRivens,
  type ItemDatabase,
  type RivenBestAttributes,
  type RivenRollSession,
  type RivenWeapon,
  type WfmContract,
} from "../lib/ipc";
import { usePersisted } from "../lib/usePersisted";
import RivenInventory, { rivenCounts } from "../components/rivens/RivenInventory";

interface Props {
  tr: Translator;
  locale: string;
  items?: ItemDatabase;
  /** Persisted app setting for the detail modal's "similar auctions" chip. */
  similarAuctions?: boolean;
  onSimilarAuctions?: (shown: boolean) => void;
}

const TABS = ["unveiled", "veiled", "sessions", "finder"] as const;

// Pinned first, then newest first: what the backend returns, kept after a local change.
const order = (list: RivenRollSession[]) =>
  [...list].sort((a, b) => Number(b.pinned) - Number(a.pinned) || (b.startedAt ?? 0) - (a.startedAt ?? 0));

function SessionRow({ s, tr, locale, onChange }: { s: RivenRollSession; tr: Translator; locale: string; onChange: (s: RivenRollSession | null) => void }) {
  const [note, setNote] = useState(s.notes ?? "");
  return (
    <Card className="flex flex-col gap-3 p-4" data-riven-session={s.id}>
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <strong>{s.weapon || tr("rivens.sessions.unknownWeapon")}</strong>
        <span className="text-xs text-text-muted">{new Date(s.startedAt ?? 0).toLocaleString(locale)}</span>
        <span className="text-sm text-text-secondary">{tr("rivens.sessions.rolls", { count: s.rolls })}</span>
        <span className="text-sm text-text-secondary">{tr("rivens.sessions.kuva", { count: formatNumber(s.kuvaSpent, locale) })}</span>
        <span className="ml-auto flex gap-1">
          <IconButton
            label={s.pinned ? tr("rivens.sessions.unpin") : tr("rivens.sessions.pin")}
            data-riven-pin
            aria-pressed={s.pinned}
            onClick={async () => (await rivensPin(s.id, !s.pinned)) && onChange({ ...s, pinned: !s.pinned })}
          >
            {s.pinned ? <PinOff /> : <Pin />}
          </IconButton>
          <IconButton label={tr("common.delete")} danger data-riven-delete onClick={async () => (await rivensDelete(s.id)) && onChange(null)}>
            <Trash2 />
          </IconButton>
        </span>
      </div>
      <Input
        value={note}
        maxLength={2000}
        placeholder={tr("rivens.sessions.note")}
        aria-label={tr("rivens.sessions.note")}
        onChange={(e) => setNote(e.target.value)}
        onBlur={async () => {
          const stored = await rivensSetNote(s.id, note);
          if (stored !== null && stored !== (s.notes ?? "")) onChange({ ...s, notes: stored || null });
        }}
      />
    </Card>
  );
}

function Sessions({ tr, locale }: Props) {
  const [sessions, setSessions] = useState<RivenRollSession[] | null>(null);
  const [query, setQuery] = useState("");

  useEffect(() => {
    void rivensGet().then((l) => setSessions(order(l)));
    let off = () => {};
    let gone = false;
    void onRivenSessionSaved((s) => setSessions((cur) => order([s, ...(cur ?? []).filter((x) => x.id !== s.id)]))).then((u) => {
      if (gone) u();
      else off = u;
    });
    return () => {
      gone = true;
      off();
    };
  }, []);

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return (sessions ?? []).filter((s) => !q || `${s.weapon} ${s.notes ?? ""}`.toLowerCase().includes(q));
  }, [sessions, query]);

  if (!sessions) return <Skeleton height="6rem" />;
  if (sessions.length === 0)
    return (
      <Card>
        <EmptyState title={tr("rivens.sessions.empty")} />
      </Card>
    );
  const totals = sessions.reduce((t, s) => ({ rolls: t.rolls + s.rolls, kuva: t.kuva + s.kuvaSpent }), { rolls: 0, kuva: 0 });
  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-3">
        <SearchInput label={tr("common.searchPlaceholder")} placeholder={tr("common.searchPlaceholder")} value={query} onChange={(e) => setQuery(e.target.value)} />
        <span data-riven-totals className="text-sm text-text-muted">
          {tr("rivens.sessions.totals", { sessions: sessions.length, rolls: formatNumber(totals.rolls, locale), kuva: formatNumber(totals.kuva, locale) })}
        </span>
      </div>
      {shown.map((s) => (
        <SessionRow
          key={s.id}
          s={s}
          tr={tr}
          locale={locale}
          onChange={(next) => setSessions((cur) => order((cur ?? []).flatMap((x) => (x.id !== s.id ? [x] : next ? [next] : []))))}
        />
      ))}
    </div>
  );
}

function Finder({ tr, locale }: Props) {
  const [weapons, setWeapons] = useState<RivenWeapon[]>([]);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ best: RivenBestAttributes | null; listings: WfmContract[] } | null>(null);

  useEffect(() => {
    void wfmRivenWeapons().then(setWeapons, () => {});
  }, []);

  const weapon = weapons.find((w) => w.name.toLowerCase() === name.trim().toLowerCase());
  async function search() {
    if (!weapon) return;
    setBusy(true);
    try {
      const melee = weapon.rivenType === "Melee" || weapon.rivenType === "Zaw";
      const [best, listings] = await Promise.all([rivenBestAttributes(weapon.name, melee), wfmRivenSearch(weapon.slug).catch(() => [])]);
      setResult({ best, listings: listings.slice(0, 30) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-end gap-3">
        <Field label={tr("rivens.finder.weapon")}>
          <Input id="riven-weapon" list="riven-weapons" value={name} placeholder={tr("rivens.finder.weaponPlaceholder")} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && void search()} />
        </Field>
        <datalist id="riven-weapons">
          {weapons.map((w) => (
            <option key={w.id} value={w.name} />
          ))}
        </datalist>
        <Button variant="primary" icon={<Search />} data-riven-search disabled={!weapon || busy} loading={busy} onClick={search}>
          {busy ? tr("rivens.finder.searchingWfm") : tr("rivens.finder.searchWfm")}
        </Button>
      </div>
      {result?.best && (
        <Card className="grid gap-4 p-4 sm:grid-cols-2">
          <div data-riven-best="positives">
            <h4 className="m-0 mb-1 text-xs font-semibold text-success">{tr("rivens.finder.bestPositives")}</h4>
            <p className="m-0 text-sm">{result.best.positives.join(", ")}</p>
          </div>
          <div data-riven-best="negatives">
            <h4 className="m-0 mb-1 text-xs font-semibold text-danger">{tr("rivens.finder.bestNegatives")}</h4>
            <p className="m-0 text-sm">{result.best.negatives.join(", ")}</p>
          </div>
        </Card>
      )}
      {result &&
        (result.listings.length === 0 ? (
          <Card>
            <EmptyState title={tr("rivens.finder.noAuctions")} />
          </Card>
        ) : (
          <Card>
            <header className="ui-card-head">
              <h2>{tr("rivens.finder.resultsCount", { count: result.listings.length })}</h2>
            </header>
            <Table mode="stack" label={tr("rivens.finder.similarRivens")}>
              <tbody>
                {result.listings.map((c) => (
                  <tr key={c.id} data-riven-listing={c.id}>
                    <td data-label={tr("rivens.type.riven")}>
                      <strong>{[c.itemName, c.rivenSuffix].filter(Boolean).join(" ")}</strong>
                    </td>
                    <td data-label={tr("common.attributes")} className="text-text-secondary">
                      {c.stats.map((s) => `${s.positive === false ? "-" : "+"}${s.label}`).join(", ")}
                    </td>
                    <td data-label={tr("common.platinum")}>
                      <Readout currency="platinum" value={c.buyoutPlatinum ?? c.platinum} locale={locale} chip />
                    </td>
                    <td data-label={tr("rivens.finder.seller")} className="text-text-muted">
                      {c.seller}
                    </td>
                  </tr>
                ))}
              </tbody>
            </Table>
          </Card>
        ))}
    </div>
  );
}

export default function RivensView({ tr, locale, items, similarAuctions = true, onSimilarAuctions = () => {} }: Props) {
  const [tab, setTab] = usePersisted("cf_rivens_tab", TABS, "unveiled");
  const [decoded, setDecoded] = useState<DecodedRivens | null>(null);
  const counts = rivenCounts(decoded);
  return (
    <div className="rivens-view flex flex-col gap-4">
      <PageHeader title={tr("common.rivens")} />
      <HeaderTabs
        label={tr("common.rivens")}
        options={[
          { key: "unveiled", label: tr("rivens.tab.unveiled"), count: counts.unveiled },
          { key: "veiled", label: tr("rivens.tab.veiled"), count: counts.veiled },
          { key: "sessions", label: tr("rivens.tab.sessions") },
          { key: "finder", label: tr("rivens.tab.finder") },
        ]}
        activeKey={tab}
        onSelect={(k) => setTab(k as (typeof TABS)[number])}
      />
      {tab === "unveiled" || tab === "veiled" ? (
        <RivenInventory tab={tab} tr={tr} locale={locale} items={items} similarAuctions={similarAuctions} onSimilarAuctions={onSimilarAuctions} onData={setDecoded} />
      ) : tab === "sessions" ? (
        <Sessions tr={tr} locale={locale} />
      ) : (
        <Finder tr={tr} locale={locale} />
      )}
    </div>
  );
}
