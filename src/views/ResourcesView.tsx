import { Package } from "lucide-react";
import ItemImage from "../components/ItemImage";
import { EmptyState, Readout } from "../components/ui";
import { formatNumber } from "../lib/format";
import type { Translator } from "../lib/i18n";
import { itemLabel } from "../lib/itemLabel";
import type { Resource } from "../types/inventory";

interface Props {
  resources: Resource[];
  tr: Translator;
}

/** Owned resources, biggest stack first (the parse already sorts). */
export default function ResourcesView({ resources, tr }: Props) {
  if (resources.length === 0) return <EmptyState icon={<Package />} title={tr("resources.noResourcesFound")} />;
  return (
    <div className="resources-grid">
      {resources.map((r) => {
        const label = itemLabel(r.internalName, r);
        return (
          <div key={r.internalName} className="resource-tile ui-card" data-resource={r.internalName}>
            <span className="ui-thumb resource-thumb">
              <ItemImage src={r.imageUrl} alt="" tr={tr} cls="h-full w-full" />
            </span>
            <span className="resource-name" title={label}>
              {label}
            </span>
            <Readout chip value={r.count}>
              <span title={formatNumber(r.count)}>{formatNumber(r.count)}</span>
            </Readout>
          </div>
        );
      })}
    </div>
  );
}
