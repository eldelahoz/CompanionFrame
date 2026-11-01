import { expect, it } from "vitest";
import { presencePill } from "./presencePill";

const p = (status: "online" | "ingame" | "invisible" | null) => ({ status, expires_at: null, auto_active: false, away_active: false });

it("reads signed out, socket trouble, then the presence status", () => {
  expect(presencePill(false, "connected", p("ingame")).label).toBe("market.pill.signedOut");
  expect(presencePill(true, "reconnecting", p("ingame"))).toEqual({ label: "market.pill.connecting", tone: "warning" });
  expect(presencePill(true, "gave_up", p("ingame"))).toEqual({ label: "market.pill.disconnected", tone: "danger" });
  expect(presencePill(true, "connected", p("ingame"))).toEqual({ label: "market.pill.ingame", tone: "success" });
  expect(presencePill(true, "stopped", p("online")).label).toBe("market.pill.online");
  expect(presencePill(true, "connected", p("invisible")).tone).toBe("muted");
  expect(presencePill(true, "connected", null).label).toBe("market.pill.connected");
});
