import { useEffect, useMemo, useState } from "react";
import { translator, type Translator } from "../../lib/i18n";
import type { Language } from "../../lib/ipc";
import type { OverlayIpc } from "./overlayIpc";

/** A settings pull that never answers must not keep the overlay blank (4 s rule). */
export const OVERLAY_I18N_TIMEOUT_MS = 4000;

/**
 * Resolves the persisted language; `ready` flips even on failure/timeout so the first paint is never blocked.
 * shortcut: language is read once; a language change needs the overlay window reloaded, subscribe when settings emit an event.
 */
export function useOverlayI18n(ipc: OverlayIpc): { tr: Translator; lang: Language; ready: boolean } {
  const [lang, setLang] = useState<Language>("en");
  const [ready, setReady] = useState(false);
  useEffect(() => {
    let live = true;
    const done = () => live && setReady(true);
    const timer = setTimeout(done, OVERLAY_I18N_TIMEOUT_MS);
    ipc
      .settings()
      .then((s) => live && setLang(s.language))
      .catch(() => {})
      .then(() => {
        clearTimeout(timer);
        done();
      });
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [ipc]);
  const tr = useMemo(() => translator(lang), [lang]);
  return { tr, lang, ready };
}
