import { toFiniteNumber } from "../../../config/shared/numeric";
import { readStorage, writeStorage } from "../persistence";

const KEY = "cf_syndicate_goals_v1";

/** Target rank per syndicate tag; a cleared tag is absent. */
export type SyndicateGoals = Record<string, number>;

export function normalizeGoals(parsed: unknown): SyndicateGoals {
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return {};
  const goals: SyndicateGoals = {};
  for (const [tag, value] of Object.entries(parsed)) {
    const level = toFiniteNumber(value);
    if (level !== null) goals[tag] = Math.round(level);
  }
  return goals;
}

export function loadGoals(): SyndicateGoals {
  try {
    return normalizeGoals(JSON.parse(readStorage(KEY) ?? "{}"));
  } catch {
    return {};
  }
}

export function saveGoals(goals: SyndicateGoals): void {
  writeStorage(KEY, JSON.stringify(goals));
}

/** `null` clears the goal. */
export function withGoal(goals: SyndicateGoals, tag: string, level: number | null): SyndicateGoals {
  const { [tag]: _, ...rest } = goals;
  return level === null ? rest : { ...rest, [tag]: level };
}
