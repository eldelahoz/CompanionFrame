import { expect, it } from "vitest";
import { SYNDICATE_RANKS } from "../../data/syndicateRanks";
import type { RawInventoryData } from "../../types/inventory";
import { aggregateNeeds, alignmentConflicts, dailyStandingCap, planSteps, syndicateStatus } from "./rankup";
import { normalizeGoals, withGoal } from "./goals";

const meta = (tag: string) => SYNDICATE_RANKS.find((m) => m.tag === tag)!;
const inv = (o: Record<string, unknown>) => o as RawInventoryData;

it("reads rank and standing from Affiliations and plans each step to a goal", () => {
  const m = meta("ArbitersSyndicate");
  const data = inv({ Affiliations: [{ Tag: m.tag, Title: 1, Standing: 5000, Initiated: true }], RegularCredits: 1 });
  const status = syndicateStatus(data, m);
  expect(status).toMatchObject({ level: 1, standing: 5000, nextLevel: 2, initiated: true });
  const steps = planSteps(data, m, 3);
  expect(steps.map((s) => s.level)).toEqual([2, 3]);
  expect(steps[0].standingNeeded).toBe(status.standingToNext);
  expect(steps.every((s) => !s.initiation)).toBe(true);
});

it("charges initiation once and sums needs against owned items", () => {
  const m = meta("ArbitersSyndicate");
  const data = inv({ PlayerLevel: 10, RegularCredits: 100, [m.dailyField]: 1000 });
  const steps = planSteps(data, m, 1);
  expect(steps[0].initiation).toBe(true);
  const needs = aggregateNeeds([{ meta: m, targetLevel: 1, steps }], data, new Map([["/Lotus/Types/Items/MiscItems/AlloyPlate", 200]]));
  expect(needs.credits).toMatchObject({ needed: steps[0].credits, owned: 100 });
  expect(needs.items.find((i) => i.name === "Alloy Plate")).toMatchObject({ needed: 500, owned: 200, missing: 300 });
  expect(needs.standing[0].dailyCap).toBe(dailyStandingCap(data));
  expect(dailyStandingCap(data)).toBe(21000);
});

it("flags selected syndicates that are enemies", () => {
  const c = alignmentConflicts([meta("ArbitersSyndicate"), meta("RedVeilSyndicate")]);
  expect(c).toEqual([{ a: "ArbitersSyndicate", b: "RedVeilSyndicate", factor: -1 }]);
  expect(alignmentConflicts([meta("ArbitersSyndicate")])).toEqual([]);
});

it("normalizes persisted goals and edits them immutably", () => {
  expect(normalizeGoals({ A: 2.4, B: "x", C: "3" })).toEqual({ A: 2, C: 3 });
  expect(normalizeGoals([1])).toEqual({});
  const g = { A: 1 };
  expect(withGoal(g, "B", 2)).toEqual({ A: 1, B: 2 });
  expect(withGoal(g, "A", null)).toEqual({});
  expect(g).toEqual({ A: 1 });
});
