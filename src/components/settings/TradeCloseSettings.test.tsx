// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { translator } from "../../lib/i18n";
import type { Settings } from "../../lib/ipc";
import TradeCloseSettings from "./TradeCloseSettings";

afterEach(cleanup);
const tr = translator("en");
const base = { auto_close_trade_listings: false, trade_thanks_enabled: false, trade_thanks_message: "Thanks!" } as Settings;

it("toggles auto-close and the thank-you switch", () => {
  const onChange = vi.fn(async () => null);
  render(<TradeCloseSettings settings={base} onChange={onChange} tr={tr} />);
  fireEvent.click(screen.getByLabelText(tr("settings.autoCloseListings")));
  expect(onChange).toHaveBeenCalledWith({ auto_close_trade_listings: true });
  fireEvent.click(screen.getByLabelText(tr("settings.thanksEnabled")));
  expect(onChange).toHaveBeenCalledWith({ trade_thanks_enabled: true });
});

it("disables the message while the thank-you is off and saves it on blur", () => {
  const onChange = vi.fn(async () => null);
  const { rerender } = render(<TradeCloseSettings settings={base} onChange={onChange} tr={tr} />);
  const input = screen.getByLabelText(tr("settings.thanksMessage")) as HTMLInputElement;
  expect(input.disabled).toBe(true);
  rerender(<TradeCloseSettings settings={{ ...base, trade_thanks_enabled: true }} onChange={onChange} tr={tr} />);
  expect(input.disabled).toBe(false);
  fireEvent.change(input, { target: { value: "ty <3" } });
  expect(onChange).not.toHaveBeenCalled();
  fireEvent.blur(input);
  expect(onChange).toHaveBeenCalledWith({ trade_thanks_message: "ty <3" });
});
