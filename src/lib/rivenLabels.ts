import type { MessageKey } from "./i18n";

// The decoder hands back raw English labels; map them here so every view resolves the same translation.
export const RIVEN_TYPE_KEYS: Record<string, MessageKey> = {
  Rifle: "rivens.type.rifle",
  Shotgun: "rivens.type.shotgun",
  Pistol: "rivens.type.pistol",
  Melee: "rivens.type.melee",
  Archgun: "rivens.type.archgun",
  Kitgun: "rivens.type.kitgun",
  Zaw: "rivens.type.zaw",
  Riven: "rivens.type.riven",
};

// Attribute grades are Great/Good/OK/Bad, or "?" when the weapon has no good-roll data; "?" stays unmapped.
export const RIVEN_ATTR_GRADE_KEYS: Record<string, MessageKey> = {
  Great: "rivens.grade.great",
  Good: "rivens.grade.good",
  OK: "rivens.grade.ok",
  Bad: "rivens.grade.bad",
};

/** Sort weight for the attribute grade; "?" (no sheet data) has none. */
export const RIVEN_ATTR_GRADE_ORDER: Record<string, number> = { Great: 4, Good: 3, OK: 2, Bad: 1 };
