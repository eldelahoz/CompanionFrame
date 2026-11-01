import { describe, expect, it } from "vitest";

import { hasQuantityPrefix, stripQuantityPrefix } from "./quantityPrefix";

// Drop tables spell the bonus count "2X Forma Blueprint"; a reward card prints
// "2 X". Both fold, and nothing that merely starts with a digit or an x does.
describe("quantity prefix folding", () => {
  it("strips every count spelling the tables and the cards use", () => {
    expect(stripQuantityPrefix("2X Forma Blueprint")).toBe("Forma Blueprint");
    expect(stripQuantityPrefix("2 X Orokin Cell")).toBe("Orokin Cell");
    expect(stripQuantityPrefix("2x forma blueprint")).toBe("forma blueprint");
    expect(stripQuantityPrefix("1200X Kuva")).toBe("Kuva");
  });

  it("leaves names that only look counted", () => {
    // "X3lp Glyph" is a real item: a leading x is not a count.
    expect(stripQuantityPrefix("X3lp Glyph")).toBe("X3lp Glyph");
    expect(stripQuantityPrefix("10 Year Anniversary Community Sigil")).toBe(
      "10 Year Anniversary Community Sigil",
    );
    expect(stripQuantityPrefix("2XForma Blueprint")).toBe("2XForma Blueprint");
    expect(stripQuantityPrefix("Forma 2X Blueprint")).toBe("Forma 2X Blueprint");
    expect(stripQuantityPrefix("Forma Blueprint")).toBe("Forma Blueprint");
  });

  it("reports the same prefixes it strips", () => {
    expect(hasQuantityPrefix("2 X Orokin Cell")).toBe(true);
    expect(hasQuantityPrefix("2x forma blueprint")).toBe(true);
    expect(hasQuantityPrefix("X3lp Glyph")).toBe(false);
    expect(hasQuantityPrefix("Forma Blueprint")).toBe(false);
  });
});
