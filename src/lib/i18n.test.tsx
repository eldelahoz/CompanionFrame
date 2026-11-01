// @vitest-environment jsdom
import { expect, test } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { translator } from "./i18n";
import App from "../App";

test("translates, interpolates and falls back to the key", () => {
  expect(translator("de")("common.settings")).not.toBe("Settings");
  expect(translator("en")("common.settings")).toBe("Settings");
  expect(translator("de")("nav.nonexistent" as never)).toBe("nav.nonexistent");
});

test("missing de/zh key falls back to English", async () => {
  const en = (await import("../i18n/en.json")).default as Record<string, string>;
  const de = (await import("../i18n/de.json")).default as Record<string, string>;
  const missing = Object.keys(en).find((k) => !(k in de));
  if (missing) expect(translator("de")(missing as never)).toBe(en[missing]);
});

test("every locale file has every key in en.json", () => {
  const files = import.meta.glob<Record<string, string>>("../i18n/*.json", { eager: true, import: "default" });
  const en = files["../i18n/en.json"];
  for (const [path, dict] of Object.entries(files)) {
    expect(Object.keys(en).filter((k) => !(k in dict)), path).toEqual([]);
  }
});

test("es.json has exactly en.json's keys and the same placeholders per key", async () => {
  const en = (await import("../i18n/en.json")).default as Record<string, string>;
  const es = (await import("../i18n/es.json")).default as Record<string, string>;
  expect(Object.keys(es).sort()).toEqual(Object.keys(en).sort());
  const holders = (v: string) => (v.match(/\{\w+\}/g) ?? []).sort();
  const drifted = Object.keys(en).filter((k) => holders(en[k]).join() !== holders(es[k]).join());
  expect(drifted).toEqual([]);
});

test("switching language in settings relabels the sidebar", async () => {
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: "Settings" }));
  fireEvent.change(await screen.findByDisplayValue("English"), { target: { value: "de" } });
  expect(await screen.findByRole("button", { name: "Einstellungen" })).toBeTruthy();
  expect(document.documentElement.lang).toBe("de");
});

test("every literal tr(\"key\") in src exists in en.json", () => {
  const en = import.meta.glob<Record<string, string>>("../i18n/en.json", { eager: true, import: "default" })["../i18n/en.json"];
  const srcs = import.meta.glob<string>(["../**/*.{ts,tsx}", "!../**/*.test.*", "!../bindings.ts"], { eager: true, query: "?raw", import: "default" });
  const missing = Object.entries(srcs).flatMap(([file, s]) =>
    [...s.matchAll(/\btr\(\s*"([^"]+)"/g)].filter((m) => !(m[1] in en)).map((m) => `${file}: ${m[1]}`),
  );
  expect(missing).toEqual([]);
});
