// @vitest-environment jsdom
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import type { RewardOverlayItem, RewardOverlayPayload, RewardOverlayView } from "../../bindings";
import { translator } from "../../lib/i18n";
import type { OverlayIpc } from "../common/overlayIpc";
import { REWARD_EVENT, RewardOverlay } from "./RewardOverlay";
import { rewardCardViews } from "./rewardCards";

afterEach(cleanup);

const tr = translator("en");

const item = (over: Partial<RewardOverlayItem> = {}): RewardOverlayItem => ({
  name: "Khora Prime Systems Blueprint", rarity: "Rare", ducats: 100, partOwnedCount: 1, partRequiredCount: 1, mastered: null,
  vaulted: true, building: false, setOwnedCount: 2, setRequiredCount: 4, setUrlName: "khora_prime_set", setParts: [], ...over,
});

const payload = (...names: string[]): RewardOverlayPayload => ({
  count: names.length,
  slots: [...names.map((name) => ({ item: item({ name }), price: 28, setPrice: null })), ...Array(4 - names.length).fill(null)],
});

const view = (over: Partial<RewardOverlayView>): RewardOverlayView => ({
  phase: "ready", presentation: null, failure: null, autoCloseMs: 8500, token: 1, ...over,
});

function mount() {
  let push: (v: RewardOverlayView) => void = () => {};
  const ipc: OverlayIpc = {
    settings: async () => ({}) as never,
    placement: async () => [],
    layout: { get: async () => null, save: async () => {}, reset: async () => {} },
    on: (e, cb) => {
      if (e === REWARD_EVENT) push = cb as typeof push;
      return () => {};
    },
  };
  const { container } = render(<RewardOverlay tr={tr} ipc={ipc} interactive={false} />);
  return { container, emit: (v: RewardOverlayView) => act(async () => push(v)) };
}

it("maps a payload to one card view per read slot, marks the best pick and leaves absent chips empty", () => {
  const cards = rewardCardViews({ ...payload("A Prime Part", "B Prime Part"), slots: [null, payload("A Prime Part").slots[0], null, payload("B Prime Part").slots[0]] }, tr, true);
  expect(cards.map((c) => c.name)).toEqual(["A Prime Part", "B Prime Part"]);
  expect(cards[0]).toMatchObject({ platinum: 28, ducats: 100, rarity: "rare", status: { kind: "owned", owned: 1 }, setOwned: { owned: 2, required: 4 }, setPrice: null, foundry: false, vaulted: true, best: true, slug: "a_prime_part" });
  expect(cards[1].best).toBe(false);
  const plain = rewardCardViews({ count: 1, slots: [{ item: item({ vaulted: null, ducats: 0, building: true, setRequiredCount: 0 }), price: null, setPrice: 96 }, null, null, null] }, tr, true);
  expect(plain[0]).toMatchObject({ platinum: null, ducats: null, setOwned: null, setPrice: 96, foundry: true, vaulted: false, best: false });
});

it("says no inventory on every card instead of guessing counts", () => {
  const cards = rewardCardViews(payload("A Prime Part"), tr, false);
  expect(cards[0]).toMatchObject({ status: { kind: "unknown" }, setOwned: null, foundry: false });
});

it("draws a card's art from the item DB's public URL", async () => {
  const url = "https://cdn.warframestat.us/img/khora-prime.png";
  const parts = [{ name: "", imageUrl: url, ownedCount: 0, requiredCount: 1, isReward: true, building: false }];
  const { container, emit } = mount();
  await emit(view({ presentation: { count: 1, slots: [{ item: item({ setParts: parts }), price: 28, setPrice: null }, null, null, null] } }));
  expect(container.querySelector(`img[src="${url}"]`)).not.toBeNull();
});

it("is blank until the first round starts", () => {
  expect(mount().container.textContent).toBe("");
});

it("shows the reading state, then 1-4 cards, then clears them when a new round starts", async () => {
  const { container, emit } = mount();
  await emit(view({ phase: "scanning", autoCloseMs: 0, token: 1 }));
  expect(container.textContent).toContain("Reading reward screen");
  await emit(view({ presentation: payload("A Prime Part", "B Prime Part", "C Prime Part"), token: 1 }));
  expect(container.querySelectorAll("[data-reward-card]").length).toBe(3);
  expect(container.textContent).toContain("A Prime Part");
  expect(container.textContent).toContain("Best pick");
  expect(container.textContent).toContain("Owned ×1");
  expect(container.textContent).not.toMatch(/·/);
  await emit(view({ phase: "scanning", autoCloseMs: 0, token: 2 }));
  expect(container.querySelectorAll("[data-reward-card]").length).toBe(0);
});

it("shows a failure and ignores an event of an older round", async () => {
  const { container, emit } = mount();
  await emit(view({ phase: "failed", failure: "nothingRead", token: 5 }));
  expect(container.textContent).toContain("OCR failed");
  await emit(view({ presentation: payload("Old Prime Part"), token: 4 }));
  expect(container.textContent).not.toContain("Old Prime Part");
});
