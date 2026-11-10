/** Comparison key for names: no case, no accents, single spaces. */
export const foldName = (value: string): string => value.normalize("NFD").replace(/\p{M}/gu, "").replace(/\s+/g, " ").trim().toLowerCase();
