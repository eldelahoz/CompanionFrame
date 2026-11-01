import { expect, test } from "vitest";
import { withSearch } from "../savedSearches";
import { enemySpawnGroups, findEnemyByPartialName, tileSetSpawnPlanets } from "./enemyInfo";

test("a typed name resolves to its enemy and ambiguity resolves to nothing", () => {
  expect(findEnemyByPartialName("Butcher")?.name).toBe("Butcher");
  const arid = findEnemyByPartialName("arid butch");
  expect(arid?.name).toBe("Arid Butcher");
  expect(findEnemyByPartialName("zz")).toBeNull();
});

test("spawn groups drop empty lists", () => {
  const info = findEnemyByPartialName("arid butch");
  const groups = enemySpawnGroups(info, tileSetSpawnPlanets(info));
  expect(groups.every((g) => g.values.length > 0)).toBe(true);
});

test("saved searches skip blanks and case-insensitive duplicates", () => {
  expect(withSearch(["Forma"], "  forma ")).toEqual(["Forma"]);
  expect(withSearch(["Forma"], " ")).toEqual(["Forma"]);
  expect(withSearch(["Forma"], "Orokin Cell")).toEqual(["Forma", "Orokin Cell"]);
});
