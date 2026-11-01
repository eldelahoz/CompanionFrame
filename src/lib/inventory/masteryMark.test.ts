import { describe, expect, it } from "vitest";
import type { ItemDbEntry, ParsedItem } from "../../types/inventory";
import { buildMasteryIndex, masteryOfItem } from "./masteryMark";

const index = buildMasteryIndex([{ uniqueName: "/Ash", status: "progress", rank: 12, maxRank: 30 }]);
const db = { "/AshSystems": { isBuildComponent: true, componentOf: "/Ash" }, "/Mod": {} } as unknown as Record<string, ItemDbEntry>;
const row = (internalName: string) => ({ internalName }) as ParsedItem;

describe("masteryOfItem", () => {
  it("resolves the item itself, a part to its parent and a set to its root", () => {
    expect(masteryOfItem(row("/Ash"), index, db)?.status).toBe("progress");
    expect(masteryOfItem(row("/AshSystems"), index, db)).toEqual({ status: "progress", rank: 12, maxRank: 30 });
    expect(masteryOfItem(row("/Ash#set"), index, db)?.status).toBe("progress");
  });
  it("is null for items that do not count toward mastery", () => {
    expect(masteryOfItem(row("/Mod"), index, db)).toBeNull();
    expect(masteryOfItem(row("/Unknown"), index, db)).toBeNull();
  });
});
