import { LICENSE_URL } from "../../config/links";
import type { Translator } from "../../lib/i18n";

/** The app-wide risk and no-warranty text, with "MIT License" linking to the LICENSE. Callers wrap it. */
export default function AppDisclaimer({ tr }: { tr: Translator }) {
  const [before, after = ""] = tr("app.license").split("{license}");
  return (
    <span>
      {tr("app.disclaimer")} {before}
      <a href={LICENSE_URL} target="_blank" rel="noreferrer noopener" className="text-accent underline underline-offset-2">
        {tr("app.licenseName")}
      </a>
      {after}
    </span>
  );
}
