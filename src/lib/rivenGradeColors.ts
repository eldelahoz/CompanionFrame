const GRADE_VARS: Record<string, string> = { S: "s", A: "a", B: "b", C: "c", D: "d", F: "f" };
const ATTR_VARS: Record<string, string> = { Great: "s", Good: "a", OK: "b", Bad: "f" };

/** CSS variable reference for a riven overall grade (S, A, B, C, D, F, with optional +/-). */
export const gradeColor = (grade: string): string => `var(--grade-${GRADE_VARS[grade.charAt(0)] ?? "default"})`;

/** CSS variable reference for an individual attribute grade. */
export const attrGradeColor = (grade: string): string => `var(--grade-${ATTR_VARS[grade] ?? "default"})`;

/** Return a disposition star string for a given disposition value. */
export function dispoStars(dispo: number): string {
  if (dispo >= 1.3) return "●●●●●";
  if (dispo >= 1.1) return "●●●●○";
  if (dispo >= 0.9) return "●●●○○";
  if (dispo >= 0.7) return "●●○○○";
  return "●○○○○";
}
