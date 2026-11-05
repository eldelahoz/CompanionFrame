// @vitest-environment jsdom
import { cleanup, render } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import type { Translator } from "../../lib/i18n";
import type { TradeEvent } from "../../types/stats";
import StatsTradePanel from "./StatsTradePanel";

afterEach(cleanup);

const tr: Translator = (key) => key;
const trade = (items: TradeEvent["items"]): TradeEvent => ({ id: "t1", date: "2026-01-02T03:04:05Z", type: "sale", platChange: 10, items });
const item = (over: Partial<TradeEvent["items"][number]>): TradeEvent["items"][number] => ({ internalName: "", displayName: "Serration", count: 1, direction: "given", ...over });

it("loads item art through ItemImage, never from warframe.market/static", () => {
  const { container } = render(
    <StatsTradePanel trades={[trade([item({ wfmSlug: "serration", wfmThumb: "items/images/en/thumbs/serration.png" })])]} locale="en" tr={tr} />,
  );
  for (const img of container.querySelectorAll("img")) expect(img.getAttribute("src")).not.toContain("warframe.market/static");
  // Outside Tauri there is no proxy source, so ItemImage shows its placeholder.
  expect(container.querySelector('[title="common.noImageAvailable"]')).not.toBeNull();
});

it("renders no image for an item without a slug", () => {
  const { container } = render(<StatsTradePanel trades={[trade([item({ wfmThumb: "items/images/en/thumbs/x.png" })])]} locale="en" tr={tr} />);
  expect(container.querySelector("img.size-4")).toBeNull(); // the platinum Readout icon is unrelated
  expect(container.querySelector('[title="common.noImageAvailable"]')).toBeNull();
});
