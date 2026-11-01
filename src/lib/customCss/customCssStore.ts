import { useSyncExternalStore } from "react";
import { readStorage, writeStorage } from "../persistence";
import { isSafeMode } from "./safeMode";
import { sanitizeCustomCss, verifyCustomCss, type SanitizedCustomCss } from "./sanitize";

export const CUSTOM_CSS_STORAGE_KEY = "wf_custom_css_v1";

export interface CustomCssState {
  enabled: boolean;
  css: string;
  updatedAt: number;
}

const EMPTY: CustomCssState = { enabled: false, css: "", updatedAt: 0 };

function normalize(raw: unknown): CustomCssState {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return EMPTY;
  const r = raw as Record<string, unknown>;
  return {
    enabled: r.enabled === true,
    css: typeof r.css === "string" ? r.css : "",
    updatedAt: typeof r.updatedAt === "number" && Number.isFinite(r.updatedAt) ? r.updatedAt : 0,
  };
}

function load(): CustomCssState {
  const raw = readStorage(CUSTOM_CSS_STORAGE_KEY);
  if (raw == null || raw.trim() === "") return EMPTY;
  try {
    return normalize(JSON.parse(raw));
  } catch {
    return EMPTY;
  }
}

let state = load();
const listeners = new Set<() => void>();

function commit(next: CustomCssState): void {
  writeStorage(CUSTOM_CSS_STORAGE_KEY, JSON.stringify(next));
  state = next;
  listeners.forEach((l) => l());
}

export const customCss = {
  get: () => state,
  subscribe: (l: () => void) => (listeners.add(l), () => void listeners.delete(l)),
  /** Saves the editor draft. The stylesheet itself is only applied when enabled. */
  save: (css: string) => commit({ ...state, css: typeof css === "string" ? css : "", updatedAt: Date.now() }),
  setEnabled: (enabled: boolean) => commit({ ...state, enabled: enabled === true, updatedAt: Date.now() }),
  reset: () => commit({ ...EMPTY, updatedAt: Date.now() }),
};

export const useCustomCss = () => useSyncExternalStore(customCss.subscribe, customCss.get);

/** Constant for the lifetime of the load; the flag is decided before the first paint. */
export const safeMode = isSafeMode();

/**
 * Sanitized stylesheet for a state. Safe mode and the opt-in toggle both short-circuit
 * to an empty sheet, so nothing user-authored can render.
 */
export function applyCustomCss(s: CustomCssState = state): SanitizedCustomCss {
  if (isSafeMode() || !s.enabled) return { css: "", warnings: [] };
  const sanitized = sanitizeCustomCss(s.css);
  return { css: sanitized.css, warnings: [...sanitized.warnings, ...verifyCustomCss(sanitized.css)] };
}
