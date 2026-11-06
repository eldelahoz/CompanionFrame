import { createViewState } from "./viewState";

/** The World tabs' searches and filters come back as the user left them (memory only). */
export const worldView = createViewState({ arbiSearch: "", baroSearch: "", baroUnowned: false, nightwaveSearch: "", dailiesQuery: "" });
