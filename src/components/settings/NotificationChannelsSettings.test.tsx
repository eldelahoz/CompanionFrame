// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "vitest";
import { translator } from "../../lib/i18n";
import { mockNotify, notifSoundGet, resetMock } from "../../lib/ipc";
import { resetNotificationsForTest } from "../../stores/notifications";
import ToastHost from "../shell/ToastHost";
import { clearToasts } from "../../stores/toasts";
import NotificationHistory from "../NotificationHistory";
import NotificationChannelsSettings from "./NotificationChannelsSettings";

const tr = translator("en");
const url = "https://discord.com/api/webhooks/123/abcdef-token";

beforeEach(() => {
  resetMock();
  resetNotificationsForTest();
  clearToasts();
});
afterEach(cleanup);

const discord = () => screen.getByLabelText(tr("settings.webhookDiscord")) as HTMLInputElement;
// jsdom Files lack arrayBuffer()
const clip = (name: string) => Object.assign(new File([new Uint8Array([1, 2, 3])], name), { arrayBuffer: async () => new ArrayBuffer(3) });
const row = (source: string) => document.querySelector(`[data-setting="notify-source-${source}"]`)!;

test("a saved webhook shows masked, never the full URL, and enables test and clear", async () => {
  render(<><ToastHost tr={tr} /><NotificationChannelsSettings tr={tr} /></>);
  await screen.findByLabelText(tr("settings.webhookDiscord"));
  const save = screen.getAllByRole("button", { name: tr("common.save") })[0] as HTMLButtonElement;
  expect(save.disabled).toBe(true);

  fireEvent.change(discord(), { target: { value: url } });
  fireEvent.click(save);
  await screen.findByText(tr("settings.webhookSaved"));
  const masked = document.querySelector("[data-webhook-masked]")!.textContent!;
  expect(masked).toContain("...");
  expect(masked).not.toContain("abcdef-token");
  expect(discord().value).toBe("");
  expect((screen.getAllByRole("button", { name: tr("settings.webhookTest") })[0] as HTMLButtonElement).disabled).toBe(false);

  fireEvent.click(screen.getAllByRole("button", { name: tr("settings.webhookTest") })[0]);
  await screen.findByText(tr("settings.webhookTestSent"));
  fireEvent.click(screen.getAllByRole("button", { name: tr("settings.webhookClear") })[0]);
  await screen.findByText(tr("settings.webhookCleared"));
  expect(document.querySelector("[data-webhook-masked]")).toBeNull();
});

test("an http URL is rejected with its message and nothing is stored", async () => {
  render(<><ToastHost tr={tr} /><NotificationChannelsSettings tr={tr} /></>);
  await screen.findByLabelText(tr("settings.webhookDiscord"));
  fireEvent.change(discord(), { target: { value: "http://discord.com/x" } });
  fireEvent.click(screen.getAllByRole("button", { name: tr("common.save") })[0]);
  expect(await screen.findByText(tr("settings.webhookErrorNotHttps"))).toBeTruthy();
  expect(document.querySelector("[data-webhook-masked]")).toBeNull();
});

test("an invalid Discord user id is refused, a valid one is saved", async () => {
  render(<><ToastHost tr={tr} /><NotificationChannelsSettings tr={tr} /></>);
  await screen.findByLabelText(tr("settings.webhookDiscord"));
  const ping = document.querySelector("[data-discord-ping-input]") as HTMLInputElement;
  fireEvent.change(ping, { target: { value: "12ab" } });
  fireEvent.blur(ping);
  expect(await screen.findByText(tr("settings.discordPingInvalid"))).toBeTruthy();
  fireEvent.change(ping, { target: { value: "12345678901234567" } });
  fireEvent.blur(ping);
  await screen.findByText(tr("settings.saved"));
});

test("routing toggles and the game gate persist", async () => {
  render(<><ToastHost tr={tr} /><NotificationChannelsSettings tr={tr} /></>);
  await screen.findByLabelText(tr("settings.webhookDiscord"));
  const webhookBox = row("whisper").querySelectorAll("input")[1] as HTMLInputElement;
  expect(webhookBox.checked).toBe(false);
  fireEvent.click(webhookBox);
  await waitFor(() => expect((row("whisper").querySelectorAll("input")[1] as HTMLInputElement).checked).toBe(true));
  const gate = screen.getByLabelText(new RegExp(tr("settings.notifyOnlyWhileGameRunning"))) as HTMLInputElement;
  fireEvent.click(gate);
  await waitFor(() => expect(gate.checked).toBe(true));
});

test("choosing a clip stores it, reset deletes it, and preview needs the sound enabled", async () => {
  // jsdom has no audio decoding: stand in for the browser codec path.
  const wav = new Uint8Array([1, 2, 3]);
  (globalThis as { AudioContext?: unknown }).AudioContext = class {
    decodeAudioData = async () => ({ duration: 0.1 });
    close = async () => {};
  };
  (globalThis as { OfflineAudioContext?: unknown }).OfflineAudioContext = class {
    destination = {};
    createBufferSource = () => ({ connect() {}, start() {}, disconnect() {} });
    startRendering = async () => ({ getChannelData: () => new Float32Array(wav.length) });
  };
  render(<><ToastHost tr={tr} /><NotificationChannelsSettings tr={tr} /></>);
  await screen.findByLabelText(tr("settings.webhookDiscord"));
  const input = document.querySelector("[data-setting='notification-sound-file']") as HTMLInputElement;
  const reset = document.querySelector("[data-setting='notification-sound-reset']") as HTMLButtonElement;
  // the sound row loads its clip on its own; choosing while it loads is ignored by design
  await waitFor(() => expect(input.disabled).toBe(false));
  expect(reset.disabled).toBe(true);

  fireEvent.change(input, { target: { files: [clip("ping.wav")] } });
  await waitFor(() => expect(document.querySelector("[data-notification-sound-name]")!.textContent).toBe("ping.wav"));
  expect((await notifSoundGet())?.name).toBe("ping.wav");

  fireEvent.click(reset);
  await waitFor(() => expect(document.querySelector("[data-notification-sound-name]")!.textContent).toBe(tr("common.default")));
  expect(await notifSoundGet()).toBeNull();

  fireEvent.change(input, { target: { files: [clip("bad.txt")] } });
  expect((await screen.findByText(tr("settings.notificationSoundInvalid")))).toBeTruthy();
  expect(await notifSoundGet()).toBeNull();

  const preview = document.querySelector("[data-setting='notification-sound-preview']") as HTMLButtonElement;
  expect(preview.disabled).toBe(false);
  fireEvent.click(screen.getByLabelText(tr("settings.windowsNotifSound")));
  await waitFor(() => expect(preview.disabled).toBe(true));
});

test("the history lists pushed notifications, removes one and clears all", async () => {
  render(<NotificationHistory tr={tr} locale="en" onClose={() => {}} />);
  expect(document.querySelector("[data-notification-empty]")).not.toBeNull();
  const { addNotificationEntry } = await import("../../stores/notifications");
  act(() => {
    addNotificationEntry(mockNotify("trade", "Sold", "2x Rhino"));
    addNotificationEntry(mockNotify("world", "Cycle", "Night soon"));
  });
  expect(document.querySelectorAll("[data-notification-entry]")).toHaveLength(2);
  fireEvent.click(document.querySelector("[data-notification-remove]")!);
  await waitFor(() => expect(document.querySelectorAll("[data-notification-entry]")).toHaveLength(1));
  fireEvent.click(document.querySelector("[data-notification-clear]")!);
  await waitFor(() => expect(document.querySelector("[data-notification-empty]")).not.toBeNull());
});
