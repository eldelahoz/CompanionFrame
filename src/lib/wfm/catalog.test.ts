import { describe, expect, it, vi } from "vitest";

import type { CatalogItem, SetLookup } from "../ipc";
import { buildCatalogIndex, createSetResolver, hasRanks } from "./catalog";

const item = (slug: string, name: string, extra: Partial<CatalogItem> = {}): CatalogItem => ({
  id: `id_${slug}`,
  slug,
  name,
  thumb: null,
  icon: null,
  maxRank: null,
  gameRef: null,
  ...extra,
});

describe("buildCatalogIndex", () => {
  const index = buildCatalogIndex([
    item("serration", "Serration", { maxRank: 10, gameRef: "/Lotus/Upgrades/Mods/Rifle/WeaponDamageAmountMod" }),
    item("ash_prime_set", "Ash Prime Set"),
    item("mutalist_alad_v_assassinate_key", "Mutalist Alad V Assassinate (Key)"),
    item("serrated_edge", "Serrated Edge"),
  ]);

  it("resolves name <-> slug <-> id, case-insensitively", () => {
    expect(index.byName("SERRATION")?.slug).toBe("serration");
    expect(index.bySlug("ash_prime_set")?.name).toBe("Ash Prime Set");
    expect(index.byId("id_serration")?.slug).toBe("serration");
    expect(index.byName("nope")).toBeNull();
    expect(index.bySlug("nope")).toBeNull();
  });

  it("tries the set form of a bare name and the bare form of a set name", () => {
    expect(index.byName("Ash Prime")?.slug).toBe("ash_prime_set");
    expect(index.byName("Serration Set")?.slug).toBe("serration");
  });

  it("indexes a disambiguated listing under the bare game name", () => {
    expect(index.byName("Mutalist Alad V Assassinate")?.slug).toBe("mutalist_alad_v_assassinate_key");
  });

  it("matches the game reference regardless of casing", () => {
    expect(index.byGameRef("/lotus/upgrades/mods/rifle/weapondamageamountmod")?.slug).toBe("serration");
    expect(index.byGameRef("")).toBeNull();
  });

  it("never lets an alias shadow a real name", () => {
    const i = buildCatalogIndex([item("x_set", "Other Name"), item("x", "X")]);
    expect(i.byName("x")?.slug).toBe("x");
  });

  it("searches prefix matches before substring matches and ignores short queries", () => {
    expect(index.search("ser").map((r) => r.slug)).toEqual(["serration", "serrated_edge"]);
    expect(index.search("prime").map((r) => r.slug)).toEqual(["ash_prime_set"]);
    expect(index.search("s")).toEqual([]);
    expect(index.search("ser", 1)).toHaveLength(1);
  });
});

describe("hasRanks", () => {
  it.each([
    [{}, false],
    [{ maxRank: null }, false],
    [{ maxRank: 0 }, false],
    [{ maxRank: 5 }, true],
  ])("%j -> %s", (i, expected) => expect(hasRanks(i)).toBe(expected));
});

describe("createSetResolver", () => {
  const set: SetLookup = {
    kind: "set",
    setSlug: "ash_prime_set",
    parts: [
      { slug: "ash_prime_blueprint", quantityInSet: 1 },
      { slug: "ash_prime_chassis_blueprint", quantityInSet: 1 },
    ],
  };

  it("stores one answer under the set and every part, and shares in-flight lookups", async () => {
    const fetchSet = vi.fn().mockResolvedValue(set);
    const resolve = createSetResolver(fetchSet);
    const [a, b] = await Promise.all([resolve("ash_prime_blueprint"), resolve("ash_prime_blueprint")]);
    expect(a).toBe(b);
    for (const slug of ["ash_prime_set", "ash_prime_chassis_blueprint"]) await resolve(slug);
    expect(fetchSet).toHaveBeenCalledTimes(1);
  });

  it("remembers a non-set", async () => {
    const fetchSet = vi.fn().mockResolvedValue({ kind: "not_set" });
    const resolve = createSetResolver(fetchSet);
    await resolve("serration");
    await resolve("serration");
    expect(fetchSet).toHaveBeenCalledTimes(1);
  });

  it("does not remember a failure", async () => {
    const fetchSet = vi.fn().mockRejectedValueOnce(new Error("503")).mockResolvedValue(set);
    const resolve = createSetResolver(fetchSet);
    await expect(resolve("ash_prime_set")).rejects.toThrow("503");
    await expect(resolve("ash_prime_set")).resolves.toEqual(set);
    expect(fetchSet).toHaveBeenCalledTimes(2);
  });
});
