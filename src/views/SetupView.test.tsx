// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { translator } from "../lib/i18n";
import { getOnboarding, getProfileAccount, getSettings, resetMock } from "../lib/ipc";
import SetupView from "./SetupView";

const tr = translator("en");
const setup = (props: Partial<React.ComponentProps<typeof SetupView>> = {}) => {
  const onComplete = vi.fn();
  render(<SetupView tr={tr} onComplete={onComplete} {...props} />);
  return onComplete;
};
/** From the step after the inventory to the end. */
const finish = async () => {
  await screen.findByText("Place your overlays");
  fireEvent.click(screen.getByText("Next"));
  fireEvent.click(await screen.findByText("Finish", { selector: "button" }));
};

beforeEach(resetMock);
afterEach(cleanup);

const withSettings = async () => setup({ settings: await getSettings(), onChange: vi.fn(async () => null) });

test("passive is picked and its three sources are usable right in the step, with no helper", async () => {
  await withSettings();
  await screen.findByText("Bring your inventory");
  expect(document.querySelector('[data-setup-data-source] [data-data-source="passive"]')).toBeTruthy();
  expect(document.querySelector('[data-data-source-panel="passive"]')).toBeTruthy();
  for (const name of ["AlecaFrame cache (lastData.dat)", "Inventory file (inventory.json)", "Profile account ID"]) expect(screen.getByText(name)).toBeTruthy();
  expect(screen.getByText("Choose file")).toBeTruthy();
  expect(document.querySelector("[data-app-disclaimer]")!.textContent).toMatch(/third-party tool.*at your own risk.*MIT License.*including action against your account/);
  expect(document.querySelector<HTMLAnchorElement>("[data-app-disclaimer] a")!.href).toMatch(/LICENSE$/);
  expect(document.querySelector("[data-memory-disclaimer]")).toBeNull();
});

test("game memory shows the disclaimer and an unticked consent", async () => {
  await withSettings();
  fireEvent.click(await screen.findByText("Game memory (warframe-api-helper)"));
  expect(document.querySelector("[data-memory-disclaimer]")).toBeTruthy();
  expect(document.querySelector<HTMLInputElement>("[data-memory-consent]")!.checked).toBe(false);
  expect(document.querySelector<HTMLButtonElement>("[data-memory-enable]")!.disabled).toBe(true);
  expect(document.querySelector("[data-app-disclaimer]")).toBeTruthy();
});

test("a found AlecaFrame cache loads in one click and Next leads to the inventory", async () => {
  const onComplete = await withSettings();
  await waitFor(() => expect(document.querySelector('[data-aleca-state="found"]')).toBeTruthy());
  fireEvent.click(document.querySelector("[data-aleca-use]")!);
  await waitFor(() => expect(document.querySelector('[data-aleca-state="inUse"]')).toBeTruthy());
  fireEvent.click(screen.getByText("Next"));
  await waitFor(() => expect(document.querySelector('[data-setup-step="configure"]')).toBeTruthy());
  fireEvent.click(screen.getByText("Next"));
  await finish();
  await waitFor(() => expect(onComplete).toHaveBeenCalledWith("inventory"));
});

test("a pasted profile link saves the account id; garbage is refused", async () => {
  await withSettings();
  const input = await waitFor(() => document.querySelector<HTMLInputElement>("[data-profile-id]")!);
  fireEvent.change(input, { target: { value: "not an id" } });
  fireEvent.click(screen.getByText("Save"));
  expect(await screen.findByText(/No valid account ID found/)).toBeTruthy();
  expect(await getProfileAccount()).toBeNull();
  fireEvent.change(input, { target: { value: "https://www.warframe.com/profile?playerId=0123456789abcdef01234567" } });
  fireEvent.click(screen.getByText("Save"));
  await waitFor(async () => expect(await getProfileAccount()).toBe("0123456789abcdef01234567"));
});

test("continuing without an inventory clears the source and opens the world view", async () => {
  const onComplete = setup();
  fireEvent.click(await screen.findAllByText("Continue without inventory").then((l) => l[0]));
  await finish();
  await waitFor(() => expect(onComplete).toHaveBeenCalledWith("world"));
});

test("market sign-in comes right after the inventory, webview first, and a session shows as signed in", async () => {
  const signInWebview = vi.fn().mockResolvedValue(undefined);
  setup({ market: { session: null, signIn: vi.fn(), signInWebview } });
  fireEvent.click(await screen.findAllByText("Continue without inventory").then((l) => l[0]));
  fireEvent.click(await screen.findByText(/Sign in with warframe.market|Sign in on warframe.market/i, { selector: "button" }));
  await waitFor(() => expect(signInWebview).toHaveBeenCalled());
});

test("the wizard finishes by recording setup as done", async () => {
  setup();
  fireEvent.click(await screen.findAllByText("Continue without inventory").then((l) => l[0]));
  await finish();
  await waitFor(async () => expect((await getOnboarding()).setupDone).toBe(true));
});
