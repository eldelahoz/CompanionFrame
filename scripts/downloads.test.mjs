import { describe, expect, it } from "vitest";
import { badgeJson, summarize } from "./downloads.mjs";

const asset = (name, download_count) => ({ name, download_count });

describe("summarize", () => {
  it("skips drafts", () => {
    const rows = summarize([
      { tag_name: "v1", draft: true, assets: [] },
      { tag_name: "v2", draft: false, published_at: "2026-11-10T12:00:00Z", assets: [] },
    ]);
    expect(rows.map((r) => r.tag)).toEqual(["v2"]);
  });

  it("reads installs from -setup.exe and updates from -update.exe", () => {
    const [row] = summarize([
      {
        tag_name: "v0.5.4",
        draft: false,
        published_at: "2026-11-10T12:00:00Z",
        assets: [
          asset("CompanionFrame_0.5.4_x64-setup.exe", 12),
          asset("CompanionFrame_0.5.4_x64-update.exe", 34),
          asset("CompanionFrame_0.5.4_x64-setup.exe.sig", 99),
          asset("latest.json", 500),
        ],
      },
    ]);
    expect(row).toEqual({ tag: "v0.5.4", published: "2026-11-10", installs: 12, updates: 34 });
  });

  it("gives updates null when there is no -update.exe, and never counts .sig or latest.json", () => {
    const [row] = summarize([
      {
        tag_name: "v0.5.3",
        draft: false,
        published_at: null,
        assets: [asset("CompanionFrame_0.5.3_x64-setup.exe.sig", 99), asset("latest.json", 500)],
      },
    ]);
    expect(row).toEqual({ tag: "v0.5.3", published: "", installs: 0, updates: null });
  });

  it("keeps a zero update count distinct from absent", () => {
    const [row] = summarize([{ tag_name: "v1", draft: false, assets: [asset("a-setup.exe", 1), asset("a-update.exe", 0)] }]);
    expect(row.updates).toBe(0);
  });
});

it("importing the module does not run the CLI", async () => {
  await expect(import("./downloads.mjs")).resolves.toBeDefined();
});

describe("badgeJson", () => {
  it("is a shields endpoint with a string message", () => {
    expect(badgeJson("installs", 1234)).toEqual({ schemaVersion: 1, label: "installs", message: "1234", color: "blue" });
  });

  it("renders zero as the string 0", () => {
    expect(badgeJson("updates", 0).message).toBe("0");
  });
});
