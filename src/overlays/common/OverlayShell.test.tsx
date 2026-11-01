// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import type { PlacementEntry } from "../../bindings";
import type { Settings } from "../../lib/ipc";
import type { Translator } from "../../lib/i18n";
import { SCREENS } from "../screens";
import { OVERLAY_INTERACTION_EVENT, OVERLAY_LAYOUT_RESET_EVENT, OVERLAY_MOVE_EVENT, OVERLAY_SETTINGS_EVENT, OVERLAY_THEME_EVENT, OverlayShell } from "./OverlayShell";
import { OVERLAY_DEFAULTS } from "./overlayDefaults";
import type { OverlayIpc } from "./overlayIpc";
import { OVERLAY_KINDS } from "./overlayTheme";
import { OVERLAY_I18N_TIMEOUT_MS } from "./useOverlayI18n";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.unstubAllGlobals();
  layouts.clear();
  placements = [];
  placementCalls = 0;
});

const settings = (over: Partial<Settings> = {}) =>
  ({ language: "en", theme: "dark", reward_scan_hotkey: "F8", eelog_path: null, auto_inventory_sync: true, overlay_layouts: {}, overlay_enabled: {}, overlay_opacity: {}, message_notifications_enabled: true, message_notifications_while_focused: false, riven_similar_auctions_shown: true, ...over }) as Settings;

const layouts = new Map<string, string>();
let placements: PlacementEntry[] = [];
let placementCalls = 0;
function fakeIpc(get: () => Promise<Settings>) {
  const handlers = new Map<string, Set<(p: unknown) => void>>();
  const ipc: OverlayIpc = {
    settings: get,
    layout: { get: async (k) => layouts.get(k) ?? null, save: async (k, j) => void layouts.set(k, j), reset: async (k) => void layouts.delete(k) },
    placement: async () => (placementCalls++, placements),
    on: (event, cb) => {
      const set = handlers.get(event) ?? handlers.set(event, new Set()).get(event)!;
      set.add(cb as (p: unknown) => void);
      return () => set.delete(cb as (p: unknown) => void);
    },
  };
  return { ipc, emit: (e: string, p: unknown) => act(async () => handlers.get(e)?.forEach((cb) => cb(p))) };
}

const probe = ({ tr }: { tr: Translator }) => <span>{tr("nav.overlay")}</span>;

it.each(OVERLAY_KINDS)("mounts the %s screen with the persisted language, theme and opacity", async (kind) => {
  const { ipc, emit } = fakeIpc(async () => settings({ language: "de", theme: "light" }));
  const { container } = render(
    <OverlayShell kind={kind} ipc={ipc}>
      {SCREENS[kind]}
    </OverlayShell>,
  );
  // The arbi screen is blank until a run reports, so wait on the shell itself.
  await waitFor(() => expect(container.querySelector(`[data-overlay=${kind}]`)).toBeTruthy());
  // lang and theme are set by effects right after the shell mounts.
  await waitFor(() => expect(document.documentElement.lang).toBe("de"));
  await waitFor(() => expect(document.documentElement.dataset.theme).toBe("light"));
  expect(container.querySelector(`[data-overlay=${kind}]`)).toBeTruthy();
  await emit(OVERLAY_THEME_EVENT, { "--overlay-opacity": "55%" });
  expect(document.documentElement.style.getPropertyValue("--overlay-opacity-current")).toBe("55%");
});

it("applies the Settings opacity of its own kind when the settings event fires, and drops its offset on a layout reset", async () => {
  let current = settings({ overlay_opacity: { arbi: 40, reward: 90 } });
  layouts.set("offset:arbi", JSON.stringify({ x: 12, y: 7 }));
  const { ipc, emit } = fakeIpc(async () => current);
  const { container } = render(<OverlayShell kind="arbi" ipc={ipc}>{SCREENS.arbi}</OverlayShell>);
  const shell = () => container.querySelector<HTMLElement>("[data-overlay=arbi]")!;
  await waitFor(() => expect(document.documentElement.style.getPropertyValue("--overlay-opacity-current")).toBe("40%"));
  // The theme and the saved offset load separately: wait for the shell, not just the opacity.
  await waitFor(() => expect(shell()?.style.transform).toContain("12px"));
  // Let the mount-time placement load settle so the settings event below is the only one in flight.
  await waitFor(() => expect(placementCalls).toBe(1));
  current = settings({ overlay_opacity: { arbi: 75 } });
  await emit(OVERLAY_SETTINGS_EVENT, null);
  await waitFor(() => expect(document.documentElement.style.getPropertyValue("--overlay-opacity-current")).toBe("75%"));
  await emit(OVERLAY_LAYOUT_RESET_EVENT, "offset:reward");
  expect(shell().style.transform).toContain("12px");
  await emit(OVERLAY_LAYOUT_RESET_EVENT, "offset:arbi");
  expect(shell().style.transform).toContain("0px");
});

