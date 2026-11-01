import { useCallback, useEffect, useState } from "react";
import type { Translator } from "../../lib/i18n";
import { ocrLanguageInstall, ocrLanguageStatus, onItemDbUpdated, type OcrInstallOutcome, type OcrLanguageStatus } from "../../lib/ipc";
import { gameLanguageName } from "../../lib/gameLanguage";
import { Button } from "../ui";

const PROBLEM = {
  cancelled: "settings.ocrInstallCancelled",
  failed: "settings.ocrInstallFailed",
  timeout: "settings.ocrInstallTimeout",
  unsupported: "settings.ocrInstallFailed",
} as const;

/**
 * Optional: Windows text recognition for the game language as a second opinion next to the
 * built-in reader, offered when it is missing. The install
 * is one elevated command (a UAC prompt) and only ever starts from this button.
 */
export default function OcrLanguageNotice({ tr }: { tr: Translator }) {
  const [status, setStatus] = useState<OcrLanguageStatus | null>(null);
  const [phase, setPhase] = useState<"idle" | "installing" | OcrInstallOutcome>("idle");

  const load = useCallback(() => void ocrLanguageStatus().then(setStatus).catch(() => {}), []);
  useEffect(() => {
    load();
    const off = onItemDbUpdated(load);
    return () => void off.then((f) => f());
  }, [load]);

  if (!status) return null;
  const language = gameLanguageName(status.gameLanguage);
  if (status.installed) {
    return phase === "installed" ? (
      <p role="status" className="m-0 text-sm text-success">
        {tr("settings.ocrInstalled", { language })}
      </p>
    ) : null;
  }

  const install = async () => {
    setPhase("installing");
    const outcome = await ocrLanguageInstall().catch((): OcrInstallOutcome => "failed");
    setPhase(outcome);
    if (outcome === "installed") load();
  };
  const installing = phase === "installing";

  return (
    <div className="flex flex-col gap-2" data-ocr-missing>
      <p role="status" className="m-0 text-sm text-text-secondary">
        {tr("settings.ocrLanguageMissing", { language })}
      </p>
      {status.installable && (
        <div className="flex flex-wrap items-center gap-3">
          <Button size="sm" variant="primary" loading={installing} disabled={installing} onClick={() => void install()}>
            {tr("settings.ocrInstall", { language })}
          </Button>
          {installing && <span aria-live="polite" className="text-xs text-text-secondary">{tr("settings.ocrInstalling")}</span>}
        </div>
      )}
      {phase !== "idle" && phase !== "installing" && phase !== "installed" && (
        <p role="alert" className="m-0 text-sm text-danger">
          {tr(PROBLEM[phase], { language })}
        </p>
      )}
    </div>
  );
}
