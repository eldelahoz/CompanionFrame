import { useEffect, useState } from "react";
import { addToast } from "../../stores/toasts";
import { DEFAULT_SOURCE_CHANNELS, ROUTABLE_NOTIFICATION_SOURCES, WEBHOOK_CHANNELS, type NotificationSource, type WebhookChannel } from "../../../config/shared/notifications";
import type { WebhookTestError, WebhookUrlError } from "../../bindings";
import type { MessageKey, Translator } from "../../lib/i18n";
import {
  notifClearWebhook,
  notifGetState,
  notifSetDiscordPing,
  notifSetGameGate,
  notifSetSource,
  notifSetWebhook,
  notifSoundSetPrefs,
  notifTestWebhook,
  type ChannelState,
} from "../../lib/ipc";
import NotificationSoundSettings from "./NotificationSoundSettings";

const WEBHOOK_LABELS: Record<WebhookChannel, MessageKey> = { discord: "settings.webhookDiscord", generic: "settings.webhookGeneric" };
const SOURCE_LABELS: Record<(typeof ROUTABLE_NOTIFICATION_SOURCES)[number], MessageKey> = {
  worldState: "settings.channelSourceWorld",
  arbiSchedule: "settings.channelSourceArbi",
  whisper: "settings.channelSourceWhisper",
  tradeToast: "settings.channelSourceTrade",
  marketAlerts: "settings.channelSourceMarketAlerts",
  inventorySelections: "settings.channelSourceInventorySelections",
};
const URL_ERRORS: Record<WebhookUrlError, MessageKey> = {
  empty: "settings.webhookErrorEmpty",
  "invalid-url": "settings.webhookErrorInvalid",
  "not-https": "settings.webhookErrorNotHttps",
  "blocked-host": "settings.webhookErrorBlockedHost",
  "dns-failed": "settings.webhookErrorDns",
};
const TEST_ERRORS: Record<WebhookTestError, MessageKey> = {
  "not-configured": "settings.webhookErrorEmpty",
  "blocked-url": "settings.webhookErrorBlockedHost",
  failed: "settings.webhookTestFailed",
};

const field = "rounded border border-border bg-bg-raised px-2 py-1 text-text-primary focus:border-border-strong focus:outline-none";
const btn = "rounded border border-border bg-bg-raised px-2 py-1 text-text-primary enabled:cursor-pointer disabled:opacity-50";

