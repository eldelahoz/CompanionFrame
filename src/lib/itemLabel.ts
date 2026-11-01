import { fallbackNameFromUniqueName, sanitizeDisplayName } from "../../config/shared/displayName";

/** Name to render: the game-language name, else English, else derived from the unique name. */
export const itemLabel = (uniqueName: string, entry?: { name: string; displayName?: string | null }): string =>
  sanitizeDisplayName(entry?.displayName || entry?.name) || fallbackNameFromUniqueName(uniqueName);
