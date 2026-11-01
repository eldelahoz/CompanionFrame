import en from "../i18n/en.json";
import type { Language } from "./ipc";

export type MessageKey = keyof typeof en;
export type Translator = (key: MessageKey, params?: Record<string, string | number>) => string;

export const LOCALE_OPTIONS: ReadonlyArray<{ code: Language; label: string }> = [
  { code: "en", label: "English" },
  { code: "de", label: "Deutsch" },
  { code: "zh", label: "简体中文" },
  { code: "fr", label: "Français" },
  { code: "es", label: "Español" },
];

// All five ship in the main bundle (~100 kB each); WFHelper lazy-loaded de/zh. Upgrade to dynamic import when size matters.
import de from "../i18n/de.json";
import zh from "../i18n/zh.json";
import fr from "../i18n/fr.json";
import es from "../i18n/es.json";
const DICTS: Record<Language, Partial<Record<MessageKey, string>>> = { en, de, zh, fr, es };

/** {name} placeholders with an English fallback. */
export function translator(lang: Language): Translator {
  return (key, params = {}) =>
    (DICTS[lang][key] ?? en[key] ?? key).replace(/\{(\w+)\}/g, (_m, k: string) =>
      params[k] == null ? `{${k}}` : String(params[k]),
    );
}
