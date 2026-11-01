import { Copy, RefreshCw, ScanSearch } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import type { Translator } from "../../lib/i18n";
import { captureGameWindowInfo, eelogStatus, ocrOnnxStatus, rewardScanDebug, type GameWindowInfo, type OnnxStatus, type EeLogStatus, type ScanDebug } from "../../lib/ipc";
import { Badge, Button, Skeleton } from "../ui";

type Load<T> = { data: T } | { error: string } | null;
type Rows = Array<[label: string, value: string]>;
export interface Group {
  id: string;
  title: string;
  rows: Rows;
  error?: string;
}

const settle = async <T,>(p: Promise<T>): Promise<Load<T>> => p.then((data) => ({ data }), (e) => ({ error: e instanceof Error ? e.message : String(e) }));

/** Only counts, paths and flags: none of these commands returns tokens, account ids or nonces. */
export function diagnosticsGroups(
  tr: Translator,
  s: { ee: Load<EeLogStatus>; ocr: Load<OnnxStatus>; win: Load<GameWindowInfo>; scan: Load<ScanDebug | null> },
): Group[] {
  const yn = (b: boolean) => tr(b ? "diag.yes" : "diag.no");
  const none = tr("diag.none");
  const g = (id: string, title: string, l: Load<unknown>, rows: () => Rows): Group => ({ id, title, rows: l && "data" in l ? rows() : [], error: l && "error" in l ? l.error : undefined });
  const ee = s.ee && "data" in s.ee ? s.ee.data : null;
  const ocr = s.ocr && "data" in s.ocr ? s.ocr.data : null;
  const win = s.win && "data" in s.win ? s.win.data : null;
  const scan = s.scan && "data" in s.scan ? s.scan.data : null;
  return [
    g("eelog", tr("diag.eelog"), s.ee, () => [
      [tr("diag.eelogPath"), ee?.path ?? none],
      [tr("diag.eelogActive"), yn(!!ee?.active)],
      [tr("diag.eelogBytes"), String(ee?.bytes_read ?? 0)],
      [tr("diag.eelogDbwin"), yn(!!ee?.dbwin_active)],
    ]),
    g("ocr", tr("diag.ocr"), s.ocr, () => [
      [tr("diag.ocrReady"), yn(!!ocr?.ready)],
      [tr("diag.ocrRuntime"), yn(!!ocr?.runtimePresent)],
      [tr("diag.ocrPaddle"), yn(!!ocr?.paddlePresent)],
      [tr("diag.ocrDetector"), yn(!!ocr?.detectorPresent)],
      [tr("diag.ocrError"), ocr?.error ?? none],
    ]),
    g("window", tr("diag.window"), s.win, () => [
      [tr("diag.winFound"), yn(!!win?.found)],
      [tr("diag.winSize"), `${win?.width} x ${win?.height}`],
      [tr("diag.winPos"), `${win?.x}, ${win?.y}`],
      [tr("diag.winDpi"), String(win?.dpi)],
      [tr("diag.winSource"), win?.source ?? none],
      [tr("diag.winBlack"), yn(!!win?.black)],
      [tr("diag.winError"), win?.error ?? none],
    ]),
    g("scan", tr("diag.scan"), s.scan, () =>
      scan ? [[tr("diag.scanCards"), String(scan.cards)], ...scan.slots.map((t, i): [string, string] => [tr("diag.scanSlot", { n: i + 1 }), t.join(" | ") || none])] : [[tr("diag.scanCards"), tr("diag.scanNone")]],
    ),
  ];
}

/** Swaps the home folder for a marker so a pasted issue does not leak the user name. */
export const redactHome = (t: string): string =>
  t.replace(/[A-Za-z]:[\\/]+Users[\\/]+[^\\/\r\n]+/gi, "%USERPROFILE%").replace(/\/(?:home|Users)\/[^/\r\n]+/g, "~");

export const diagnosticsText = (groups: Group[]): string =>
  redactHome(
    groups
      .filter((g) => g.rows.length || g.error)
      .map((g) => [`## ${g.title}`, ...(g.error ? [g.error] : g.rows.map(([k, v]) => `${k}: ${v}`))].join("\n"))
      .join("\n\n"),
  );

/** Support tools kept from the port: log tail, OCR, game window and last reward scan, copyable into an issue. */
export default function DiagnosticsCard({ tr }: { tr: Translator }) {
  const [state, setState] = useState<{ ee: Load<EeLogStatus>; ocr: Load<OnnxStatus>; win: Load<GameWindowInfo>; scan: Load<ScanDebug | null> }>({ ee: null, ocr: null, win: null, scan: null });
  const [busy, setBusy] = useState(true);
  const [scanning, setScanning] = useState(false);
  const [note, setNote] = useState("");

  const refresh = useCallback(async () => {
    setBusy(true);
    const [ee, ocr, win] = await Promise.all([settle(eelogStatus()), settle(ocrOnnxStatus()), settle(captureGameWindowInfo())]);
    setState((s) => ({ ...s, ee, ocr, win }));
    setBusy(false);
  }, []);
  useEffect(() => void refresh(), [refresh]);

  const runScan = async () => {
    setScanning(true);
    const scan = await settle(rewardScanDebug());
    setState((s) => ({ ...s, scan }));
    setScanning(false);
  };

  const groups = diagnosticsGroups(tr, state);
  const copy = async () => {
    try {
      if (!navigator.clipboard?.writeText) throw new Error();
      await navigator.clipboard.writeText(diagnosticsText(groups));
      setNote(tr("common.copied"));
    } catch {
      setNote(tr("common.copyFailed"));
    }
  };

  return (
    <section className="flex flex-col gap-2" data-diagnostics-card aria-busy={busy}>
      <p className="m-0 text-xs text-text-muted">{tr("diag.desc")}</p>
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" icon={<RefreshCw />} loading={busy} onClick={() => void refresh()}>
          {tr("common.refresh")}
        </Button>
        <Button size="sm" icon={<ScanSearch />} loading={scanning} onClick={() => void runScan()}>
          {tr("diag.runScan")}
        </Button>
        <Button size="sm" icon={<Copy />} disabled={busy} onClick={() => void copy()}>
          {tr("diag.copy")}
        </Button>
        <span role="status" className="text-xs text-text-muted">
          {note}
        </span>
      </div>
      {groups.map((g) => (
        <div key={g.id} data-diag-group={g.id} className="grid gap-1">
          <h4 className="m-0 text-sm font-semibold text-text-primary">{g.title}</h4>
          {g.error ? (
            <p role="alert" className="m-0 text-sm text-danger">
              {tr("diag.error", { reason: g.error })}
            </p>
          ) : g.rows.length ? (
            <dl className="m-0 grid gap-1">
              {g.rows.map(([k, v]) => (
                <div key={k} className="flex items-center justify-between gap-4 border-b border-dashed border-surface-hover py-1 text-sm">
                  <dt className="text-text-secondary">{k}</dt>
                  <dd className="m-0 min-w-0 break-all text-right text-text-primary">{v === tr("diag.yes") || v === tr("diag.no") ? <Badge tone={v === tr("diag.yes") ? "success" : "neutral"}>{v}</Badge> : v}</dd>
                </div>
              ))}
            </dl>
          ) : g.id === "scan" ? (
            <p className="m-0 text-sm text-text-muted">{tr("diag.scanHint")}</p>
          ) : (
            <Skeleton height={16} />
          )}
        </div>
      ))}
    </section>
  );
}
