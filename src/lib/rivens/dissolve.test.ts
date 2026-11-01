import { expect, it } from "vitest";
import { rivenDissolveHint } from "./dissolve";

const riven = (o: Partial<Parameters<typeof rivenDissolveHint>[0]> = {}) => ({ attributeGrade: "Bad", masteryReq: 16, currentRank: 8, rerolls: 6, ...o });

it("suggests dissolving only weak, heavily rerolled or maxed rivens", () => {
  expect(rivenDissolveHint(riven())).toBe(100 * 8 + Math.floor(22.5 * 256) + 200 * 6 - 7);
  expect(rivenDissolveHint(riven({ attributeGrade: "Great" }))).toBeNull();
  expect(rivenDissolveHint(riven({ currentRank: 0, rerolls: 1 }))).toBeNull();
});
