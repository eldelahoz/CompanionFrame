import { useEffect, useRef, useState } from "react";
import { addToast } from "../../stores/toasts";
import { normalizeNotificationVolume } from "../../../config/shared/notificationSound";
import type { Translator } from "../../lib/i18n";
import { notifSoundGet, notifSoundPreview, notifSoundReset, notifSoundSet, notifSoundSetPrefs, type SoundAsset } from "../../lib/ipc";
import { prepareNotificationSound } from "../../lib/notificationSoundFile";

interface Props {
  enabled: boolean;
  volume: number | null;
  tr: Translator;
  /** Called with the saved preferences so the parent's channel state stays current. */
  onPrefs: (enabled: boolean, volume: number) => void;
}

const btn = "rounded border border-border bg-bg-raised px-2 py-1 text-text-primary enabled:cursor-pointer disabled:opacity-50";

/** Custom clip picker, volume and preview. The clip is re-encoded to bounded WAV before upload. */
export default function NotificationSoundSettings({ enabled, volume, tr, onPrefs }: Props) {
  const [sound, setSound] = useState<SoundAsset | null>(null);
  const [loading, setLoading] = useState(true);
  const [pending, setPending] = useState(false);
  const [slider, setSlider] = useState<number | null>(null);
  const file = useRef<HTMLInputElement>(null);
  const percent = slider ?? Math.round(normalizeNotificationVolume(volume) * 100);
  const disabled = !enabled || loading || pending;

  useEffect(() => {
    let alive = true;
    notifSoundGet()
      .then((s) => alive && setSound(s))
      .catch(() => alive && addToast({ level: "error", message: tr("settings.notificationSoundLoadFailed") }))
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
    };
    // tr only labels the error
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const run = async (fn: () => Promise<void>, failure: string) => {
    setPending(true);
    try {
      await fn();
    } catch {
      addToast({ level: "error", message: failure });
    } finally {
      setPending(false);
    }
  };

  const choose = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const picked = e.target.files?.[0];
    e.target.value = "";
    if (!picked || disabled) return;
    setPending(true);
    let upload;
    try {
      upload = await prepareNotificationSound(picked);
    } catch {
      addToast({ level: "error", message: tr("settings.notificationSoundInvalid") });
      setPending(false);
      return;
    }
    await run(async () => setSound(await notifSoundSet(upload)), tr("settings.notificationSoundSaveFailed"));
  };

  const reset = () =>
    run(async () => {
      await notifSoundReset();
      setSound(null);
    }, tr("settings.notificationSoundSaveFailed"));

  // Persist on release only: every input event would be an atomic write.
  const commitVolume = (value: number) => {
    setSlider(null);
    if (disabled) return;
    const next = value / 100;
    void notifSoundSetPrefs(enabled, next).then((s) => onPrefs(s.soundEnabled, normalizeNotificationVolume(s.soundVolume)));
  };

  return (
    <div className="flex flex-col gap-3" data-notification-sound-settings>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div>{tr("settings.notificationCustomSound")}</div>
          <div className="mt-1 truncate text-xs text-text-muted" title={sound?.name} data-notification-sound-name>
            {loading ? tr("common.loading") : (sound?.name ?? tr("common.default"))}
          </div>
        </div>
        <input
          ref={file}
          className="hidden"
          type="file"
          accept=".wav,.mp3,.ogg,audio/wav,audio/mpeg,audio/ogg"
          aria-label={tr("settings.notificationSoundChoose")}
          data-setting="notification-sound-file"
          disabled={disabled}
          onChange={(e) => void choose(e)}
        />
        <button type="button" className={btn} disabled={disabled} onClick={() => file.current?.click()}>
          {tr("settings.notificationSoundChoose")}
        </button>
        <button type="button" className={btn} data-setting="notification-sound-reset" disabled={loading || pending || !sound} onClick={() => void reset()}>
          {tr("common.reset")}
        </button>
      </div>
      <p className="m-0 text-xs text-text-muted">{tr("settings.notificationSoundFileHint")}</p>
      <div className="flex flex-wrap items-center gap-3">
        <label htmlFor="notification-sound-volume">{tr("settings.notificationSoundVolume")}</label>
        <input
          id="notification-sound-volume"
          type="range"
          min={0}
          max={100}
          step={1}
          value={percent}
          disabled={disabled}
          className="min-w-24 flex-1 accent-accent disabled:opacity-50"
          onChange={(e) => setSlider(Number(e.target.value))}
          onPointerUp={(e) => commitVolume(Number(e.currentTarget.value))}
          onKeyUp={(e) => commitVolume(Number(e.currentTarget.value))}
        />
        <output htmlFor="notification-sound-volume" className="w-10 text-right text-xs text-text-muted">
          {percent}%
        </output>
        <button
          type="button"
          className={btn}
          data-setting="notification-sound-preview"
          disabled={disabled}
          onClick={() => void run(notifSoundPreview, tr("settings.notificationSoundPreviewFailed"))}
        >
          {tr("settings.notificationSoundPreview")}
        </button>
      </div>
    </div>
  );
}
