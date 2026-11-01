import { PackageOpen } from "lucide-react";
import type { MessageKey, Translator } from "../../lib/i18n";
import { openSettingsCard } from "../../stores/nav";
import { Button, EmptyState } from "../ui";

/**
 * What a view shows with no inventory source: what is missing, why this view needs it, and how to
 * fix it (import, AlecaFrame, public profile id or memory mode), linking to Settings › Inventory.
 */
export default function NoInventoryState({ tr, why }: { tr: Translator; why: MessageKey }) {
  return (
    <div data-no-inventory>
      <EmptyState
        icon={<PackageOpen />}
        title={tr("app.noInventoryLoaded")}
        action={
          <Button size="sm" variant="primary" onClick={() => openSettingsCard("inventory")}>
            {tr("noInventory.open")}
          </Button>
        }
      >
        {tr(why)} {tr("noInventory.how")}
      </EmptyState>
    </div>
  );
}
