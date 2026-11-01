import type { Translator } from "../../lib/i18n";
import { LOCALE_OPTIONS } from "../../lib/i18n";
import type { Language, Settings } from "../../lib/ipc";
import AppScaleSection from "../settings/AppScaleSection";
import GameLanguageRow from "../settings/GameLanguageRow";
import PresetSelector from "../settings/PresetSelector";
import { Select, SettingRow } from "../ui";

interface Props {
  tr: Translator;
  settings: Settings;
  onChange: (patch: Partial<Settings>) => Promise<string | null> | void;
}

/** First-run look and feel: language, theme preset and app size. Everything stays editable under Settings. */
export default function ConfigureStep({ tr, settings, onChange }: Props) {
  return (
    <div className="flex flex-col gap-5" data-setup-configure>
      <SettingRow
        label={tr("settings.languageRow")}
        htmlFor="setup-language"
        control={<Select id="setup-language" value={settings.language} options={LOCALE_OPTIONS.map((o) => [o.code, o.label] as const)} onChange={(e) => void onChange({ language: e.target.value as Language })} />}
      />
      <GameLanguageRow id="setup-game-language" tr={tr} settings={settings} onChange={onChange} />
      <div>
        <PresetSelector tr={tr} />
        <p className="m-0 text-xs text-text-muted">{tr("setup.theme.hint")}</p>
      </div>
      <div>
        <AppScaleSection tr={tr} scale={settings.ui_scale} onCommit={(ui_scale) => void onChange({ ui_scale })} />
        <p className="m-0 text-xs text-text-muted">{tr("setup.appSize.hint")}</p>
      </div>
      <p className="m-0 text-xs text-text-muted">{tr("setup.theme.footer")}</p>
    </div>
  );
}
