// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { translator } from "../lib/i18n";
import { SYNDICATE_RANKS } from "../data/syndicateRanks";
import SyndicateEmblem from "./SyndicateEmblem";

afterEach(cleanup);

const tr = translator("en");

it("renders the public icon URL with the syndicate name as alt", () => {
  const meta = SYNDICATE_RANKS.find((m) => m.tag === "ArbitersSyndicate")!;
  const { container } = render(<SyndicateEmblem meta={meta} tr={tr} />);
  const img = container.querySelector("img")!;
  expect(img.getAttribute("src")).toBe("https://browse.wf/Lotus/Interface/Icons/Syndicates/FactionSigilJudge.png");
  expect(img.getAttribute("alt")).toBe(meta.name);
  fireEvent.error(img);
  expect(container.querySelector("img")!.getAttribute("src")).toContain("content.warframe.com/PublicExport/Lotus/Interface/Icons/Syndicates/FactionSigilJudge.png!");
});

it("uses the placeholder without an icon", () => {
  const { container } = render(<SyndicateEmblem meta={{ name: "Nobody" }} tr={tr} />);
  expect(container.querySelector("img")).toBeNull();
  expect(container.querySelector("svg")).not.toBeNull();
});

it("covers every syndicate in the table", () => {
  expect(SYNDICATE_RANKS.filter((m) => !m.icon)).toEqual([]);
});
