import { useMemo, useRef, useState, type KeyboardEvent } from "react";
import type { Translator } from "../../lib/i18n";
import { customCss, safeMode, useCustomCss } from "../../lib/customCss/customCssStore";
import { exportCustomCss, importCustomCss } from "../../lib/customCss/portable";
import { CUSTOM_CSS_MAX_BYTES, sanitizeCustomCss, verifyCustomCss, type CustomCssWarningReason } from "../../lib/customCss/sanitize";
import ThemedControlCard from "../ThemedControlCard";

const TAB_SPACES = "  ";

/** Port of CustomCssSection.svelte: the sanitized stylesheet editor with import/export and the sanitizer's warnings. */
export default function CustomCssSection({ tr }: { tr: Translator }) {
  const saved = useCustomCss();
  const [draft, setDraft] = useState(saved.css);
  const [importError, setImportError] = useState<CustomCssWarningReason | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);

  const dirty = draft !== saved.css;
  const warnings = useMemo(() => {
    const sanitized = sanitizeCustomCss(draft);
    return [...sanitized.warnings, ...verifyCustomCss(sanitized.css)];
  }, [draft]);

  const handleTab = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key !== "Tab" || e.shiftKey || e.ctrlKey || e.altKey || e.metaKey) return;
    e.preventDefault();
    const field = e.currentTarget;
    const { selectionStart: start, selectionEnd: end } = field;
    setDraft(draft.slice(0, start) + TAB_SPACES + draft.slice(end));
    requestAnimationFrame(() => {
      field.selectionStart = field.selectionEnd = start + TAB_SPACES.length;
    });
  };

  const handleFile = async (file: File | undefined) => {
    if (!file) return;
    if (file.size > CUSTOM_CSS_MAX_BYTES) return setImportError("tooLarge");
    const result = importCustomCss(await file.text());
    if (!result.ok) return setImportError(result.reason);
    // An imported file never flips the opt-in toggle; enabling stays a user action.
    setImportError(null);
    setDraft(result.css);
  };

  const handleExport = () => {
    const blob = new Blob([exportCustomCss({ css: draft, enabled: saved.enabled, updatedAt: saved.updatedAt })], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `companionframe-custom-css-${new Date().toISOString().slice(0, 10)}.json`;
    document.body.appendChild(link);
    link.click();
    link.remove();
    URL.revokeObjectURL(url);
  };

  return (
    <article className="w-full rounded-[var(--radius-xl)] border border-[var(--ui-panel-border)] bg-[var(--ui-panel-bg)] p-4 shadow-[var(--ui-panel-shadow)] [backdrop-filter:var(--ui-backdrop-blur)]">
      <h3 className="m-0 mb-1.5 font-display text-[var(--font-heading-size,0.95rem)] font-semibold tracking-[0.03em] text-text-primary">{tr("customCss.title")}</h3>
      <p className="text-[var(--font-small-size,0.82rem)] text-text-secondary">{tr("customCss.description")}</p>
      <p className="mt-1.5 border-l-2 border-accent-dim pl-[0.55rem] text-[var(--font-small-size,0.82rem)] leading-snug text-text-secondary">{tr("customCss.info")}</p>

      {safeMode && (
        <p className="my-3 rounded-[var(--radius-md)] border border-warning/40 bg-warning/10 px-3 py-2 text-xs text-text-primary" data-custom-css-safe-mode>
          {tr("customCss.safeModeBanner")}
        </p>
      )}

      <div className="mt-3" data-setting="custom-css-enabled">
        <ThemedControlCard as="label">
          <span className="text-xs font-medium text-text-secondary">
            {tr("customCss.enable")}
            <span className="mt-0.5 block text-xs font-normal text-text-muted">{tr("customCss.enableWarning")}</span>
          </span>
          <input type="checkbox" checked={saved.enabled} onChange={(e) => customCss.setEnabled(e.target.checked)} />
        </ThemedControlCard>
      </div>

      <textarea
        className="mt-2 h-56 w-full resize-y rounded-[var(--radius-md)] border border-[var(--ui-control-border)] bg-[var(--ui-control-bg)] p-2 font-mono text-xs leading-relaxed text-text-primary"
        spellCheck={false}
        data-custom-css-editor
        aria-label={tr("customCss.editorLabel")}
        placeholder={tr("customCss.placeholder")}
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={handleTab}
      />

      <div className="mt-2 flex flex-wrap gap-2">
        <button className="btn-primary btn-sm" disabled={!dirty} onClick={() => customCss.save(draft)}>
          {tr("customCss.apply")}
        </button>
        <button
          className="btn-secondary btn-sm"
          disabled={!dirty}
          onClick={() => {
            setDraft(saved.css);
            setImportError(null);
          }}
        >
          {tr("customCss.revert")}
        </button>
        <button className="btn-secondary btn-sm" onClick={() => fileInput.current?.click()}>
          {tr("customCss.import")}
        </button>
        <button className="btn-secondary btn-sm" onClick={handleExport}>
          {tr("customCss.export")}
        </button>
        <input
          ref={fileInput}
          className="hidden"
          type="file"
          accept=".css,text/css,application/json"
          onChange={(e) => {
            const file = e.target.files?.[0];
            e.target.value = "";
            void handleFile(file);
          }}
        />
      </div>

      {importError && (
        <p className="mt-2 text-xs text-danger" data-custom-css-import-error>
          {tr(`customCss.reason.${importError}`)}
        </p>
      )}

      {warnings.length > 0 && (
        <div className="mt-3" data-custom-css-warnings>
          <p className="text-xs font-semibold text-text-secondary">{tr("customCss.warningsTitle")}</p>
          <ul className="mt-1 space-y-0.5">
            {warnings.map((w, i) => (
              <li key={`${w.line}-${w.reason}-${i}`} className="text-xs text-text-muted">
                {tr("customCss.warningLine", { line: String(w.line) })} {tr(`customCss.reason.${w.reason}`)}
              </li>
            ))}
          </ul>
        </div>
      )}
    </article>
  );
}
