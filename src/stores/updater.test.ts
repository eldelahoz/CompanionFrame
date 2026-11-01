// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";

const check = vi.fn();
const relaunch = vi.fn();
const openUrl = vi.fn();
vi.mock("@tauri-apps/plugin-updater", () => ({ check }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl }));
vi.mock("../lib/ipc", () => ({ inTauri: () => true }));

const { checkForUpdate, dismissUpdate, getUpdaterState, installUpdate, openReleaseNotes, resetUpdater, startAutoCheck, CHECK_DELAY_MS, CHECK_INTERVAL_MS } = await import("./updater");

const found = (version = "0.3.0", downloadAndInstall = vi.fn().mockResolvedValue(undefined)) => ({ version, downloadAndInstall });

beforeEach(() => {
  vi.clearAllMocks();
  resetUpdater();
});

describe("checkForUpdate", () => {
  it("reports up to date when there is no update", async () => {
    check.mockResolvedValue(null);
    await checkForUpdate(true);
    expect(getUpdaterState()).toMatchObject({ status: "uptodate", version: null });
  });

  it("offers a found update", async () => {
    check.mockResolvedValue(found());
    await checkForUpdate(false);
    expect(getUpdaterState()).toMatchObject({ status: "available", version: "0.3.0", dismissed: false });
  });

  it("shows the error of a manual check", async () => {
    check.mockRejectedValue(new Error("offline"));
    await checkForUpdate(true);
    expect(getUpdaterState()).toMatchObject({ status: "error", error: "offline" });
  });

  it("swallows the error of an automatic check", async () => {
    check.mockRejectedValue(new Error("offline"));
    await checkForUpdate(false);
    expect(getUpdaterState()).toMatchObject({ status: "idle", error: null });
  });

  it("keeps a known update when a later automatic check fails", async () => {
    check.mockResolvedValueOnce(found());
    await checkForUpdate(false);
    check.mockRejectedValueOnce(new Error("offline"));
    await checkForUpdate(false);
    expect(getUpdaterState()).toMatchObject({ status: "available", version: "0.3.0" });
  });
});

describe("Later", () => {
  it("hides the banner for that version across the 6 h checks, but a newer one comes back", async () => {
    check.mockResolvedValue(found());
    await checkForUpdate(false);
    dismissUpdate();
    expect(getUpdaterState().dismissed).toBe(true);
    await checkForUpdate(false);
    expect(getUpdaterState().dismissed).toBe(true);
    check.mockResolvedValue(found("0.3.1"));
    await checkForUpdate(false);
    expect(getUpdaterState()).toMatchObject({ version: "0.3.1", dismissed: false });
  });

  it("a manual check shows it again", async () => {
    check.mockResolvedValue(found());
    await checkForUpdate(false);
    dismissUpdate();
    await checkForUpdate(true);
    expect(getUpdaterState().dismissed).toBe(false);
  });
});

describe("installUpdate", () => {
  it("tracks the download, then relaunches", async () => {
    const seen: Array<number | null> = [];
    const dl = vi.fn(async (cb: (e: unknown) => void) => {
      cb({ event: "Started", data: { contentLength: 200 } });
      cb({ event: "Progress", data: { chunkLength: 50 } });
      seen.push(getUpdaterState().progress);
      cb({ event: "Finished" });
    });
    check.mockResolvedValue(found("0.3.0", dl as never));
    await checkForUpdate(false);
    await installUpdate();
    expect(seen).toEqual([25]);
    expect(getUpdaterState().status).toBe("installing");
    expect(relaunch).toHaveBeenCalledOnce();
  });

  it("surfaces a failed install and lets the user retry", async () => {
    check.mockResolvedValue(found("0.3.0", vi.fn().mockRejectedValue(new Error("bad signature"))));
    await checkForUpdate(false);
    await installUpdate();
    expect(getUpdaterState()).toMatchObject({ status: "error", error: "bad signature", version: "0.3.0" });
    expect(relaunch).not.toHaveBeenCalled();
  });
});

it("opens the release page of the offered version", async () => {
  check.mockResolvedValue(found());
  await checkForUpdate(false);
  openReleaseNotes();
  expect(openUrl).toHaveBeenCalledWith("https://github.com/eldelahoz/CompanionFrame/releases/tag/v0.3.0");
});

it("checks after a delay and then every 6 hours until stopped", async () => {
  vi.useFakeTimers();
  check.mockResolvedValue(null);
  const stop = startAutoCheck();
  await vi.advanceTimersByTimeAsync(CHECK_DELAY_MS - 1);
  expect(check).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(1);
  expect(check).toHaveBeenCalledTimes(1);
  await vi.advanceTimersByTimeAsync(CHECK_INTERVAL_MS);
  expect(check).toHaveBeenCalledTimes(2);
  stop();
  await vi.advanceTimersByTimeAsync(CHECK_INTERVAL_MS);
  expect(check).toHaveBeenCalledTimes(2);
  vi.useRealTimers();
});
