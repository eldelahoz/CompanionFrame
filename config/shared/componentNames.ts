const INFESTED_MECH_PART_PATH = /\/InfestedMicroplanet\/Resources\/Mechs\//i;

export function isInfestedMechPart(uniqueName: string): boolean {
  return INFESTED_MECH_PART_PATH.test(uniqueName);
}

export function componentUniqueNameAliases(uniqueName: string): string[] {
  const aliases = [uniqueName];
  if (/Blueprint$/i.test(uniqueName)) aliases.push(uniqueName.replace(/Blueprint$/i, "Component"));
  if (/Component$/i.test(uniqueName)) aliases.push(uniqueName.replace(/Component$/i, "Blueprint"));
  // Weapon parts: the set lists .../AkbroncoPrimeLink, the inventory ...LinkBlueprint.
  if (!/Blueprint$/i.test(uniqueName) && /\/Types\/Recipes\//i.test(uniqueName)) {
    aliases.push(`${uniqueName}Blueprint`);
  }
  return aliases;
}

/** Owned count for a set component; max across aliases - one pile, two spellings. */
export function ownedComponentCount(
  uniqueName: string | null | undefined,
  ownedCounts: ReadonlyMap<string, number>,
): number {
  if (!uniqueName) return 0;
  let owned = 0;
  for (const alias of componentUniqueNameAliases(uniqueName)) {
    owned = Math.max(owned, ownedCounts.get(alias) || 0);
  }
  return owned;
}
