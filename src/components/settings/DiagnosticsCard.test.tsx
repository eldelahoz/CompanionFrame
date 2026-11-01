// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { translator } from "../../lib/i18n";
import DiagnosticsCard, { diagnosticsGroups, diagnosticsText, redactHome } from "./DiagnosticsCard";

const ipc = vi.hoisted(() => ({ eelogStatus: vi.fn(), ocrOnnxStatus: vi.fn(), captureGameWindowInfo: vi.fn(), rewardScanDebug: vi.fn() }));
vi.mock("../../lib/ipc", () => ipc);

const tr = translator("en");
const win = { found: true, x: 0, y: 0, width: 1920, height: 1080, dpi: 96, source: "window", black: false, error: null };

beforeEach(() => {
  ipc.eelogStatus.mockResolvedValue({ path: "C:/EE.log", active: true, bytes_read: 42, dbwin_active: false });
  ipc.ocrOnnxStatus.mockResolvedValue({ runtimePresent: true, paddlePresent: true, detectorPresent: false, ready: true, error: null });
  ipc.captureGameWindowInfo.mockResolvedValue(win);
  ipc.rewardScanDebug.mockResolvedValue({ cards: 2, slots: [["Forma"], ["Ash Prime Systems", "Ash Prime"]] });
});
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("DiagnosticsCard", () => {
  it("renders the three statuses and runs the scan debug on demand", async () => {
    render(<DiagnosticsCard tr={tr} />);
    expect(await screen.findByText("C:/EE.log")).toBeTruthy();
    expect(screen.getByText("1920 x 1080")).toBeTruthy();
    expect(screen.getByText(tr("diag.scanHint"))).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: tr("diag.runScan") }));
    expect(await screen.findByText("Ash Prime Systems | Ash Prime")).toBeTruthy();
  });

  it("shows a per-section error without hiding the others", async () => {
    ipc.ocrOnnxStatus.mockRejectedValue(new Error("no resources"));
    render(<DiagnosticsCard tr={tr} />);
    expect((await screen.findByRole("alert")).textContent).toContain("no resources");
    expect(screen.getByText("C:/EE.log")).toBeTruthy();
  });

  it("copies every section as text", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    render(<DiagnosticsCard tr={tr} />);
    await screen.findByText("C:/EE.log");
    fireEvent.click(screen.getByRole("button", { name: tr("diag.copy") }));
    await waitFor(() => expect(writeText).toHaveBeenCalled());
    const text = writeText.mock.calls[0][0] as string;
    expect(text).toContain("## EE.log");
    expect(text).toContain("Path: C:/EE.log");
    expect(text).toContain("## Game window");
  });
});

describe("diagnosticsText", () => {
  it("skips sections that have not loaded", () => {
    const g = diagnosticsGroups(tr, { ee: null, ocr: { error: "boom" }, win: null, scan: { data: null } });
    expect(diagnosticsText(g)).toBe(`## ${tr("diag.ocr")}\nboom\n\n## ${tr("diag.scan")}\n${tr("diag.scanCards")}: ${tr("diag.scanNone")}`);
  });
});

describe("redactHome", () => {
  it("hides the user name in Windows, macOS and Linux paths", () => {
    expect(redactHome(String.raw`C:\Users\Ana Lopez\AppData\Local\Warframe\EE.log`)).toBe(String.raw`%USERPROFILE%\AppData\Local\Warframe\EE.log`);
    expect(redactHome("c:/users/ana/x/EE.log")).toBe("%USERPROFILE%/x/EE.log");
    expect(redactHome("/home/ana/.steam/EE.log\n/Users/bob/y")).toBe("~/.steam/EE.log\n~/y");
    expect(redactHome("D:/Games/EE.log")).toBe("D:/Games/EE.log");
  });
  it("is applied to the copied text", () => {
    const g = diagnosticsGroups(tr, { ee: { data: { path: String.raw`C:\Users\ana\EE.log`, active: true, bytes_read: 1, dbwin_active: false } }, ocr: null, win: null, scan: null });
    expect(diagnosticsText(g)).not.toContain("ana");
  });
});
