import { useState } from "react";
import { Trash2 } from "lucide-react";
import { Button, IconButton, Input, Select } from "../ui";
import type { Translator } from "../../lib/i18n";
import { deleteSavedSelection, loadSavedSelection, saveSelection, useSavedSelections } from "../../stores/inventorySelection";

interface Props {
  count: number;
  /** How many of the picked rows the current tab and filters show. */
  onTabCount: number;
  /** Selectable rows the filters show; drives the "Select all" label. */
  eligibleCount: number;
  onSelectAll: () => void;
  onClear: () => void;
  onBulkSell: () => void;
  tr: Translator;
}

/** Sticky strip under the toolbar while the selection mode is on: counts, select all / clear, saved sets, bulk sell. */
export default function InventorySelectionBar({ count, onTabCount, eligibleCount, onSelectAll, onClear, onBulkSell, tr }: Props) {
  const saved = useSavedSelections();
  const [draft, setDraft] = useState("");
  const [picked, setPicked] = useState("");

  return (
    <div className="inv-selection-bar" data-inventory-select-bar role="region" aria-label={tr("inventory.selectMode")}>
      <span className="inv-selection-count" data-inventory-select-count>
        {tr("common.selected", { count })}
      </span>
      {count > onTabCount && <span className="text-xs text-text-muted">{tr("market.selectedHidden", { count: count - onTabCount })}</span>}
      <Button size="sm" disabled={eligibleCount === 0} data-inventory-select-all onClick={onSelectAll}>
        {tr("inventory.selectAllFiltered", { count: eligibleCount })}
      </Button>
      <Button size="sm" variant="ghost" disabled={count === 0} data-inventory-select-clear onClick={onClear}>
        {tr("inventory.clearSelection")}
      </Button>
      <span className="inv-selection-save">
        <Input value={draft} placeholder={tr("inventory.saveSelectionPlaceholder")} aria-label={tr("inventory.saveSelectionPlaceholder")} data-inventory-selection-name onChange={(e) => setDraft(e.target.value)} />
        <Button
          size="sm"
          disabled={!draft.trim() || count === 0}
          data-inventory-selection-save
          onClick={() => {
            saveSelection(draft);
            setDraft("");
          }}
        >
          {tr("common.save")}
        </Button>
      </span>
      {saved.length > 0 && (
        <span className="inv-selection-save">
          <Select
            aria-label={tr("inventory.savedSelections")}
            value={picked}
            data-inventory-selection-saved
            options={[["", tr("inventory.savedSelections")], ...saved.map((s) => [s.name, `${s.name} (${s.keys.length})`] as const)]}
            onChange={(e) => setPicked(e.target.value)}
          />
          <Button size="sm" disabled={!picked} data-inventory-selection-load onClick={() => loadSavedSelection(picked)}>
            {tr("inventory.loadSelection")}
          </Button>
          <IconButton
            label={tr("common.delete")}
            danger
            disabled={!picked}
            onClick={() => {
              deleteSavedSelection(picked);
              setPicked("");
            }}
          >
            <Trash2 />
          </IconButton>
        </span>
      )}
      <Button variant="primary" size="sm" className="ml-auto" disabled={count === 0} data-bulk-sell-open onClick={onBulkSell}>
        {tr("inventory.bulkSell", { count })}
      </Button>
    </div>
  );
}
