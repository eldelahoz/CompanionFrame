import SegmentedControl from "../components/SegmentedControl";
import type { Translator } from "../lib/i18n";
import { writeRelicViewPreference, type RelicView } from "../lib/relic/relicView";

/** Normal (item popup) / detailed (rewards breakdown); the last choice is remembered. */
export default function RelicViewSwitch({ view, onSwitch, tr }: { view: RelicView; onSwitch: (view: RelicView) => void; tr: Translator }) {
  const choose = (next: RelicView) => {
    writeRelicViewPreference(next);
    if (next !== view) onSwitch(next);
  };
  return (
    <div className="inline-flex shrink-0" role="group" aria-label={tr("detail.relicView")} data-relic-view-switch={view}>
      <SegmentedControl
        value={view}
        options={[
          { value: "simple", label: tr("common.normal") },
          { value: "detailed", label: tr("common.detailed") },
        ]}
        onChange={choose}
      />
    </div>
  );
}
