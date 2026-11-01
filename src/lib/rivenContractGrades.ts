/** What the contract row shows: overall letter plus the attribute grade ("?" without good-roll data). */
export interface RivenContractGrade {
  overallGrade: string;
  attributeGrade: string;
  stats?: { grade: string; rollFloat: number }[];
}

interface ContractGradeMerge {
  entries: [string, RivenContractGrade | null][];
  provisional: string[];
  settled: string[];
}

export function mergeContractGrades(ids: string[], grades: (RivenContractGrade | null)[], sheetReady: boolean): ContractGradeMerge {
  const entries = ids.slice(0, grades.length).map((id, i): [string, RivenContractGrade | null] => [id, grades[i]]);
  const waits = (g: RivenContractGrade | null) => !sheetReady && g != null && g.attributeGrade === "?";
  return {
    entries,
    provisional: entries.filter(([, g]) => waits(g)).map(([id]) => id),
    settled: entries.filter(([, g]) => !waits(g)).map(([id]) => id),
  };
}

export const contractIdsToGrade = (
  ids: string[],
  graded: ReadonlyMap<string, RivenContractGrade | null>,
  provisional: ReadonlySet<string>,
  pending: ReadonlySet<string>,
): string[] => ids.filter((id) => (!graded.has(id) || provisional.has(id)) && !pending.has(id));
