import type { ReactNode } from "react";
import type { OverlayContext } from "./common/OverlayShell";
import type { OverlayKind } from "./common/overlayTheme";
import { ArbiOverlay } from "./arbi/ArbiOverlay";
import { RewardOverlay } from "./reward/RewardOverlay";
import { RivenOverlay } from "./riven/RivenOverlay";
import { TradeOverlay } from "./trade/TradeOverlay";

export const SCREENS: Record<OverlayKind, (ctx: OverlayContext) => ReactNode> = {
  reward: (c) => <RewardOverlay {...c} />,
  riven: (c) => <RivenOverlay {...c} />,
  arbi: (c) => <ArbiOverlay {...c} />,
  trade: (c) => <TradeOverlay {...c} />,
};
