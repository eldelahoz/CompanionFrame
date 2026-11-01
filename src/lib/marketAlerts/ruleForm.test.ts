import { describe, expect, it } from "vitest";
import { emptyRule, MAX_COOLDOWN_MINUTES, MIN_COOLDOWN_MINUTES, normalizeRule, parseOptionalNumber, RIVEN_ATTRIBUTES, validateRule } from "./ruleForm";

describe("validateRule", () => {
  it("asks for a name first", () => {
    expect(validateRule(emptyRule("item"))).toBe("marketAlerts.nameRequired");
  });

  it("asks for a weapon on a riven rule", () => {
    expect(validateRule({ ...emptyRule("riven"), name: "x" })).toBe("marketAlerts.weaponRequired");
  });

  it("asks for an item, then a price threshold, on an item rule", () => {
    const rule = { ...emptyRule("item"), name: "x" };
    expect(validateRule(rule)).toBe("marketAlerts.itemRequired");
    const withItem = { ...rule, item: { ...rule.item!, itemUrlName: "serration" } };
    expect(validateRule(withItem)).toBe("marketAlerts.priceRequired");
    expect(validateRule({ ...withItem, item: { ...withItem.item, minPlatinum: 5 } })).toBeNull();
  });

  it("accepts a riven rule with only a weapon", () => {
    const rule = emptyRule("riven");
    expect(validateRule({ ...rule, name: "x", riven: { ...rule.riven!, weaponUrlName: "rubico" } })).toBeNull();
  });
});

describe("normalizeRule", () => {
  it("trims the name and clamps the cooldown into the accepted range", () => {
    expect(normalizeRule({ ...emptyRule("item"), name: "  hi  ", cooldownMinutes: 1 })).toMatchObject({ name: "hi", cooldownMinutes: MIN_COOLDOWN_MINUTES });
    expect(normalizeRule({ ...emptyRule("item"), cooldownMinutes: 99_999 }).cooldownMinutes).toBe(MAX_COOLDOWN_MINUTES);
    expect(normalizeRule({ ...emptyRule("item"), cooldownMinutes: Number.NaN }).cooldownMinutes).toBe(MIN_COOLDOWN_MINUTES);
  });
});

describe("parseOptionalNumber", () => {
  it("reads empty as no bound and keeps a real zero", () => {
    expect(parseOptionalNumber("")).toBeNull();
    expect(parseOptionalNumber("  ")).toBeNull();
    expect(parseOptionalNumber("0")).toBe(0);
    expect(parseOptionalNumber("abc")).toBeNull();
  });
});

describe("RIVEN_ATTRIBUTES", () => {
  it("is a sorted list of unique slugs", () => {
    expect(RIVEN_ATTRIBUTES.length).toBeGreaterThan(10);
    expect(new Set(RIVEN_ATTRIBUTES).size).toBe(RIVEN_ATTRIBUTES.length);
    expect(RIVEN_ATTRIBUTES.every((s) => /^[a-z0-9_/]+$/.test(s))).toBe(true);
  });
});
