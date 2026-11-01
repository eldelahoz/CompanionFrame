import { createSignal } from "../lib/signal";

/** Settings (and the wizard's end) ask App to run the feature tour; the Tour component consumes it. */
export const tourRequested = createSignal(false);
export const startTour = (): void => tourRequested.set(true);
