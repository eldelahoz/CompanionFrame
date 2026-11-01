import { useEffect, useState } from "react";
import type { Translator } from "../../lib/i18n";
import AppDisclaimer from "./AppDisclaimer";
import { appInfo, getRelicDataInfo, onRelicDbUpdated, type RelicDataInfo } from "../../lib/ipc";

/** Date of a downloaded relic data set, so a bundled one never claims a publish date. */
export function relicDataLabel(tr: Translator, locale: string, info: RelicDataInfo | null): string | null {
  if (!info?.version) return null;
  if (info.updateOffline) return tr("settings.creditRelicDataOffline", { version: info.version });
  if (info.updateError) return tr("settings.creditRelicDataFailed", { version: info.version, reason: info.updateError });
  const published = info.source === "downloaded" ? info.publishedAt : null;
  const date = published && new Date(published).toLocaleDateString(locale, { year: "numeric", month: "short", day: "numeric" });
  const label = date ? tr("settings.creditRelicDataValue", { version: info.version, date }) : info.version;
  // The version in use is the newest usable one: newer npm releases ship without relic rewards.
  return info.newerUnusable ? tr("settings.creditRelicDataNewerUnusable", { version: label }) : label;
}

/** Version and data credits (no links: the app has no external opener, and the data sources are public). */
export default function AboutCard({ tr, locale }: { tr: Translator; locale: string }) {
  const [version, setVersion] = useState("?");
  const [relics, setRelics] = useState<RelicDataInfo | null>(null);

  useEffect(() => {
    void appInfo().then((i) => setVersion(i.version));
    const load = () => void getRelicDataInfo().then(setRelics).catch(() => {});
    load();
    const off = onRelicDbUpdated(load);
    return () => void off.then((f) => f());
  }, []);

  const relicValue = relicDataLabel(tr, locale, relics);
  const rows: Array<{ id?: string; label: string; value: string }> = [
    { label: tr("settings.creditPrices"), value: "warframe.market" },
    { label: tr("settings.creditGameData"), value: tr("settings.creditGameDataValue") },
    { label: tr("settings.creditItemDropData"), value: tr("settings.creditWfcd") },
    ...(relicValue ? [{ id: "relic-data", label: tr("settings.creditRelicData"), value: relicValue }] : []),
    { label: tr("settings.creditArbiStats"), value: tr("settings.creditArbiStatsValue") },
    { label: tr("settings.creditArbiMaps"), value: "arbi.guide (remesis)" },
    { label: tr("settings.creditInventorySnapshots"), value: "warframe-api-helper" },
  ];
  return (
    <section className="flex flex-col gap-1" data-about-card>
      <div className="flex items-center justify-between">
        <h3 className="m-0 font-display text-lg font-semibold text-text-primary">{tr("settings.aboutTitle")}</h3>
        <span className="rounded bg-bg-raised px-2 py-0.5 font-display text-xs font-semibold text-text-secondary">v{version}</span>
      </div>
      <p className="m-0 text-sm text-text-muted">{tr("settings.aboutDesc")}</p>
      <p className="m-0 text-xs text-text-muted" data-about-disclaimer>
        <AppDisclaimer tr={tr} />
      </p>
      <p className="m-0 text-xs text-text-muted">{tr("dataSource.disclaimer")}</p>
      <div className="mt-2 grid gap-1">
        {rows.map((r) => (
          <div key={r.label} data-credit={r.id} className="flex items-center justify-between gap-4 border-b border-dashed border-surface-hover py-1 text-sm">
            <span className="text-text-secondary">{r.label}</span>
            <span className={r.id === "relic-data" && relics?.updateError && !relics.updateOffline ? "text-danger" : "text-text-primary"}>{r.value}</span>
          </div>
        ))}
      </div>
    </section>
  );
}
