// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { translator } from "../../lib/i18n";
import { ocrLanguageInstall, ocrLanguageStatus, type Settings } from "../../lib/ipc";
import OverlaySettings from "./OverlaySettings";

vi.mock("../../lib/ipc", async (orig) => ({
  ...(await orig<typeof import("../../lib/ipc")>()),
  ocrLanguageStatus: vi.fn(async () => ({ gameLanguage: "en", installed: true, installable: true })),
  ocrLanguageInstall: vi.fn(async () => "cancelled"),
}));

afterEach(cleanup);
const tr = translator("en");
const base = { reward_scan_hotkey: "F8", overlay_move_hotkey: "F7", riven_rescan_hotkey: "F6", overlay_enabled: {}, overlay_opacity: {} } as Settings;

it("saves the enabled switch and the opacity of one overlay", () => {
  const onChange = vi.fn(async () => null);
  render(<OverlaySettings settings={base} onChange={onChange} tr={tr} />);
  fireEvent.click(screen.getByLabelText(tr("settings.overlayKindArbi")));
  expect(onChange).toHaveBeenCalledWith({ overlay_enabled: { arbi: false } });
  fireEvent.change(screen.getAllByRole("slider")[0], { target: { value: "55" } });
  expect(onChange).toHaveBeenCalledWith({ overlay_opacity: { reward: 55 } });
});

it("captures a hotkey and shows the backend's refusal", async () => {
  const onChange = vi.fn(async () => "hotkey F9 is already used");
  render(<OverlaySettings settings={base} onChange={onChange} tr={tr} />);
  fireEvent.click(screen.getByText(/^F8/));
  await waitFor(() => expect(onChange).toHaveBeenCalledWith({ reward_scan_hotkey: "Ctrl+Shift+F9" }));
  expect((await screen.findByRole("alert")).textContent).toContain("already used");
});

it("saves the whisper switches and disables the focused one while notifications are off", () => {
  const onChange = vi.fn(async () => null);
  const s = { ...base, message_notifications_enabled: true, message_notifications_while_focused: false } as Settings;
  const { rerender } = render(<OverlaySettings settings={s} onChange={onChange} tr={tr} />);
  fireEvent.click(screen.getByLabelText(new RegExp(tr("settings.notifyWhileFocused"))));
  expect(onChange).toHaveBeenCalledWith({ message_notifications_while_focused: true });
  fireEvent.click(screen.getByLabelText(tr("settings.inGameMessageNotifications")));
  expect(onChange).toHaveBeenCalledWith({ message_notifications_enabled: false });
  rerender(<OverlaySettings settings={{ ...s, message_notifications_enabled: false }} onChange={onChange} tr={tr} />);
  expect((screen.getByLabelText(new RegExp(tr("settings.notifyWhileFocused"))) as HTMLInputElement).disabled).toBe(true);
});

const openAdvanced = () => fireEvent.click(screen.getByRole("button", { name: tr("settings.advanced") }));

it("keeps the optional Windows OCR pack note under Advanced (French game, en-US only)", async () => {
  vi.mocked(ocrLanguageStatus).mockResolvedValueOnce({ gameLanguage: "fr", installed: false, installable: true });
  render(<OverlaySettings settings={base} onChange={vi.fn(async () => null)} tr={tr} />);
  expect(screen.queryByRole("status")).toBeNull();
  openAdvanced();
  expect((await screen.findByRole("status")).textContent).toContain("Optional");
});

it("installs the OCR pack only on click and explains a cancelled UAC prompt", async () => {
  vi.mocked(ocrLanguageStatus).mockResolvedValueOnce({ gameLanguage: "fr", installed: false, installable: true });
  render(<OverlaySettings settings={base} onChange={vi.fn(async () => null)} tr={tr} />);
  openAdvanced();
  const button = await screen.findByRole("button", { name: "Install Français text recognition" });
  expect(ocrLanguageInstall).not.toHaveBeenCalled();
  fireEvent.click(button);
  expect((await screen.findByRole("alert")).textContent).toContain("cancelled");
  expect(ocrLanguageInstall).toHaveBeenCalledTimes(1);
});

it("offers no install button for a language Windows has no OCR pack for", async () => {
  vi.mocked(ocrLanguageStatus).mockResolvedValueOnce({ gameLanguage: "uk", installed: false, installable: false });
  render(<OverlaySettings settings={base} onChange={vi.fn(async () => null)} tr={tr} />);
  openAdvanced();
  await screen.findByRole("status");
  expect(screen.queryByRole("button", { name: /text recognition/ })).toBeNull();
});

it("labels the move and scan hotkeys and saves the scan-capture switch", () => {
  const onChange = vi.fn(async () => null);
  render(<OverlaySettings settings={base} onChange={onChange} tr={tr} />);
  expect(screen.getByText(tr("settings.moveOverlaysHotkey"), { selector: "span" })).toBeTruthy();
  expect(screen.getByText(tr("settings.rewardScanHotkey"))).toBeTruthy();
  expect(screen.getByRole("button", { name: tr("settings.moveOverlays") })).toBeTruthy();
  openAdvanced();
  fireEvent.click(screen.getByLabelText(new RegExp(tr("settings.saveScanCaptures"))));
  expect(onChange).toHaveBeenCalledWith({ save_scan_captures: true });
});
