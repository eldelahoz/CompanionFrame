import { expect, it } from "vitest";
import { itemLabel } from "./itemLabel";

it("prefers the game-language name, then English, then the unique name", () => {
  expect(itemLabel("/a/B", { name: "Boltor", displayName: "<ENDO> Bolzer" })).toBe("Bolzer");
  expect(itemLabel("/a/B", { name: "Boltor" })).toBe("Boltor");
  expect(itemLabel("/Lotus/Weapons/BoltoPrime")).toBe("Bolto Prime");
});
