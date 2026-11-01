import { useCallback, useEffect, useState } from "react";
import { Badge, Button, Input, SettingRow } from "../ui";
import type { Translator } from "../../lib/i18n";
import { getCodexScans, getProfileAccount, onProfileInventoryUpdated, setProfileAccount, type CodexScansView } from "../../lib/ipc";

/**
 * The public account ID for the profile endpoint. It is pasted by the user, never read from the game.
 * Saving fetches the profile right away (it also refreshes on its own every few hours) and the row
 * says whether the last fetch worked.
 */
export default function ProfileIdRow({ tr }: { tr: Translator }) {
  const [saved, setSaved] = useState<string | null>(null);
  const [text, setText] = useState("");
  const [error, setError] = useState(false);
  const [fetching, setFetching] = useState(false);
  const [sync, setSync] = useState<CodexScansView | null>(null);

  const loadSync = useCallback(() => void getCodexScans(false).then(setSync).catch(() => {}), []);
  useEffect(() => {
    void getProfileAccount().then((id) => {
      setSaved(id);
      setText(id ?? "");
      if (id) loadSync();
    });
    const off = onProfileInventoryUpdated(loadSync);
    return () => void off.then((f) => f());
  }, [loadSync]);

  const save = async () => {
    try {
      const id = await setProfileAccount(text);
      setSaved(id);
      setText(id ?? "");
      setError(false);
      setSync(null);
      if (!id) return;
      setFetching(true);
      setSync(await getCodexScans(true).catch(() => ({ fetchedAt: null, scans: null, error: "fetch-failed" as const, nextRefreshAt: 0 })));
    } catch {
      setError(true);
    } finally {
      setFetching(false);
    }
  };

  const failed = !!saved && sync?.error === "fetch-failed";
  const description = error
    ? tr("settings.profileIdInvalid")
    : fetching
      ? tr("settings.profileFetching")
      : failed
        ? tr("settings.profileFetchFailed")
        : saved && sync?.fetchedAt
          ? tr("settings.profileFetchedAt", { time: new Date(sync.fetchedAt).toLocaleString() })
          : saved
            ? tr("settings.profileIdSaved")
            : tr("settings.profileIdHelp");

  return (
    <SettingRow
      label={
        <span className="flex flex-wrap items-center gap-2">
          {tr("settings.profileId")}
          <Badge>{tr("dataSource.profile.scope")}</Badge>
        </span>
      }
      description={
        <span role={failed || error ? "alert" : undefined} className={failed || error ? "text-danger" : undefined}>
          {description}
        </span>
      }
      control={
        <span className="flex items-center gap-2">
          <Input value={text} placeholder={tr("settings.profileIdPlaceholder")} aria-label={tr("settings.profileId")} data-profile-id onChange={(e) => setText(e.target.value)} />
          <Button size="sm" loading={fetching} disabled={fetching} onClick={() => void save()}>
            {tr("common.save")}
          </Button>
        </span>
      }
    />
  );
}
