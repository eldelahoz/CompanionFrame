import { useEffect, useState } from "react";
import type { Translator } from "../../lib/i18n";
import { appInfo, type Settings } from "../../lib/ipc";
import { checkForUpdate, useUpdater } from "../../stores/updater";
import { Button, SettingRow, Switch } from "../ui";

/** Automatic-check switch, a manual check with its result, and the running version. */
export default function UpdatesCard({ tr, settings, onChange }: { tr: Translator; settings: Settings; onChange: (patch: Partial<Settings>) => unknown }) {
  const u = useUpdater();
  const [version, setVersion] = useState("?");
  useEffect(() => void appInfo().then((i) => setVersion(i.version)), []);
  const status =
    u.status === "checking" ? tr("update.checking")
    : u.status === "uptodate" ? tr("update.upToDate")
    : u.status === "error" ? tr("update.failed", { reason: u.error ?? "" })
    : u.version ? tr("update.available", { version: u.version })
    : "";
  return (
    <>
      <SettingRow
        htmlFor="settings-auto-update"
        label={tr("update.auto")}
        description={tr("update.autoDesc")}
        control={<Switch id="settings-auto-update" checked={settings.auto_update_check} aria-label={tr("update.auto")} onChange={(e) => void onChange({ auto_update_check: e.target.checked })} />}
      />
      <SettingRow
        label={tr("update.current", { version })}
        description={<span role="status" className={u.status === "error" ? "text-danger" : undefined} data-update-status>{status}</span>}
        control={
          <Button size="sm" loading={u.status === "checking"} disabled={u.status === "checking" || u.status === "downloading" || u.status === "installing"} onClick={() => void checkForUpdate(true)}>
            {tr("update.checkNow")}
          </Button>
        }
      />
    </>
  );
}
