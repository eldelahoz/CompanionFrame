import { createSignal } from "../lib/signal";
import type { ViewName } from "../lib/viewRegistry";

/** Views ask for navigation here; App owns the current view and consumes the request. */
export const requestedView = createSignal<ViewName | "setup" | null>(null);
export const openView = (view: ViewName | "setup"): void => requestedView.set(view);

/** Dashboard section to scroll to and flash; DashboardView consumes it. */
export const requestedDashboardSection = createSignal<string | null>(null);
export const openDashboardSection = (id: string): void => {
  requestedDashboardSection.set(id);
  openView("dashboard");
};

/** Settings card to scroll to; SettingsView consumes it. */
export const requestedSettingsCard = createSignal<string | null>(null);
export const openSettingsCard = (card: string): void => {
  requestedSettingsCard.set(card);
  openView("settings");
};
