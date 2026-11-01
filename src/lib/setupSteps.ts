// The first-run wizard: configure (language, look, size), inventory source, overlay placement.
const SETUP_STEPS = ["configure", "inventory", "placement"] as const;
export type SetupStep = (typeof SETUP_STEPS)[number];

export const nextStep = (s: SetupStep): SetupStep | null => SETUP_STEPS[SETUP_STEPS.indexOf(s) + 1] ?? null;
export const prevStep = (s: SetupStep): SetupStep | null => SETUP_STEPS[SETUP_STEPS.indexOf(s) - 1] ?? null;

/** Starter themes offered in the configure step; the rest stay in Settings > Appearance. Keys of `THEME_PRESETS`. */
export const SETUP_THEME_KEYS: readonly string[] = ["default", "midnight", "graphite", "tennoMinimal", "light", "corpusGlass"];
