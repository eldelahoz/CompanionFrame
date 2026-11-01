import type { Translator } from "../../lib/i18n";
import { contrastRatio, WCAG_AA_NORMAL } from "../../lib/theme/contrastUtils";

export default function ContrastBadge({ fg, bg, tr }: { fg: string; bg: string; tr: Translator }) {
  const ratio = contrastRatio(fg, bg);
  if (ratio >= WCAG_AA_NORMAL) return null;
  const label = `${ratio.toFixed(1)}:1`;
  return (
    <span
      className="inline-flex items-center gap-0.5 rounded-[2px] px-1 py-px text-xs font-bold bg-danger/20 text-danger border border-danger/35"
      title={`${tr("appearance.lowContrast")} (${label})`}
    >
      <svg className="h-2.5 w-2.5" viewBox="0 0 16 16" fill="currentColor">
        <path d="M8 1a7 7 0 100 14A7 7 0 008 1zm0 10.5a1 1 0 110-2 1 1 0 010 2zM8.75 4.75v4h-1.5v-4h1.5z" />
      </svg>
      {label}
    </span>
  );
}
