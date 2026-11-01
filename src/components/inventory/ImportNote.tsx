import { useInventory } from "../../stores/inventory";
import type { Translator } from "../../lib/i18n";

/** "Based on your import from <date>": views whose numbers come from an imported snapshot say so. */
export default function ImportNote({ tr, locale = "en" }: { tr: Translator; locale?: string }) {
  const { status } = useInventory();
  if (!status?.found || status.importedAt == null) return null;
  const date = new Date(status.importedAt).toLocaleDateString(locale, { year: "numeric", month: "short", day: "numeric" });
  const deltas = status.tradesApplied + status.picksApplied;
  return (
    <p className="m-0 text-xs text-text-muted" data-import-note>
      {tr("inventory.basedOn", { date })}
      {deltas > 0 ? ` ${tr("inventory.basedOnDeltas")}` : ""}
    </p>
  );
}
