import { Check, RefreshCw, SearchX } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Button, Card, Checkbox, Chip, EmptyState, Field, Meter, SearchInput, Select, Skeleton } from "../ui";
import { getCodexScans, onInventoryUpdated, type CodexScansView } from "../../lib/ipc";
import { buildCodexRows, CODEX_FACTIONS, enemyImageUrl, sortCodexRows, type CodexRow, type CodexSortKey } from "../../lib/codexScans";
import type { MessageKey, Translator } from "../../lib/i18n";
import { useClock } from "../../stores/world";

const ERROR_KEY: Record<NonNullable<CodexScansView["error"]>, MessageKey> = {
  "no-account": "codex.noAccount",
  "no-data": "codex.noData",
  "fetch-failed": "codex.fetchFailed",
};

function Entry({ row }: { row: CodexRow }) {
  const [broken, setBroken] = useState(false);
  const url = broken ? null : enemyImageUrl(row.image);
  const pct = row.required ? Math.min(100, (row.scanned / row.required) * 100) : 0;
  return (
    <div className="ms-codex-entry" data-codex-entry={row.type} title={row.type}>
      <div className="ms-codex-art">
        {url ? <img src={url} alt="" loading="lazy" onError={() => setBroken(true)} /> : <span>{row.name.slice(0, 1)}</span>}
        {row.complete && (
          <span className="ms-codex-done">
            <Check aria-hidden="true" />
          </span>
        )}
      </div>
      <div className="ms-codex-body">
        <span className="ms-card-name" title={row.name}>
          {row.name}
        </span>
        <span className={`ms-sub tabular-nums ${row.complete ? "text-success" : "ms-sub--muted"}`}>
          {row.scanned}
          {row.required !== null ? ` / ${row.required}` : ""}
        </span>
        {row.required !== null && <Meter value={pct} tone={row.complete ? "success" : "accent"} />}
      </div>
    </div>
  );
}

export default function CodexPanel({ tr, locale }: { tr: Translator; locale: string }) {
  const [result, setResult] = useState<CodexScansView | null>(null);
  const [loading, setLoading] = useState(false);
  const [search, setSearch] = useState("");
  const [incompleteOnly, setIncompleteOnly] = useState(false);
  const [faction, setFaction] = useState("all");
  const [sortBy, setSortBy] = useState<CodexSortKey>("name");
  const busy = useRef(false);
  const now = useClock(1000);

  const load = useCallback(async (refresh = false) => {
    if (busy.current) return;
    busy.current = true;
    setLoading(true);
    try {
      setResult(await getCodexScans(refresh));
    } catch {
      setResult((r) => ({ fetchedAt: r?.fetchedAt ?? null, scans: r?.scans ?? null, nextRefreshAt: 0, error: "fetch-failed" }));
    } finally {
      busy.current = false;
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
    const off = onInventoryUpdated(() => void load());
    return () => void off.then((f) => f());
  }, [load]);

  const rows = useMemo(() => (result?.scans ? buildCodexRows(result.scans) : []), [result]);
  const factions = useMemo(() => CODEX_FACTIONS.filter((f) => rows.some((r) => r.faction === f.key)), [rows]);
  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    return sortCodexRows(
      rows.filter((r) => (faction === "all" || r.faction === faction) && (!incompleteOnly || r.complete === false) && (!q || r.name.toLowerCase().includes(q))),
      sortBy,
    );
  }, [rows, faction, incompleteOnly, search, sortBy]);

  const done = rows.filter((r) => r.complete === true).length;
  const known = rows.filter((r) => r.complete !== null).length;
  const wait = Math.max(0, Math.ceil(((result?.nextRefreshAt ?? 0) - now) / 1000));
  const error = result?.error ?? null;
  const [before, after] = tr("codex.attribution").split("{link}");

  return (
    <div className="grid gap-3" data-codex-panel>
      <div className="ms-filters ms-filters--start">
        <SearchInput className="ms-search" label={tr("codex.searchPlaceholder")} placeholder={tr("codex.searchPlaceholder")} value={search} onChange={(e) => setSearch(e.target.value)} />
        <Checkbox data-codex-incomplete label={tr("codex.incompleteOnly")} checked={incompleteOnly} onChange={(e) => setIncompleteOnly(e.target.checked)} />
        <Field label={tr("common.sort")}>
          <Select
            data-codex-sort
            value={sortBy}
            options={[["name", tr("common.name")], ["scans", tr("common.scans")], ["progress", tr("codex.sortProgress")]]}
            onChange={(e) => setSortBy(e.target.value as CodexSortKey)}
          />
        </Field>
        <div className="ms-filters-end">
          {result?.fetchedAt && (
            <span className="ms-sub ms-sub--muted" data-codex-updated>
              {tr("codex.updated", { when: new Date(result.fetchedAt).toLocaleString(locale) })}
            </span>
          )}
          <Button size="sm" data-codex-refresh disabled={loading || wait > 0} icon={<RefreshCw />} onClick={() => void load(true)}>
            {loading ? tr("codex.refreshing") : wait > 0 ? tr("profile.refreshWait", { seconds: wait }) : tr("common.refresh")}
          </Button>
        </div>
      </div>

      {error && rows.length > 0 && (
        <p role="alert" data-codex-error className="m-0 text-sm text-warning">
          {tr(ERROR_KEY[error])}
        </p>
      )}

      {rows.length > 0 && (
        <div className="flex flex-wrap items-center justify-between gap-2" data-tour="mastery-codex-factions">
          <div className="flex flex-wrap gap-1.5" role="group" aria-label={tr("common.filters")}>
            {[{ key: "all", label: tr("common.all") }, ...factions].map((f) => (
              <Chip key={f.key} pressed={faction === f.key} data-codex-faction={f.key} onClick={() => setFaction(f.key)}>
                {f.label}
              </Chip>
            ))}
          </div>
          <span className="ms-sub">{tr("codex.summary", { done: String(done), total: String(known) })}</span>
        </div>
      )}

      {!result || (loading && rows.length === 0) ? (
        <div className="ms-codex-grid" aria-busy="true">
          {[0, 1, 2, 3, 4, 5].map((i) => (
            <Skeleton key={i} height="10rem" />
          ))}
        </div>
      ) : rows.length === 0 && error ? (
        <Card>
          <EmptyState title={tr(ERROR_KEY[error])} />
        </Card>
      ) : filtered.length === 0 ? (
        <Card>
          <EmptyState icon={<SearchX />} title={tr("codex.empty")} />
        </Card>
      ) : (
        <div className="ms-codex-grid" data-tour="mastery-codex-list">
          {filtered.map((row) => (
            <Entry key={row.type} row={row} />
          ))}
        </div>
      )}

      <p className="ms-sub ms-sub--muted m-0">
        {before}
        <a className="text-text-link" href="https://wiki.warframe.com" target="_blank" rel="noreferrer">
          Warframe Wiki
        </a>
        {after ?? ""}
      </p>
    </div>
  );
}