it("still opens in English when the settings pull fails", async () => {
  const { ipc } = fakeIpc(() => Promise.reject(new Error("unavailable")));
  render(<OverlayShell kind="reward" ipc={ipc}>{probe}</OverlayShell>);
  expect(await screen.findByText("Overlay")).toBeTruthy();
});

it("still opens when the settings pull never answers", async () => {
  vi.useFakeTimers();
  const { ipc } = fakeIpc(() => new Promise(() => {}));
  render(<OverlayShell kind="reward" ipc={ipc}>{probe}</OverlayShell>);
  expect(screen.queryByText("Overlay")).toBeNull();
  await act(() => vi.advanceTimersByTimeAsync(OVERLAY_I18N_TIMEOUT_MS + 10));
  expect(screen.getByText("Overlay")).toBeTruthy();
});

it("dragging in interaction mode moves the content and persists the offset", async () => {
  vi.stubGlobal("requestAnimationFrame", (cb: () => void) => (cb(), 1));
  const { ipc, emit } = fakeIpc(async () => settings());
  const { container } = render(<OverlayShell kind="arbi" ipc={ipc}>{SCREENS.arbi}</OverlayShell>);
  await waitFor(() => expect(container.querySelector("[data-overlay=arbi]")).toBeTruthy());
  const shell = (c: HTMLElement) => c.querySelector<HTMLElement>("[data-overlay=arbi]")!;
  const mouse = (type: string, init: MouseEventInit) => document.dispatchEvent(new MouseEvent(type, { bubbles: true, ...init }));

  mouse("mousedown", { button: 0 });
  mouse("mousemove", { buttons: 1, screenX: 4, screenY: 4 });
  expect(shell(container).style.transform).toBe("translate(0px, 0px)"); // click-through: no drag

  await emit(OVERLAY_INTERACTION_EVENT, { interactive: true });
  mouse("mousedown", { button: 0, screenX: 0, screenY: 0 });
  await act(async () => mouse("mousemove", { buttons: 1, screenX: 30, screenY: 12 }));
  expect(shell(container).style.transform).toBe("translate(30px, 12px)");
  await waitFor(() => expect(JSON.parse(layouts.get("offset:arbi")!)).toEqual({ x: 30, y: 12 }));

  cleanup();
  const again = render(<OverlayShell kind="arbi" ipc={ipc}>{SCREENS.arbi}</OverlayShell>);
  await waitFor(() => expect(again.container.querySelector("[data-overlay=arbi]")).toBeTruthy());
  expect(shell(again.container).style.transform).toBe("translate(30px, 12px)");
});

it("sits at the default spot until Setup saves one, then at the saved spot and size", async () => {
  const { ipc, emit } = fakeIpc(async () => settings());
  const { container } = render(<OverlayShell kind="riven" ipc={ipc}>{() => null}</OverlayShell>);
  const shell = () => container.querySelector<HTMLElement>("[data-overlay=riven]")!;
  await waitFor(() => expect(shell().style.getPropertyValue("--ov-x")).toBe(String(OVERLAY_DEFAULTS.riven.x)));
  // Let the first load resolve before Setup "saves", so the test checks the update, not the race.
  await waitFor(() => expect(placementCalls).toBe(1));
  await act(async () => {});
  placements = [{ kind: "riven", placement: { x_frac: 0.1, y_frac: 0.5, scale: 1.25 } }];
  await emit(OVERLAY_SETTINGS_EVENT, null);
  await waitFor(() => expect(shell().style.getPropertyValue("--ov-x")).toBe("0.1"));
  expect(shell().style.getPropertyValue("--ov-y")).toBe("0.5");
  expect(shell().style.getPropertyValue("--ov-scale")).toBe("1.25");
});

it("asks again for the saved spot when the window gets its monitor size", async () => {
  const { ipc } = fakeIpc(async () => settings());
  const { container } = render(<OverlayShell kind="riven" ipc={ipc}>{() => null}</OverlayShell>);
  const shell = () => container.querySelector<HTMLElement>("[data-overlay=riven]")!;
  await waitFor(() => expect(placementCalls).toBe(1));
  await waitFor(() => expect(shell()).toBeTruthy());
  placements = [{ kind: "riven", placement: { x_frac: 0.2, y_frac: 0.3, scale: 1 } }];
  await act(async () => void window.dispatchEvent(new Event("resize")));
  await waitFor(() => expect(shell().style.getPropertyValue("--ov-x")).toBe("0.2"));
});

