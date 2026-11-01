import { afterEach, describe, expect, it, vi } from "vitest";

import { stubStorage } from "../../test/storage";

// The store reads localStorage and the safe-mode flag at import time.
async function loadStore(): Promise<typeof import("./customCssStore")> {
  vi.resetModules();
  return import("./customCssStore");
}

describe("customCss store", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("starts empty with nothing persisted", async () => {
    vi.stubGlobal("location", { search: "" });
    stubStorage();
    const { customCss } = await loadStore();
    expect(customCss.get()).toEqual({ enabled: false, css: "", updatedAt: 0 });
  });

  it("round-trips through localStorage", async () => {
    const storage = stubStorage();
    vi.stubGlobal("location", { search: "" });

    const first = await loadStore();
    first.customCss.save(".x { color: red; }");
    first.customCss.setEnabled(true);
    expect(first.customCss.get().css).toBe(".x { color: red; }");

    const raw = storage.get(first.CUSTOM_CSS_STORAGE_KEY);
    expect(raw).toBeTruthy();

    const second = await loadStore();
    const reloaded = second.customCss.get();
    expect(reloaded.enabled).toBe(true);
    expect(reloaded.css).toBe(".x { color: red; }");
    expect(reloaded.updatedAt).toBeGreaterThan(0);
  });

  it("falls back to empty on corrupt or wrongly typed storage", async () => {
    vi.stubGlobal("location", { search: "" });
    stubStorage({ wf_custom_css_v1: "{not json" });
    expect((await loadStore()).customCss.get()).toEqual({ enabled: false, css: "", updatedAt: 0 });

    stubStorage({ wf_custom_css_v1: JSON.stringify({ enabled: "yes", css: 5, updatedAt: "x" }) });
    expect((await loadStore()).customCss.get()).toEqual({ enabled: false, css: "", updatedAt: 0 });
  });

  it("reset clears the saved sheet and the opt-in", async () => {
    vi.stubGlobal("location", { search: "" });
    stubStorage();
    const { customCss } = await loadStore();
    customCss.save(".x { color: red; }");
    customCss.setEnabled(true);
    customCss.reset();
    expect(customCss.get().enabled).toBe(false);
    expect(customCss.get().css).toBe("");
  });
});

describe("applyCustomCss", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("emits nothing while the opt-in is off", async () => {
    vi.stubGlobal("location", { search: "" });
    stubStorage();
    const { applyCustomCss } = await loadStore();
    expect(applyCustomCss({ enabled: false, css: ".x { color: red; }", updatedAt: 0 })).toEqual({
      css: "",
      warnings: [],
    });
  });

  it("sanitises and scopes the saved sheet when enabled", async () => {
    vi.stubGlobal("location", { search: "" });
    stubStorage();
    const { applyCustomCss, customCss } = await loadStore();

    const result = applyCustomCss({
      enabled: true,
      css: ".x { color: red; } @import 'a.css';",
      updatedAt: 0,
    });
    expect(result.css).toBe("#shell .x {\n  color: red;\n}");
    expect(result.warnings.map((warning) => warning.reason)).toEqual(["atImport"]);

    customCss.save(".x { color: red; }");
    customCss.setEnabled(true);
    expect(applyCustomCss().css).toBe("#shell .x {\n  color: red;\n}");
  });

  it("emits nothing in safe mode even when enabled", async () => {
    vi.stubGlobal("location", { search: "?safe=1" });
    stubStorage();
    const { applyCustomCss, customCss, safeMode } = await loadStore();

    expect(safeMode).toBe(true);
    expect(applyCustomCss({ enabled: true, css: ".x { color: red; }", updatedAt: 0 })).toEqual({
      css: "",
      warnings: [],
    });
    customCss.save(".x { color: red; }");
    customCss.setEnabled(true);
    expect(applyCustomCss().css).toBe("");
  });
});
