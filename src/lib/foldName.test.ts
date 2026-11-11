import { expect, test } from "vitest";
import { foldName } from "./foldName";

test("foldName drops case, accents and extra spaces", () => {
  expect(foldName("  Éclat   PRIME ")).toBe("eclat prime");
  expect(foldName("Pièce Sérration")).toBe("piece serration");
  expect(foldName("")).toBe("");
});
