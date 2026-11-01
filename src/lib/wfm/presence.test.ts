import { expect, test } from "vitest";
import { DEFAULT_PRESENCE_OPTIONS, parsePresenceOptions } from "./presence";
import { formatHoldRemaining } from "../../components/market/WfmPresenceBar";

test("garbage presence options fall back to defaults field by field", () => {
  expect(parsePresenceOptions(null)).toEqual(DEFAULT_PRESENCE_OPTIONS);
  expect(parsePresenceOptions({ auto_ingame_enabled: true, hold_minutes: 999, away_idle_minutes: "x" })).toEqual({
    ...DEFAULT_PRESENCE_OPTIONS,
    auto_ingame_enabled: true,
  });
});

test("hold countdown formats h/m/s", () => {
  expect(formatHoldRemaining(null, 0)).toBe("");
  expect(formatHoldRemaining(90_000, 0)).toBe("1m");
  expect(formatHoldRemaining(3_900_000, 0)).toBe("1h 05m");
  expect(formatHoldRemaining(5_000, 0)).toBe("5s");
});
