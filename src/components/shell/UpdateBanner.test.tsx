// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const check = vi.fn();
vi.mock("@tauri-apps/plugin-updater", () => ({ check }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("../../lib/ipc", () => ({ inTauri: () => true, appInfo: async () => ({ version: "0.2.0" }) }));

const { checkForUpdate, resetUpdater } = await import("../../stores/updater");
const { default: UpdateBanner } = await import("./UpdateBanner");
const { default: UpdatesCard } = await import("../settings/UpdatesCard");
const tr = ((k: string, p?: Record<string, string>) => (p ? `${k}:${Object.values(p).join(",")}` : k)) as never;

beforeEach(() => resetUpdater());
afterEach(cleanup);

describe("UpdateBanner", () => {
  it("renders nothing without an update", () => {
    render(<UpdateBanner tr={tr} />);
    expect(document.querySelector("[data-update-banner]")).toBeNull();
  });

  it("offers the update and hides on Later", async () => {
    check.mockResolvedValue({ version: "0.3.0", downloadAndInstall: vi.fn() });
    render(<UpdateBanner tr={tr} />);
    await act(() => checkForUpdate(false));
    expect(screen.getByText("update.available:0.3.0")).toBeTruthy();
    expect(screen.getByText("update.now")).toBeTruthy();
    expect(screen.getByText("update.notes")).toBeTruthy();
    fireEvent.click(screen.getByText("update.later"));
    expect(document.querySelector("[data-update-banner]")).toBeNull();
  });
});

describe("UpdatesCard", () => {
  const settings = { auto_update_check: true } as never;

  it("saves the automatic-check switch", () => {
    const onChange = vi.fn();
    render(<UpdatesCard tr={tr} settings={settings} onChange={onChange} />);
    fireEvent.click(screen.getByRole("switch"));
    expect(onChange).toHaveBeenCalledWith({ auto_update_check: false });
  });

  it("shows up to date after a manual check", async () => {
    check.mockResolvedValue(null);
    render(<UpdatesCard tr={tr} settings={settings} onChange={vi.fn()} />);
    fireEvent.click(screen.getByText("update.checkNow"));
    expect(await screen.findByText("update.upToDate")).toBeTruthy();
  });

  it("shows the error of a manual check", async () => {
    check.mockRejectedValue(new Error("offline"));
    render(<UpdatesCard tr={tr} settings={settings} onChange={vi.fn()} />);
    fireEvent.click(screen.getByText("update.checkNow"));
    expect(await screen.findByText("update.failed:offline")).toBeTruthy();
  });
});
