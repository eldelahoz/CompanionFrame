// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { translator } from "../../lib/i18n";
import { getSettings, resetMock, type Settings } from "../../lib/ipc";
import DataSourceMode from "./DataSourceMode";

const tr = translator("en");
const DISCLAIMER = /not affiliated with or endorsed by Digital Extremes/;

beforeEach(resetMock);
afterEach(cleanup);

const renderWith = async (patch: Partial<Settings> = {}) => {
  const onChange = vi.fn(async () => null);
  render(<DataSourceMode tr={tr} settings={{ ...(await getSettings()), ...patch }} onChange={onChange} />);
  return onChange;
};

test("passive is the default and shows no disclaimer", async () => {
  await renderWith();
  expect(document.querySelector('[data-data-source="passive"]')).toBeTruthy();
  expect(screen.queryByText(DISCLAIMER)).toBeNull();
});

test("memory mode is enabled only after the unchecked consent box is ticked", async () => {
  const onChange = await renderWith();
  fireEvent.click(document.querySelector('[data-data-source-option="memory"]')!);
  expect(screen.getByText(DISCLAIMER)).toBeTruthy();
  const box = document.querySelector<HTMLInputElement>("[data-memory-consent]")!;
  const enable = document.querySelector<HTMLButtonElement>("[data-memory-enable]")!;
  expect(box.checked).toBe(false);
  expect(enable.disabled).toBe(true);
  fireEvent.click(enable);
  expect(onChange).not.toHaveBeenCalled();
  fireEvent.click(box);
  fireEvent.click(enable);
  expect(onChange).toHaveBeenCalledWith({ data_source_mode: "memory", memory_consent_at: expect.any(Number) });
});

test("an active memory mode shows the helper panel and can switch back to passive", async () => {
  const onChange = await renderWith({ data_source_mode: "memory", memory_consent_at: 1 });
  expect(screen.getByText(DISCLAIMER)).toBeTruthy();
  expect(await screen.findByText("Run helper now")).toBeTruthy();
  fireEvent.click(document.querySelector('[data-data-source-option="passive"]')!);
  expect(onChange).toHaveBeenCalledWith({ data_source_mode: "passive" });
});
