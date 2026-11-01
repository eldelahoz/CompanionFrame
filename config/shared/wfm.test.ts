import { describe, it, expect } from "vitest";
import { formatWfmAssetUrl, isWfmSlug, normalizeWfmSlug, sanitizeWfmSlug } from "./wfm";
import { normalizeForSlug } from "./textNormalize";

describe("normalizeWfmSlug", () => {
  it("lowercases and trims", () => {
    expect(normalizeWfmSlug("  Frost Prime  ")).toBe("frost_prime");
  });

  it("replaces spaces and special characters with underscores", () => {
    expect(normalizeWfmSlug("Ash Prime Set")).toBe("ash_prime_set");
    expect(normalizeWfmSlug("Primed Flow!")).toBe("primed_flow");
  });

  it("strips ASCII apostrophe", () => {
    expect(normalizeWfmSlug("Loki's Decoy")).toBe("lokis_decoy");
  });

  it("strips unicode curly quotes (U+2019, U+2018) like ASCII apostrophe", () => {
    // Matches WFM's canonical slug form: user input with smart quotes must
    // resolve to the same slug as ASCII apostrophe input.
    expect(normalizeWfmSlug("Loki\u2019s Decoy")).toBe("lokis_decoy");
    expect(normalizeWfmSlug("Loki\u2018s Decoy")).toBe("lokis_decoy");
  });

  it("collapses multiple non-alphanumeric runs to single underscore", () => {
    expect(normalizeWfmSlug("a -- b ++ c")).toBe("a_b_c");
  });

  it("strips leading and trailing underscores", () => {
    expect(normalizeWfmSlug("_test_")).toBe("test");
    expect(normalizeWfmSlug("___abc___")).toBe("abc");
  });

  it("returns null for non-string input", () => {
    expect(normalizeWfmSlug(null)).toBeNull();
    expect(normalizeWfmSlug(undefined)).toBeNull();
    expect(normalizeWfmSlug(42 as never)).toBeNull();
    expect(normalizeWfmSlug({} as never)).toBeNull();
  });

  it("returns null for empty or whitespace-only strings", () => {
    expect(normalizeWfmSlug("")).toBeNull();
    expect(normalizeWfmSlug("   ")).toBeNull();
  });

  it("returns null for strings that become empty after normalization", () => {
    expect(normalizeWfmSlug("!!!")).toBeNull();
    expect(normalizeWfmSlug("---")).toBeNull();
  });

  it("handles typical WFM slugs passthrough", () => {
    expect(normalizeWfmSlug("nikana_prime_set")).toBe("nikana_prime_set");
    expect(normalizeWfmSlug("serration")).toBe("serration");
  });
});

describe("minted warframe.market slugs", () => {
  // The Tektolyst arcanes are slugged with hyphens, which folding turned into
  // zid_an_asheir and pointed every lookup at an item that does not exist.
  it("keeps a slug the catalog minted verbatim", () => {
    expect(normalizeWfmSlug("zid-an-asheir")).toBe("zid-an-asheir");
    expect(normalizeWfmSlug("summoner’s_wrath")).toBe("summoner’s_wrath");
    expect(normalizeWfmSlug("melee_riven_mod_(veiled)")).toBe("melee_riven_mod_(veiled)");
    expect(normalizeWfmSlug("höllvanian_old_town_in_fall")).toBe("höllvanian_old_town_in_fall");
  });

  // Lowercased, a display name and a slug are the same string, so provenance
  // decides: only a caller holding a real slug may reach normalizeWfmSlug.
  // Medi-Ray is a name and is slugged medi_ray, so names fold on their own path.
  it("folds anything not already slug shaped", () => {
    expect(normalizeWfmSlug("Zid-an Asheir")).toBe("zid_an_asheir");
    expect(normalizeForSlug("Medi-Ray")).toBe("medi_ray");
    expect(normalizeForSlug("Zid-an Asheir")).toBe("zid_an_asheir");
  });

  it("rejects anything carrying a path separator or whitespace", () => {
    expect(sanitizeWfmSlug("../secrets")).toBeNull();
    expect(sanitizeWfmSlug("a/b")).toBeNull();
    expect(sanitizeWfmSlug("ash prime")).toBeNull();
    expect(sanitizeWfmSlug("-".repeat(200))).toBeNull();
    expect(isWfmSlug("zid-an-asheir")).toBe(true);
    expect(isWfmSlug("_test_")).toBe(false);
  });
});

describe("formatWfmAssetUrl", () => {
  it("resolves relative asset paths against the WFM static base", () => {
    expect(formatWfmAssetUrl("items/images/en/thumbs/astral_bond.128x128.png")).toBe(
      "https://warframe.market/static/assets/items/images/en/thumbs/astral_bond.128x128.png",
    );
  });

  it("leaves absolute URLs alone and rejects blanks", () => {
    expect(formatWfmAssetUrl("https://example.com/thumbs/x.png")).toBe(
      "https://example.com/thumbs/x.png",
    );
    expect(formatWfmAssetUrl("   ")).toBeNull();
    expect(formatWfmAssetUrl(null)).toBeNull();
  });
});