it.each(OVERLAY_KINDS)("move mode draws sample %s content and Done ends it", async (kind) => {
  const { ipc, emit } = fakeIpc(async () => settings());
  const done = vi.fn(async () => {});
  ipc.move = { regions: vi.fn(async () => {}), interactive: vi.fn(async () => {}), done };
  const { container } = render(
    <OverlayShell kind={kind} ipc={ipc}>
      {SCREENS[kind]}
    </OverlayShell>,
  );
  await waitFor(() => expect(container.querySelector(`[data-overlay=${kind}]`)).toBeTruthy());
  const content = () => container.querySelector("[data-overlay-content]")!;
  expect(content().textContent).toBe("");
  await emit(OVERLAY_MOVE_EVENT, true);
  expect(content().textContent).not.toBe("");
  // jsdom lays nothing out, so the frame with the Done chip needs a measured box; ask Done directly.
  const chip = container.querySelector("[data-overlay-move] button");
  if (chip) fireEvent.click(chip);
  await emit(OVERLAY_MOVE_EVENT, false);
  expect(content().textContent).toBe("");
});

it("move mode shows the relic planner with sample rows; dragging it saves its own offset, not the reward row's", async () => {
  const { ipc, emit } = fakeIpc(async () => settings());
  ipc.move = { regions: vi.fn(async () => {}), interactive: vi.fn(async () => {}), done: vi.fn(async () => {}) };
  const { container } = render(
    <OverlayShell kind="reward" ipc={ipc}>
      {SCREENS.reward}
    </OverlayShell>,
  );
  await waitFor(() => expect(container.querySelector("[data-overlay=reward]")).toBeTruthy());
  await emit(OVERLAY_MOVE_EVENT, true);
  expect(await screen.findByText("Relic planner")).toBeTruthy();
  expect(screen.getByText("Meso C2")).toBeTruthy();

  // jsdom lays nothing out: give the planner a box to hit.
  const panel = container.querySelector<HTMLElement>(".relic-planner")!;
  vi.spyOn(panel, "getBoundingClientRect").mockReturnValue({ left: 0, top: 0, right: 100, bottom: 100, width: 100, height: 100 } as DOMRect);
  const mouse = (type: string, init: MouseEventInit) => document.dispatchEvent(new MouseEvent(type, { bubbles: true, ...init }));
  mouse("mousedown", { button: 0, clientX: 10, clientY: 10, screenX: 0, screenY: 0 });
  await act(async () => mouse("mousemove", { buttons: 1, screenX: 20, screenY: 8 }));
  await waitFor(() => expect(JSON.parse(layouts.get("offset:planner")!)).toEqual({ x: 20, y: 8 }));
  expect(layouts.has("offset:reward")).toBe(false);
  expect(panel.parentElement!.style.getPropertyValue("--pl-x")).toBe("20px");
});

it("outside move mode, buttons and links are reported as click regions and cleared on unmount", async () => {
  const { ipc, emit } = fakeIpc(async () => settings());
  const regions = vi.fn(async () => {});
  ipc.move = { regions, interactive: vi.fn(async () => {}), done: vi.fn(async () => {}) };
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ left: 10, top: 20, right: 60, bottom: 40, width: 50, height: 20 } as DOMRect);
  const { container, unmount } = render(
    <OverlayShell kind="riven" ipc={ipc}>
      {() => (
        <div>
          <a href="https://warframe.market">market</a>
          <span>plain</span>
        </div>
      )}
    </OverlayShell>,
  );
  await waitFor(() => expect(container.querySelector("[data-overlay=riven]")).toBeTruthy());
  await waitFor(() => expect(regions).toHaveBeenCalledWith("riven", [{ x: 6, y: 16, w: 58, h: 28 }]));
  // Move mode takes over with its own whole-panel region, the link region is dropped first.
  await emit(OVERLAY_MOVE_EVENT, true);
  await waitFor(() => expect(regions).toHaveBeenCalledWith("riven", []));
  await emit(OVERLAY_MOVE_EVENT, false);
  regions.mockClear();
  unmount();
  expect(regions).toHaveBeenCalledWith("riven", []);
  vi.restoreAllMocks();
});
