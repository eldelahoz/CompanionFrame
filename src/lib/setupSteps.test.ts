import { describe, expect, it } from "vitest";
import { PRESET_KEYS } from "../config/themePresets";
import { nextStep, prevStep, SETUP_THEME_KEYS } from "./setupSteps";

describe("setupSteps", () => {
  it("walks the three steps and stops at the ends", () => {
    expect(nextStep("configure")).toBe("inventory");
    expect(nextStep("inventory")).toBe("placement");
    expect(nextStep("placement")).toBeNull();
    expect(prevStep("configure")).toBeNull();
    expect(prevStep("placement")).toBe("inventory");
  });

  it("starter themes all exist", () => {
    for (const k of SETUP_THEME_KEYS) expect(PRESET_KEYS).toContain(k);
  });
});
