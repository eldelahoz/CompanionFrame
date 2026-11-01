/** Warframe client languages the app bundles name dictionaries for (scripts/copy-game-data.mjs). */
export const GAME_LANGUAGE_CODES = ["en", "es", "fr", "de", "zh"] as const;

const NAMES: Record<string, string> = {
  en: "English",
  es: "Español",
  fr: "Français",
  de: "Deutsch",
  zh: "简体中文",
  tc: "繁體中文",
};

/** Native name of a game language code ("fr" -> "Français"); unknown codes show as-is. */
export const gameLanguageName = (code: string): string => NAMES[code] ?? code;
