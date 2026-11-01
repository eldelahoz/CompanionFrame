import { normalizeRelicOverlayFilterPush, type RelicOverlayFilterPush } from "../../../config/shared/relicPlannerView";

// The overlay windows share the app's origin, so localStorage carries the push without a backend command.
const KEY = "relic.overlayFilters";

export function pushRelicOverlayFilters(push: RelicOverlayFilterPush): boolean {
  try {
    localStorage.setItem(KEY, JSON.stringify(push));
    return true;
  } catch {
    return false;
  }
}

export function loadRelicOverlayFilters(): RelicOverlayFilterPush | null {
  try {
    const raw = localStorage.getItem(KEY);
    return raw ? normalizeRelicOverlayFilterPush(JSON.parse(raw)) : null;
  } catch {
    return null;
  }
}
