import type { MessageKey } from "../i18n";
import type { ViewName } from "../viewRegistry";

/** Views whose sections can be arranged; settings keeps a fixed shape (setup is not a registry view). */
export type LayoutView = Exclude<ViewName, "settings">;

/** Column span on the two-column layout grid; "full" always takes the row. */
export type SectionSpan = 1 | 2 | "full";

export type LayoutBreakpoint = "narrow" | "wide";

/** A section a view registers once at module load. Ids are `<view>.<name>`. */
export interface SectionDescriptor {
  id: string;
  view: LayoutView;
  labelKey: MessageKey;
  defaultSpan: SectionSpan;
  minSpan?: SectionSpan;
  canCollapse?: boolean;
  /** Load-bearing sections (the inventory grid, the mastery list) opt out of hiding. */
  canHide?: boolean;
  /** Added after release: joins a layout the user already saved as hidden, not visible. */
  hiddenInSavedLayouts?: boolean;
  /** Eligible for "Open in window". */
  canPopout?: boolean;
}

export type LayoutColumn = 0 | 1;

export interface SectionState {
  id: string;
  span: SectionSpan;
  hidden: boolean;
  collapsed: boolean;
  column?: LayoutColumn;
}

export interface ViewLayout {
  version: 1;
  sections: SectionState[];
}

/** Persisted under LAYOUT_STORAGE_KEY; unknown ids drop, missing ids append. */
export interface LayoutStateV1 {
  version: 1;
  views: Partial<Record<LayoutView, Partial<Record<LayoutBreakpoint, ViewLayout>>>>;
}

export const LAYOUT_STORAGE_KEY = "wf_layout_v1";
// Grid width (not window width) under which sections stack in one column; two ~350px columns fit above it.
export const LAYOUT_NARROW_MAX_PX = 720;
