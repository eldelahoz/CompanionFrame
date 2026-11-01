import { describe, expect, it } from "vitest";

import { polarityToWfm } from "./wfmRivenVocabulary";

describe("polarityToWfm", () => {
  it("folds every polarity a riven can roll", () => {
    expect(polarityToWfm("AP_ATTACK")).toBe("madurai");
    expect(polarityToWfm("AP_DEFENSE")).toBe("vazarin");
    expect(polarityToWfm("AP_TACTIC")).toBe("naramon");
  });

  it("keeps the names WFM itself returns", () => {
    expect(polarityToWfm("madurai")).toBe("madurai");
    expect(polarityToWfm("vazarin")).toBe("vazarin");
    expect(polarityToWfm("naramon")).toBe("naramon");
  });

  // Listing one of these fails the auction form, so it must not survive the fold.
  it("rejects a school no riven carries", () => {
    expect(polarityToWfm("AP_POWER")).toBeNull();
    expect(polarityToWfm("AP_WARD")).toBeNull();
    expect(polarityToWfm("AP_UMBRA")).toBeNull();
    expect(polarityToWfm("zenurik")).toBeNull();
    expect(polarityToWfm("")).toBeNull();
    expect(polarityToWfm(null)).toBeNull();
  });
});
