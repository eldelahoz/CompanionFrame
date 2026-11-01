import { describe, expect, it } from "vitest";

import { rivenChatTag, rivenWtsLine } from "./rivenChatTag";

describe("rivenChatTag", () => {
  it("joins the weapon and the generated suffix with one space", () => {
    expect(rivenChatTag({ weaponName: "Rubico", rivenName: "Rubico Cronitor" })).toBe(
      "[Rubico Cronitor]",
    );
  });

  it("keeps an ampersand in the weapon name", () => {
    expect(rivenChatTag({ weaponName: "Ack & Brunt", rivenName: "Ack & Brunt Cronitor" })).toBe(
      "[Ack & Brunt Cronitor]",
    );
  });

  it("keeps a hyphenated riven name intact", () => {
    expect(rivenChatTag({ weaponName: "Angstrum", rivenName: "Angstrum Croni-visican" })).toBe(
      "[Angstrum Croni-visican]",
    );
  });

  it("accepts a bare suffix that does not repeat the weapon", () => {
    expect(rivenChatTag({ weaponName: "Ack & Brunt", rivenName: "Cronitor" })).toBe(
      "[Ack & Brunt Cronitor]",
    );
  });

  it("collapses stray whitespace instead of doubling the separator", () => {
    expect(
      rivenChatTag({ weaponName: "  Kuva  Bramma ", rivenName: "Kuva Bramma   Visitis" }),
    ).toBe("[Kuva Bramma Visitis]");
  });

  it("drops the suffix when the riven has no generated name", () => {
    expect(rivenChatTag({ weaponName: "Rubico", rivenName: "Rubico" })).toBe("[Rubico]");
    expect(rivenChatTag({ weaponName: "Rubico", rivenName: "" })).toBe("[Rubico]");
  });

  it("matches the prefix case-insensitively", () => {
    expect(rivenChatTag({ weaponName: "Ack & Brunt", rivenName: "ACK & BRUNT Cronitor" })).toBe(
      "[Ack & Brunt Cronitor]",
    );
  });
});

describe("rivenWtsLine", () => {
  it("renders the platinum price after the tag", () => {
    expect(rivenWtsLine({ weaponName: "Rubico", rivenName: "Rubico Cronitor" }, 120)).toBe(
      "WTS [Rubico Cronitor] 120p",
    );
  });

  it("rounds and floors the price", () => {
    const riven = { weaponName: "Rubico", rivenName: "Rubico Cronitor" };
    expect(rivenWtsLine(riven, 119.6)).toBe("WTS [Rubico Cronitor] 120p");
    expect(rivenWtsLine(riven, -5)).toBe("WTS [Rubico Cronitor] 0p");
    expect(rivenWtsLine(riven, Number.NaN)).toBe("WTS [Rubico Cronitor] 0p");
  });
});
