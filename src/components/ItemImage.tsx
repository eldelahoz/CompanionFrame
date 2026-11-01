import { convertFileSrc } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import type { Translator } from "../lib/i18n";
import { inTauri } from "../lib/ipc";

interface Props {
  src?: string | null;
  /** warframe.market slug: art served by the backend (wfmimg://, Cloudflare-gated), tried last. */
  slug?: string | null;
  /** Tried after `src` fails (an icon path the host does not have). */
  fallback?: string | null;
  alt?: string;
  cls?: string;
  tr: Translator;
}


/** Image with a dashed placeholder when every source is missing or fails to load. */
export default function ItemImage({ src, slug, fallback, alt = "", cls = "item-img", tr }: Props) {
  const sources = [src, fallback, slug && inTauri() ? convertFileSrc(slug, "wfmimg") : null].filter((s): s is string => !!s);
  const key = sources.join("|");
  const [failed, setFailed] = useState(0);
  useEffect(() => setFailed(0), [key]);

  if (failed < sources.length) {
    return (
      <img
        className={`h-auto w-auto object-contain [image-rendering:auto] ${cls}`}
        src={sources[failed]}
        alt={alt}
        loading="lazy"
        onError={() => setFailed((n) => n + 1)}
      />
    );
  }
  return (
    <div
      className={`flex h-12 w-12 items-center justify-center text-text-muted opacity-30 ${cls}`}
      title={tr("common.noImageAvailable")}
    >
      <svg className="h-full w-full" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1">
        <rect x="4" y="4" width="16" height="16" rx="3" strokeDasharray="3 2.5" />
        <path d="M9.6 9.9a2.4 2.4 0 1 1 3.3 2.2c-.6.25-.9.6-.9 1.2v.45" />
        <circle cx="12" cy="16.4" r="0.4" fill="currentColor" />
      </svg>
    </div>
  );
}
