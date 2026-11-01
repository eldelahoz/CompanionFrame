import { describe, expect, it } from "vitest";

import {
  columnPlacement,
  dropPlacement,
  resolveDropTarget,
} from "./drag";
import { columnOf, moveSectionInList, planSections } from "./plan";
import type { LayoutColumn, SectionState } from "./types";

interface Drag {
  id: string;
  scope: readonly string[] | null;
  lastCell: string | null;
}

function drag(patch: Partial<Drag> = {}): Drag {
  return { id: "world.darvo", scope: null, lastCell: null, ...patch };
}

function hit(id: string, after = false): { id: string; after: boolean } {
  return { id, after };
}

describe("resolveDropTarget", () => {
  it("takes a section the pointer has not dropped on yet", () => {
    expect(resolveDropTarget(drag(), hit("world.cycles"), true)).toEqual({
      targetId: "world.cycles",
      after: false,
      lastCell: "world.cycles|before",
    });
  });

  it("drops under a section the pointer is in the lower half of", () => {
    expect(resolveDropTarget(drag(), hit("world.cycles", true), true)).toEqual({
      targetId: "world.cycles",
      after: true,
      lastCell: "world.cycles|after",
    });
  });

  it("ignores a pointer that is over nothing", () => {
    expect(resolveDropTarget(drag({ lastCell: "world.cycles|before" }), null, true)).toEqual({
      targetId: null,
      after: false,
      lastCell: "world.cycles|before",
    });
  });

  it("forgets the last cell when the pointer is back over the dragged section", () => {
    // The move puts the dragged section under the pointer, so the cell it came
    // from has to become droppable again or a drag can only ever move once.
    expect(
      resolveDropTarget(drag({ lastCell: "world.cycles|before" }), hit("world.darvo"), true),
    ).toEqual({ targetId: null, after: false, lastCell: null });
  });

  it("commits once while the pointer stays on the same cell", () => {
    expect(
      resolveDropTarget(drag({ lastCell: "world.cycles|before" }), hit("world.cycles"), true),
    ).toEqual({ targetId: null, after: false, lastCell: "world.cycles|before" });
  });

  it("treats the other half of the same section as a fresh cell", () => {
    expect(
      resolveDropTarget(drag({ lastCell: "world.cycles|before" }), hit("world.cycles", true), true),
    ).toEqual({ targetId: "world.cycles", after: true, lastCell: "world.cycles|after" });
  });

  it("ignores a hit that belongs to another grid", () => {
    expect(resolveDropTarget(drag(), hit("world.dailies"), false)).toEqual({
      targetId: null,
      after: false,
      lastCell: null,
    });
  });

  it("ignores a section this screen does not render", () => {
    const scoped = drag({ scope: ["world.darvo", "world.cycles"] });
    expect(resolveDropTarget(scoped, hit("world.arbiSchedule"), true)).toEqual({
      targetId: null,
      after: false,
      lastCell: null,
    });
    expect(resolveDropTarget(scoped, hit("world.cycles"), true)).toEqual({
      targetId: "world.cycles",
      after: false,
      lastCell: "world.cycles|before",
    });
  });
});

const section = (
  id: string,
  column: LayoutColumn,
  span: SectionState["span"] = 1,
): SectionState => ({
  id,
  span,
  hidden: false,
  collapsed: false,
  column,
});

