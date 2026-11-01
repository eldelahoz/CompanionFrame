import { describe, expect, it } from "vitest";
import { rivenOverlayApplyLabel, rivenSessionEnd, rivenSessionRollDone, rivenSessionStart } from "./ipc";

describe("riven roll session (mock)", () => {
  it("counts rolls and kuva, and refuses a roll with no session", async () => {
    await rivenSessionEnd();
    await expect(rivenSessionRollDone()).rejects.toThrow();
    await rivenSessionStart(900);
    await rivenSessionRollDone();
    expect(await rivenSessionRollDone()).toEqual({ kuvaPerRoll: 900, rollCount: 2, totalKuvaSpent: 1800 });
    await rivenSessionEnd();
  });
  it("keeps the weapon when there is no backend", async () => {
    expect(await rivenOverlayApplyLabel(["Rubico"])).toBeNull();
  });
});
