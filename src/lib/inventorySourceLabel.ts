import {
  normalizeInventorySource,
  type InventorySource,
} from "../../config/shared/inventorySource.js";
import type { MessageKey } from "./i18n.js";

interface InventorySourceDescription {
  labelKey: MessageKey;
  /** File name of the user's pick, or "". */
  detail: string;
  /** Full path when there is one, else "" so the caller falls back to the label. */
  path: string;
}

const SOURCE_LABEL_KEYS: Record<InventorySource, MessageKey> = {
  manual: "settings.inventorySourceManual",
  aleca: "settings.inventorySourceAleca",
  helper: "settings.inventorySourceHelper",
  none: "settings.inventorySourceNone",
};

function fileName(filePath: string): string {
  const parts = filePath.split(/[\\/]/);
  return parts[parts.length - 1] || filePath;
}

/** Where inventory data comes from, as keys the caller translates. */
export function describeInventorySource(
  source: unknown,
  path: string | null,
): InventorySourceDescription {
  const normalized = normalizeInventorySource(source);
  const labelKey = SOURCE_LABEL_KEYS[normalized];
  // The helper writes its own file, so naming it says nothing the label does not.
  if (normalized === "none" || normalized === "helper" || !path) return { labelKey, detail: "", path: "" };
  return { labelKey, detail: fileName(path), path };
}
