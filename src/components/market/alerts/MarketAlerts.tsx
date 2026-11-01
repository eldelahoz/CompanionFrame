import { BellOff, BellRing, Download, FlaskConical, Pencil, Plus, Timer, Trash2, Upload } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import type { Translator } from "../../../lib/i18n";
import {
  marketAlertsClearCooldown,
  marketAlertsDelete,
  marketAlertsExport,
  marketAlertsGet,
  marketAlertsHitsClear,
  marketAlertsHitsGet,
  marketAlertsImport,
  marketAlertsSetEnabled,
  marketAlertsStatus,
  marketAlertsTest,
  type MarketAlertEngineStatus,
  type MarketAlertEntry,
  type MarketAlertHit,
} from "../../../lib/ipc";
import { useWfmCatalog } from "../../../stores/market";
import { Badge, Button, Card, CardHeader, EmptyState, IconButton, Switch, Table } from "../../ui";
import { askConfirm } from "../../../stores/confirm";
import AlertRuleEditor from "./AlertRuleEditor";

const POLL_MS = 30_000;

/** Rule list, engine health and hit history. The backend owns evaluation; this tab only edits rules and reads results. */
export default function MarketAlerts({ tr, locale = "en" }: { tr: Translator; locale?: string }) {
  const [entries, setEntries] = useState<MarketAlertEntry[]>([]);
  const [hits, setHits] = useState<MarketAlertHit[]>([]);
  const [status, setStatus] = useState<MarketAlertEngineStatus | null>(null);
  const [editing, setEditing] = useState<{ entry: MarketAlertEntry | null } | null>(null);
  const [notice, setNotice] = useState("");
  const { catalog } = useWfmCatalog();
  const fileInput = useRef<HTMLInputElement>(null);

  const load = useCallback(async () => {
    try {
      const [e, h, s] = await Promise.all([marketAlertsGet(), marketAlertsHitsGet(), marketAlertsStatus()]);
      setEntries(e);
      setHits(h);
      setStatus(s);
    } catch (err) {
      setNotice((err as Error).message);
    }
  }, []);
  useEffect(() => {
    void load();
    const t = setInterval(() => !document.hidden && void load(), POLL_MS);
    return () => clearInterval(t);
  }, [load]);

  const act = (run: () => Promise<unknown>) => async () => {
    try {
      await run();
      await load();
    } catch (err) {
      setNotice((err as Error).message);
    }
  };
  const testFire = async (id: string) => {
    try {
      const r = await marketAlertsTest(id);
      setNotice(tr("marketAlerts.testResult", { count: r.matches, detail: r.detail }));
    } catch (err) {
      setNotice(tr("marketAlerts.testFailed", { error: (err as Error).message }));
    }
  };
  const exportRules = async () => {
    try {
      const url = URL.createObjectURL(new Blob([await marketAlertsExport()], { type: "application/json" }));
      const link = document.createElement("a");
      link.href = url;
      link.download = `companionframe-market-alerts-${new Date().toISOString().slice(0, 10)}.json`;
      document.body.appendChild(link);
      link.click();
      link.remove();
      URL.revokeObjectURL(url);
    } catch (err) {
      setNotice(tr("marketAlerts.exportFailed", { error: (err as Error).message }));
    }
  };
  const importRules = async (file: File | undefined) => {
    if (!file || !(await askConfirm(tr("marketAlerts.importConfirm", { name: file.name })))) return;
    try {
      setNotice(tr("marketAlerts.imported", { count: await marketAlertsImport(await file.text()) }));
      await load();
    } catch (err) {
      setNotice(tr("marketAlerts.importFailed", { error: (err as Error).message }));
    }
  };
  const time = (iso: string) => new Date(iso).toLocaleString(locale);

  return (
    <div className="flex flex-col gap-4" data-market-alerts>
      <div className="flex flex-wrap items-center gap-3">
        <h2 className="m-0 mr-auto text-lg font-semibold">{tr("marketAlerts.title")}</h2>
        {status && (
          <span className="flex items-center gap-2 text-xs text-text-muted" data-alerts-status>
            <Badge tone={status.running ? "success" : "danger"}>{status.running ? tr("marketAlerts.healthOk") : tr("marketAlerts.engineStopped")}</Badge>
            {tr("marketAlerts.enabledCount", { enabled: status.enabledCount, total: status.ruleCount })}, {tr("marketAlerts.requestsLastHour", { count: status.requestsLastHour })}
          </span>
        )}
        <Button size="sm" icon={<Upload />} data-alert-import onClick={() => fileInput.current?.click()}>
          {tr("marketAlerts.import")}
        </Button>
        <input
          ref={fileInput}
          className="hidden"
          type="file"
          accept="application/json,.json"
          data-alert-import-file
          onChange={(e) => {
            const file = e.target.files?.[0];
            e.target.value = "";
            void importRules(file);
          }}
        />
        <Button size="sm" icon={<Download />} disabled={entries.length === 0} data-alert-export onClick={() => void exportRules()}>
          {tr("marketAlerts.export")}
        </Button>
        <Button variant="primary" size="sm" icon={<Plus />} data-alert-new onClick={() => setEditing({ entry: null })}>
          {tr("marketAlerts.newRule")}
        </Button>
      </div>
      {notice && (
        <p className="m-0 text-sm text-text-secondary" role="status">
          {notice}
        </p>
      )}
      {entries.length === 0 ? (
        <Card>
          <EmptyState icon={<BellOff />} title={tr("marketAlerts.noRules")} />
        </Card>
      ) : (
        <Card className="overflow-hidden">
          <Table mode="stack" label={tr("marketAlerts.title")}>
            <tbody>
              {entries.map((e) => {
                const baro = e.rule.kind === "baro";
                const cooling = e.cooldownUntil != null && e.cooldownUntil > Date.now();
                return (
                  <tr key={e.rule.id} data-alert-rule={e.rule.id}>
                    <td data-label={tr("marketAlerts.enabled")}>
                      <Switch checked={e.rule.enabled} aria-label={tr("marketAlerts.enabled")} onChange={act(() => marketAlertsSetEnabled(e.rule.id, !e.rule.enabled))} />
                    </td>
                    <td data-primary>
                      <div className="flex flex-wrap items-center gap-2">
                        <strong>{e.rule.name}</strong>
                        <Badge tone="accent">{e.rule.kind}</Badge>
                        {baro && <span className="text-xs text-text-muted">{tr("marketAlerts.baroDeferred")}</span>}
                      </div>
                      <div className="mt-1 text-xs text-text-muted">
                        {e.rule.noCooldown ? tr("marketAlerts.noCooldown") : tr("marketAlerts.cooldownShort", { minutes: e.rule.cooldownMinutes })}
                        {cooling && `, ${tr("marketAlerts.cooldownLeft", { minutes: Math.ceil((e.cooldownUntil! - Date.now()) / 60_000) })}`}
                      </div>
                    </td>
                    <td data-actions>
                      <div className="flex items-center justify-end gap-1">
                        {cooling && (
                          <Button size="sm" icon={<Timer />} onClick={act(() => marketAlertsClearCooldown(e.rule.id))}>
                            {tr("marketAlerts.clearCooldown")}
                          </Button>
                        )}
                        <Button size="sm" icon={<FlaskConical />} disabled={baro} onClick={() => void testFire(e.rule.id)}>
                          {tr("marketAlerts.testFire")}
                        </Button>
                        <IconButton label={tr("market.edit")} disabled={baro} onClick={() => setEditing({ entry: e })}>
                          <Pencil />
                        </IconButton>
                        <IconButton label={tr("common.delete")} danger onClick={async () => (await askConfirm(tr("marketAlerts.deleteConfirm"))) && void act(() => marketAlertsDelete(e.rule.id))()}>
                          <Trash2 />
                        </IconButton>
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </Table>
        </Card>
      )}

      <Card>
        <CardHeader
          title={tr("marketAlerts.hitHistory")}
          tools={
            hits.length > 0 ? (
              <Button size="sm" onClick={async () => (await askConfirm(tr("marketAlerts.clearHitsConfirm"))) && void act(marketAlertsHitsClear)()}>
                {tr("marketAlerts.clearHits")}
              </Button>
            ) : undefined
          }
        />
        {hits.length === 0 ? (
          <EmptyState icon={<BellRing />} title={tr("marketAlerts.noHits")} />
        ) : (
          <div className="flex flex-col">
            {[...hits]
              .sort((a, b) => b.at.localeCompare(a.at))
              .map((h) => (
                <a key={h.id} href={h.url} target="_blank" rel="noreferrer" className="flex flex-col gap-1 border-t border-border px-4 py-3 text-sm no-underline hover:bg-bg-hover" data-alert-hit={h.id}>
                  <span>
                    <strong className="text-text-primary">{h.title}</strong>
                    <span className="ml-2 text-xs text-text-muted">
                      {h.ruleName}, {time(h.at)}
                    </span>
                  </span>
                  <span className="text-xs text-text-secondary">{h.detail}</span>
                </a>
              ))}
          </div>
        )}
      </Card>
      {editing && (
        <AlertRuleEditor
          entry={editing.entry}
          catalog={catalog}
          tr={tr}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            void load();
          }}
        />
      )}
    </div>
  );
}
