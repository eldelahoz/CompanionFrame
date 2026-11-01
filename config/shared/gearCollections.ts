/** Inventory collections that hold built equipment, one row per copy. */
export const EQUIPMENT_COLLECTIONS = [
  "Suits",
  "LongGuns",
  "Pistols",
  "Melee",
  "Sentinels",
  "SentinelWeapons",
  "SpaceSuits",
  "SpaceGuns",
  "SpaceMelee",
  "OperatorAmps",
  "MechSuits",
] as const;

/** Modular collections DE keeps outside the equipment list: K-Drives, MOAs and hatched pets. */
export const MODULAR_COLLECTIONS = ["Hoverboards", "MoaPets", "KubrowPets"] as const;

/** Everything built that a recipe can consume: equipment plus pets, K-Drives and gear items. */
export const BUILT_GEAR_COLLECTIONS = [
  ...EQUIPMENT_COLLECTIONS,
  ...MODULAR_COLLECTIONS,
  "SpecialItems",
] as const;

export type EquipmentCollection = (typeof EQUIPMENT_COLLECTIONS)[number];
