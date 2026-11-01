import { useEffect, useState } from "react";
import type { Translator } from "../../lib/i18n";
import { gameLanguageDetected, onItemDbUpdated, type Settings } from "../../lib/ipc";
import { GAME_LANGUAGE_CODES, gameLanguageName } from "../../lib/gameLanguage";
import { Select, SettingRow } from "../ui";

interface Props {
  tr: Translator;
  settings: Settings;
  onChange: (patch: Partial<Settings>) => Promise<string | null> | void;
  id: string;
}

/** Warframe's own language (item names, reward and riven OCR), apart from the app's display language. */
export default function GameLanguageRow({ tr, settings, onChange, id }: Props) {
  const [detected, setDetected] = useState<string | null>(null);

  useEffect(() => {
    const load = () => void gameLanguageDetected().then(setDetected).catch(() => {});
    load();
    const off = onItemDbUpdated(load);
    return () => void off.then((f) => f());
  }, []);

  const auto = detected
    ? tr("settings.gameLanguageAuto", { language: gameLanguageName(detected) })
    : tr("settings.gameLanguageAutoUnknown", { language: gameLanguageName("en") });
  const options = [["auto", auto] as const, ...GAME_LANGUAGE_CODES.map((c) => [c, gameLanguageName(c)] as const)];
  return (
    <SettingRow
      label={tr("settings.gameLanguageRow")}
      description={tr("settings.gameLanguageHint")}
      htmlFor={id}
      control={<Select id={id} value={settings.game_language} options={options} onChange={(e) => void onChange({ game_language: e.target.value })} />}
    />
  );
}
