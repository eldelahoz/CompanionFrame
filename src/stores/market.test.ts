import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("../lib/ipc", () => ({ getWfmItems: vi.fn() }));

import { getWfmItems } from "../lib/ipc";
import { loadCatalog, resetCatalogForTest } from "./market";

const get = vi.mocked(getWfmItems);
const row = { id: "a", slug: "serration", name: "Serration", thumb: null, icon: null, maxRank: 10, gameRef: null };

afterEach(() => {
  resetCatalogForTest();
  get.mockReset();
});

describe("loadCatalog", () => {
  it("loads once and indexes by name", async () => {
    get.mockResolvedValue([row]);
    const [a, b] = await Promise.all([loadCatalog(), loadCatalog()]);
    expect(a).toBe(b);
    expect(a.byName("serration")?.slug).toBe("serration");
    expect(get).toHaveBeenCalledTimes(1);
  });

  it("does not latch a failure", async () => {
    get.mockRejectedValueOnce(new Error("503")).mockResolvedValue([row]);
    await expect(loadCatalog()).rejects.toThrow("503");
    await expect(loadCatalog()).resolves.toBeDefined();
    expect(get).toHaveBeenCalledTimes(2);
  });
});
