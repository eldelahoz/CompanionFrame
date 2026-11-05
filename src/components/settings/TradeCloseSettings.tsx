import { useEffect, useState } from "react";
import { Input, SettingRow, Switch } from "../ui";
import type { Translator } from "../../lib/i18n";
import type { Settings } from "../../lib/ipc";

interface Props {
  settings: Settings;
  onChange: (patch: Partial<Settings>) => Promise<string | null> | void;
  tr: Translator;
}

/** What happens to the matched warframe.market listings once a trade is detected. */
export default function TradeCloseSettings({ settings, onChange, tr }: Props) {
  // The message is saved on blur, not per keystroke: the backend rejects a half-edited value.
  const [message, setMessage] = useState(settings.trade_thanks_message);
  useEffect(() => setMessage(settings.trade_thanks_message), [settings.trade_thanks_message]);
  return (
    <>
      <SettingRow
        label={tr("settings.autoCloseListings")}
        description={tr("settings.autoCloseListingsInfo")}
        control={<Switch checked={settings.auto_close_trade_listings} aria-label={tr("settings.autoCloseListings")} onChange={(e) => void onChange({ auto_close_trade_listings: e.target.checked })} />}
      />
      <SettingRow
        label={tr("settings.thanksEnabled")}
        description={tr("settings.thanksInfo")}
        control={<Switch checked={settings.trade_thanks_enabled} aria-label={tr("settings.thanksEnabled")} onChange={(e) => void onChange({ trade_thanks_enabled: e.target.checked })} />}
      />
      <SettingRow
        label={tr("settings.thanksMessage")}
        htmlFor="settings-thanks-message"
        control={
          <Input
            id="settings-thanks-message"
            value={message}
            maxLength={200}
            disabled={!settings.trade_thanks_enabled}
            onChange={(e) => setMessage(e.target.value)}
            onBlur={() => message !== settings.trade_thanks_message && void onChange({ trade_thanks_message: message })}
          />
        }
      />
    </>
  );
}
