import { afterEach, describe, expect, it, vi } from "vitest";

import { stubStorage } from "../../test/storage";

// isSafeMode memoises and consumes the one-shot flag, so every case needs a fresh module.
async function loadSafeMode(): Promise<typeof import("./safeMode.js")> {
  vi.resetModules();
  return import("./safeMode.js");
}

describe("isSafeMode", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("is off with no flag anywhere", async () => {
    vi.stubGlobal("location", { search: "" });
    stubStorage();
    const { isSafeMode } = await loadSafeMode();
    expect(isSafeMode()).toBe(false);
  });

  it("reads safe=1 from the launch query", async () => {
    vi.stubGlobal("location", { search: "?safe=1" });
    stubStorage();
    const { isSafeMode } = await loadSafeMode();
    expect(isSafeMode()).toBe(true);
  });

  it("ignores any other value of the query flag", async () => {
    vi.stubGlobal("location", { search: "?safe=0" });
    stubStorage();
    const { isSafeMode } = await loadSafeMode();
    expect(isSafeMode()).toBe(false);
  });

  it("consumes the one-shot flag but keeps reporting safe mode for this load", async () => {
    const storage = stubStorage({ wf_safe_mode_once: "1" });
    vi.stubGlobal("location", { search: "" });
    const { isSafeMode, SAFE_MODE_ONCE_KEY } = await loadSafeMode();

    expect(isSafeMode()).toBe(true);
    expect(storage.has(SAFE_MODE_ONCE_KEY)).toBe(false);
    expect(isSafeMode()).toBe(true);
  });

  it("is off on the next load after the one-shot flag was consumed", async () => {
    stubStorage({ wf_safe_mode_once: "1" });
    vi.stubGlobal("location", { search: "" });
    expect((await loadSafeMode()).isSafeMode()).toBe(true);
    expect((await loadSafeMode()).isSafeMode()).toBe(false);
  });

  it("survives storage that throws", async () => {
    vi.stubGlobal("location", { search: "" });
    vi.stubGlobal("localStorage", {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("blocked");
      },
      removeItem: () => {},
    });
    const { isSafeMode } = await loadSafeMode();
    expect(isSafeMode()).toBe(false);
  });
});

describe("restartInSafeMode", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("arms the one-shot flag and reloads", async () => {
    const storage = stubStorage();
    const reload = vi.fn();
    vi.stubGlobal("location", { search: "", reload });
    const { restartInSafeMode, SAFE_MODE_ONCE_KEY } = await loadSafeMode();

    restartInSafeMode();
    expect(storage.get(SAFE_MODE_ONCE_KEY)).toBe("1");
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it("still reloads when storage refuses the flag", async () => {
    const reload = vi.fn();
    vi.stubGlobal("location", { search: "", reload });
    vi.stubGlobal("localStorage", {
      getItem: () => null,
      setItem: () => {
        throw new Error("blocked");
      },
      removeItem: () => {},
    });
    const { restartInSafeMode } = await loadSafeMode();

    restartInSafeMode();
    expect(reload).toHaveBeenCalledTimes(1);
  });
});
