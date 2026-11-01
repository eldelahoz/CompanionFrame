// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import de from "../../i18n/de.json";
import fr from "../../i18n/fr.json";
import zh from "../../i18n/zh.json";
import { translator } from "../../lib/i18n";
import { OVERLAY_DEFAULTS } from "../../overlays/common/overlayDefaults";
import { OVERLAY_KINDS } from "../../overlays/common/overlayTheme";
import OverlayGuide, { GUIDES } from "./OverlayGuide";

afterEach(cleanup);

test("every overlay has a guide inside the screen, captioned in every language", () => {
  expect(Object.keys(GUIDES).sort()).toEqual([...OVERLAY_KINDS].sort());
  for (const k of OVERLAY_KINDS) {
    const d = OVERLAY_DEFAULTS[k];
    expect(d.x >= 0 && d.y >= 0 && d.x + d.w <= 1 && d.y + d.h <= 1).toBe(true);
    const [x, y, w, h] = GUIDES[k].scan ?? [0, 0, 160, 90];
    expect(x >= 0 && y >= 0 && x + w <= 160 && y + h <= 90).toBe(true);
    for (const dict of [de, fr, zh] as Record<string, string>[]) for (const part of ["caption", "alt"]) expect(dict[`overlayGuide.${k}.${part}`]).toBeTruthy();
    render(<OverlayGuide kind={k} tr={translator("en")} />);
  }
  expect(screen.getAllByRole("img")).toHaveLength(OVERLAY_KINDS.length);
});

test("the reward overlay starts above the card row it scans", () => {
  const [, cardTop] = GUIDES.reward.scan!;
  expect((OVERLAY_DEFAULTS.reward.y + OVERLAY_DEFAULTS.reward.h) * 90).toBeLessThanOrEqual(cardTop);
});
