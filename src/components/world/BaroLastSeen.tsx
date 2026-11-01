import type { Translator } from "../../lib/i18n";

interface Props {
  available: boolean;
  /** Last recorded visit that carried the item, ms; null when never recorded. */
  lastRecorded: number | null;
  tr: Translator;
  locale: string;
  uniqueName: string;
}

export default function BaroLastSeen({ available, lastRecorded, tr, locale, uniqueName }: Props) {
  return (
    <span className={`text-xs ${available ? "text-success" : "text-text-muted"}`} data-baro-last-seen={uniqueName}>
      {available
        ? tr("baro.availableNow")
        : lastRecorded !== null
          ? tr("baro.lastRecorded", { date: new Date(lastRecorded).toLocaleDateString(locale) })
          : tr("baro.noRecordedHistory")}
    </span>
  );
}
