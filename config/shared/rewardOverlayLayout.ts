import { getOverlayDescriptor, normalizeOverlayLayout } from "./overlayLayout";
import type { OverlayFieldStyle, REWARD_OVERLAY_FIELDS } from "./overlayLayout";

export const REWARD_OVERLAY_CANVAS = getOverlayDescriptor("reward").canvas;
export interface RewardOverlayLayout {
  version: 1;
  fields: Partial<Record<(typeof REWARD_OVERLAY_FIELDS)[number], OverlayFieldStyle>>;
}

export function normalizeRewardOverlayLayout(value: unknown): RewardOverlayLayout {
  return normalizeOverlayLayout("reward", value);
}