/** Webhooks (shown masked, never read back), per-source routing, the game gate and the sound. */
export default function NotificationChannelsSettings({ tr }: { tr: Translator }) {
  const [state, setState] = useState<ChannelState | null>(null);
  const [drafts, setDrafts] = useState<Record<WebhookChannel, string>>({ discord: "", generic: "" });
  const [ping, setPing] = useState("");
  const [busy, setBusy] = useState<WebhookChannel | null>(null);
  const flash = (message: string, error: boolean) => void addToast({ level: error ? "error" : "success", message });
  const apply = (next: ChannelState) => {
    setState(next);
    setPing(next.discordPingUserId);
  };

  useEffect(() => {
    void notifGetState()
      .then(apply)
      .catch(() => flash(tr("settings.loadFailed"), true));
    // initial load only
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const guarded = async (fn: () => Promise<void>) => {
    try {
      await fn();
    } catch {
      flash(tr("settings.saveFailed"), true);
      void notifGetState().then(apply);
    }
  };

  const withBusy = async (channel: WebhookChannel, fn: () => Promise<void>) => {
    setBusy(channel);
    await guarded(fn);
    setBusy(null);
  };

  const save = (channel: WebhookChannel) =>
    withBusy(channel, async () => {
      const r = await notifSetWebhook(channel, drafts[channel]);
      if (r.ok) {
        apply(r.state);
        setDrafts({ ...drafts, [channel]: "" });
        flash(tr("settings.webhookSaved"), false);
      } else flash(tr(URL_ERRORS[r.error]), true);
    });
  const test = (channel: WebhookChannel) =>
    withBusy(channel, async () => {
      const r = await notifTestWebhook(channel);
      if (r.ok) flash(tr("settings.webhookTestSent"), false);
      else flash(tr(TEST_ERRORS[r.error]), true);
    });
  const clear = (channel: WebhookChannel) =>
    withBusy(channel, async () => {
      apply(await notifClearWebhook(channel));
      flash(tr("settings.webhookCleared"), false);
    });
  const savePing = () =>
    guarded(async () => {
      const r = await notifSetDiscordPing(ping);
      if (r.ok) {
        apply(r.state);
        flash(tr("settings.saved"), false);
      } else flash(tr("settings.discordPingInvalid"), true);
    });
  const toggleSource = (source: NotificationSource, key: "native" | "webhook", value: boolean) =>
    guarded(async () => {
      apply(await notifSetSource(source, { ...(state?.sources[source] ?? DEFAULT_SOURCE_CHANNELS[source]), [key]: value }));
      flash(tr("settings.saved"), false);
    });

  if (!state) return null;
  return (
    <section className="flex flex-col gap-2" data-notification-channels>
      <h3 className="m-0 text-sm font-bold">{tr("settings.notificationChannelsTitle")}</h3>
      <p className="m-0 text-sm text-text-secondary">{tr("settings.notificationChannelsDesc")}</p>
      <p className="m-0 text-xs text-text-muted">{tr("settings.notificationChannelsInfo")}</p>
      {WEBHOOK_CHANNELS.map((channel) => {
        const hook = state.webhooks[channel];
        return (
          <div key={channel} className="flex flex-col gap-1" data-setting={`webhook-${channel}`}>
            <span>{tr(WEBHOOK_LABELS[channel])}</span>
            {hook?.configured && (
              <span className="text-xs text-text-muted" data-webhook-masked>
                {hook.masked}
              </span>
            )}
            <div className="flex flex-wrap gap-2">
              <input
                type="url"
                className={`${field} min-w-0 flex-1`}
                aria-label={tr(WEBHOOK_LABELS[channel])}
                placeholder={tr("settings.webhookUrlPlaceholder")}
                disabled={busy === channel}
                value={drafts[channel]}
                onChange={(e) => setDrafts({ ...drafts, [channel]: e.target.value })}
              />
              <button type="button" className={btn} disabled={busy === channel || !drafts[channel]} onClick={() => void save(channel)}>
                {tr("common.save")}
              </button>
              <button type="button" className={btn} disabled={busy === channel || !hook?.configured} onClick={() => void test(channel)}>
                {tr("settings.webhookTest")}
              </button>
              <button type="button" className={btn} disabled={busy === channel || !hook?.configured} onClick={() => void clear(channel)}>
                {tr("settings.webhookClear")}
              </button>
            </div>
            {channel === "discord" && (
              <label className="flex flex-col gap-1">
                <span>{tr("settings.discordPingLabel")}</span>
                <span className="text-xs text-text-muted">{tr("settings.discordPingHint")}</span>
                <input
                  type="text"
                  inputMode="numeric"
                  autoComplete="off"
                  spellCheck={false}
                  className={field}
                  data-discord-ping-input
                  value={ping}
                  onChange={(e) => setPing(e.target.value)}
                  onBlur={() => ping !== state.discordPingUserId && void savePing()}
                />
              </label>
            )}
          </div>
        );
      })}

      <p className="m-0 mt-2 text-xs text-text-muted">{tr("settings.channelRoutingDesc")}</p>
      {ROUTABLE_NOTIFICATION_SOURCES.map((source) => {
        const toggles = state.sources[source] ?? DEFAULT_SOURCE_CHANNELS[source];
        return (
          <div key={source} className="flex items-center justify-between gap-4" data-setting={`notify-source-${source}`}>
            <span>{tr(SOURCE_LABELS[source])}</span>
            <span className="flex items-center gap-3">
              {(["native", "webhook"] as const).map((key) => (
                <label key={key} className="flex cursor-pointer items-center gap-1.5">
                  <input type="checkbox" checked={toggles[key]} onChange={(e) => void toggleSource(source, key, e.target.checked)} />
                  {tr(key === "native" ? "settings.channelNative" : "settings.channelWebhook")}
                </label>
              ))}
            </span>
          </div>
        );
      })}

      <label className="flex cursor-pointer flex-col gap-1">
        <span className="flex items-center gap-1.5">
          <input type="checkbox" checked={state.nativeOnlyWhileGameRunning} onChange={(e) => void guarded(async () => apply(await notifSetGameGate(e.target.checked)))} />
          {tr("settings.notifyOnlyWhileGameRunning")}
        </span>
        <span className="text-xs text-text-muted">{tr("settings.notifyOnlyWhileGameRunningHint")}</span>
      </label>

      <label className="flex cursor-pointer items-center gap-1.5">
        <input
          type="checkbox"
          checked={state.soundEnabled}
          onChange={(e) => void guarded(async () => apply(await notifSoundSetPrefs(e.target.checked, state.soundVolume ?? 1)))}
        />
        {tr("settings.windowsNotifSound")}
      </label>
      <NotificationSoundSettings
        enabled={state.soundEnabled}
        volume={state.soundVolume}
        tr={tr}
        onPrefs={(soundEnabled, soundVolume) => setState({ ...state, soundEnabled, soundVolume })}
      />
    </section>
  );
}
