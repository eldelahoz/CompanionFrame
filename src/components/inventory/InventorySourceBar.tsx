import { useEffect, useState } from "react";
import { Download, Trash2, Upload } from "lucide-react";
import { Button } from "../ui";
import type { Translator } from "../../lib/i18n";
import {
  clearInventory,
  exportInventory,
  importInventoryPath,
  onFilesDropped,
  openAlecaFrameInventoryFile,
  openInventoryFile,
  type InventoryStatus,
} from "../../lib/ipc";
import { freshnessText, freshnessTone, staleHint } from "../../lib/inventoryFreshness";
import { describeInventorySource } from "../../lib/inventorySourceLabel";
import { memoryMode } from "../../stores/dataSource";

interface Props {
  status: InventoryStatus | null;
  tr: Translator;
  /** False when the caller lists the import sources itself (data-source panel). */
  imports?: boolean;
}

const TONE = { success: "bg-success", warning: "bg-warning", danger: "bg-danger", muted: "bg-text-muted" } as const;

/** Freshness of the imported snapshot, the import actions (picker, AlecaFrame, drop), export and the last read error. */
export default function InventorySourceBar({ status, tr, imports = true }: Props) {
  const [dropping, setDropping] = useState(false);
  const memory = memoryMode.use();
  const now = Date.now();
  const { labelKey, detail, path } = describeInventorySource(status?.source ?? "none", status?.path ?? null);
  const hint = staleHint(status);

  // Dropped files arrive as paths from the webview; one at a time is enough for an inventory.
  useEffect(() => {
    const off = onFilesDropped((paths) => {
      setDropping(false);
      if (paths[0]) void importInventoryPath(paths[0]);
    });
    return () => void off.then((f) => f());
  }, []);

  return (
    <div className="inv-source" data-inventory-source={status?.source ?? "none"} data-dropping={dropping || undefined}
      onDragEnter={() => setDropping(true)} onDragLeave={() => setDropping(false)} onDrop={() => setDropping(false)}>
      <span className="inv-source-label" title={path || undefined}>
        <span className={`status-dot ${TONE[freshnessTone(status, now)]}`} aria-hidden /> <strong data-inventory-freshness>{freshnessText(status, now, tr)}</strong>
        <span className="text-text-muted">, {tr(labelKey)}{detail ? `, ${detail}` : ""}</span>
        {status?.found && status.source === "aleca" && <span className="text-text-muted">, {tr("inventory.alecaAuto")}</span>}
      </span>
      <span className="inv-source-actions">
        {imports && (
          <>
            <Button size="sm" variant="primary" icon={<Upload />} title={tr("inventory.importHint")} data-inventory-import onClick={() => void openInventoryFile()}>
              {tr("inventory.import")}
            </Button>
            <Button size="sm" variant="ghost" onClick={() => void openAlecaFrameInventoryFile()}>
              {tr("setup.source.aleca.name")}
            </Button>
          </>
        )}
        <Button size="sm" variant="ghost" icon={<Download />} title={tr("settings.exportInventoryHint")} onClick={() => void exportInventory()}>
          {tr("settings.exportInventory")}
        </Button>
        {status?.found && (
          <Button size="sm" variant="ghost" icon={<Trash2 />} onClick={() => void clearInventory()}>
            {tr("inventory.clear")}
          </Button>
        )}
      </span>
      {hint && (
        <p className="inv-source-hint" role="status" data-inventory-stale>
          {tr(hint)}
        </p>
      )}
      {!status?.found && <p className="inv-source-hint">{tr("inventory.dropHint")}</p>}
      {imports && !memory && status?.found && <p className="inv-source-hint" data-passive-tracks>{tr("inventory.passiveTracks")}</p>}
      {status?.lastError && (
        <p className="inv-source-error" role="alert" data-inventory-error={status.lastError.kind}>
          {status.lastError.message}
        </p>
      )}
    </div>
  );
}