describe("dropPlacement", () => {
  const order: SectionState[] = [
    section("a", 0),
    section("b", 0),
    section("c", 1),
    section("d", 1),
  ];

  it("lands before a target the pointer is in the upper half of", () => {
    expect(dropPlacement(order, "d", "b", false)).toEqual({ index: 1, column: 0 });
    expect(dropPlacement(order, "a", "c", false)).toEqual({ index: 1, column: 1 });
  });

  it("lands after a target the pointer is in the lower half of", () => {
    expect(dropPlacement(order, "d", "b", true)).toEqual({ index: 2, column: 0 });
    expect(dropPlacement(order, "a", "c", true)).toEqual({ index: 2, column: 1 });
  });

  it("takes the column of the section it was dropped on", () => {
    expect(dropPlacement(order, "b", "c", false)).toEqual({ index: 1, column: 1 });
    expect(dropPlacement(order, "c", "b", true)).toEqual({ index: 2, column: 0 });
  });

  it("keeps its own column when the target owns its whole row", () => {
    const withWide = [section("a", 0), section("wide", 0, "full"), section("c", 1)];
    expect(dropPlacement(withWide, "c", "wide", false)).toEqual({ index: 1, column: 1 });
  });

  it("reports a drop that changes nothing", () => {
    expect(dropPlacement(order, "a", "b", false)).toBeNull();
    expect(dropPlacement(order, "b", "a", true)).toBeNull();
  });

  it("ignores an id the list does not hold", () => {
    expect(dropPlacement(order, "zz", "b", false)).toBeNull();
    expect(dropPlacement(order, "a", "zz", false)).toBeNull();
  });
});

describe("columnPlacement", () => {
  const emptied: SectionState[] = [section("a", 1), section("b", 1), section("c", 1)];

  it("puts a section back into a column nothing is left in", () => {
    const before = planSections(emptied, "wide");
    if (before[0]?.kind !== "columns") throw new Error("expected a columns row");
    expect(before[0].columns[0]).toEqual([]);

    const placement = columnPlacement(emptied, "b", 0);
    expect(placement).toEqual({ index: 1, column: 0 });
    const rows = planSections(moveSectionInList(emptied, "b", placement ?? 0), "wide");
    if (rows[0]?.kind !== "columns") throw new Error("expected a columns row");
    expect(rows[0].columns[0]?.map((slot) => slot.id)).toEqual(["b"]);
    expect(rows[0].columns[1]?.map((slot) => slot.id)).toEqual(["a", "c"]);
  });

  it("reports nothing for a section already in that column or an id it has not", () => {
    expect(columnPlacement(emptied, "b", 1)).toBeNull();
    expect(columnPlacement(emptied, "zz", 0)).toBeNull();
  });
});

describe("drag drop applied to the section list", () => {
  const list: SectionState[] = [section("a", 0), section("b", 0), section("c", 1), section("d", 1)];

  const drop = (id: string, targetId: string, after: boolean): SectionState[] => {
    const placement = dropPlacement(list, id, targetId, after);
    return placement === null ? list : moveSectionInList(list, id, placement);
  };

  const dropped = (id: string, targetId: string, after: boolean): string[] =>
    drop(id, targetId, after).map((entry) => entry.id);

  it("moves a section exactly one place when it is dragged past one neighbour", () => {
    expect(dropped("a", "b", true)).toEqual(["b", "a", "c", "d"]);
    expect(dropped("d", "c", false)).toEqual(["a", "b", "d", "c"]);
  });

  it("puts a section under a target further down instead of into its slot", () => {
    expect(dropped("a", "c", true)).toEqual(["b", "c", "a", "d"]);
    expect(dropped("a", "c", false)).toEqual(["b", "a", "c", "d"]);
  });

  it("puts a section under a target further up", () => {
    expect(dropped("d", "a", true)).toEqual(["a", "d", "b", "c"]);
    expect(dropped("d", "a", false)).toEqual(["d", "a", "b", "c"]);
  });

  it("reaches the last slot from above", () => {
    expect(dropped("a", "d", true)).toEqual(["b", "c", "d", "a"]);
  });

  it("moves only the dragged section when the drop crosses the boundary", () => {
    const moved = drop("a", "c", false);
    expect(moved.map((entry) => `${entry.id}:${String(columnOf(entry))}`)).toEqual([
      "b:0",
      "a:1",
      "c:1",
      "d:1",
    ]);
  });

  it("lands under the last section of a column instead of atop the next one", () => {
    const rows = planSections(drop("a", "d", true), "wide");
    if (rows[0]?.kind !== "columns") throw new Error("expected a columns row");
    expect(rows[0].columns[0]?.map((slot) => slot.id)).toEqual(["b"]);
    expect(rows[0].columns[1]?.map((slot) => slot.id)).toEqual(["c", "d", "a"]);
  });
});
