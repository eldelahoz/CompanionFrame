import { describe, expect, it } from "vitest";

import { normalizeNotificationVolume } from "./notificationSound";
import {
  DEFAULT_SOURCE_CHANNELS,
  NOTIFICATION_SOURCES,
  ROUTABLE_NOTIFICATION_SOURCES,
  isNotificationSource,
  isWebhookChannel,
  tradeNotificationBody,
  tradeNotificationTitle,
} from "./notifications";

describe("notification sources", () => {
  it("gives every declared source a default route, with webhooks opt-in", () => {
    expect(Object.keys(DEFAULT_SOURCE_CHANNELS).sort()).toEqual([...NOTIFICATION_SOURCES].sort());
    for (const source of NOTIFICATION_SOURCES) {
      expect(DEFAULT_SOURCE_CHANNELS[source].webhook).toBe(source === "test");
    }
  });

  it("lists every source with a producer except the test fire", () => {
    expect([...ROUTABLE_NOTIFICATION_SOURCES].sort()).toEqual(
      NOTIFICATION_SOURCES.filter((s) => s !== "test").sort(),
    );
  });

  it("guards channel and source names", () => {
    expect(isWebhookChannel("discord")).toBe(true);
    expect(isWebhookChannel("slack")).toBe(false);
    expect(isNotificationSource("worldState")).toBe(true);
    expect(isNotificationSource("nope")).toBe(false);
    expect(isNotificationSource(3)).toBe(false);
  });
});

describe("trade toast text", () => {
  const match = { type: "sale", itemName: "Ash Prime Chassis", quantity: 1, platinum: 45, partner: "Buyer" };

  it("titles each status and falls back to a finished trade", () => {
    expect(tradeNotificationTitle("closed")).toBe("Listing Closed");
    expect(tradeNotificationTitle("match-failed")).toBe(tradeNotificationTitle("no-match"));
    expect(tradeNotificationTitle("bogus" as never)).toBe("Trade Finished");
  });

  it("names quantity, price and partner only when they mean something", () => {
    expect(tradeNotificationBody(match)).toBe("Ash Prime Chassis 45p with Buyer");
    expect(tradeNotificationBody({ ...match, quantity: 3, type: "purchase" })).toBe("3x Ash Prime Chassis 45p with Buyer");
    expect(tradeNotificationBody({ ...match, type: "gift", partner: "" })).toBe("Ash Prime Chassis");
  });
});

describe("notification sound volume", () => {
  it("clamps into 0..1 and keeps text at the default", () => {
    expect(normalizeNotificationVolume(0.25)).toBe(0.25);
    expect(normalizeNotificationVolume(7)).toBe(1);
    expect(normalizeNotificationVolume(-1)).toBe(0);
    expect(normalizeNotificationVolume("0.5")).toBe(1);
    expect(normalizeNotificationVolume(Number.NaN)).toBe(1);
  });
});
