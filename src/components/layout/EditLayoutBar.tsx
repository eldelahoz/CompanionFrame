import { LayoutGrid } from "lucide-react";
import { useState } from "react";
import type { Translator } from "../../lib/i18n";
import { sectionById } from "../../lib/layout/registry";
import type { LayoutView } from "../../lib/layout/types";
import { editMode, layoutBreakpoint, resetAll, resetView, sectionsOf, setHidden, undo, useCanUndo, useLayoutState } from "../../stores/layout";
import LayoutPresetPicker from "./LayoutPresetPicker";

interface Props {
  view: LayoutView;
  tr: Translator;
  /** Restricts the restore list to the sections this screen actually renders. */
  only?: readonly string[] | null;
}

export default function EditLayoutBar({ view, tr, only = null }: Props) {
  const [showPresets, setShowPresets] = useState(false);
  const editing = editMode.use() === view;
  const breakpoint = layoutBreakpoint.use();
  const canUndo = useCanUndo();
  const sections = sectionsOf(useLayoutState(), view, breakpoint);
  const hidden = sections.filter((s) => s.hidden && (!only || only.includes(s.id)));

  const toggleEdit = () => {
    setShowPresets(false);
    editMode.set(editing ? null : view);
  };

  return (
    <div className="relative flex flex-wrap items-center justify-end gap-1" data-layout-bar={view}>
      <button type="button" className="btn-secondary btn-sm gap-1.5" data-layout-edit-toggle={view} aria-pressed={editing} onClick={toggleEdit}>
        {!editing && (
          // Idle, this bar shares a header row with the title, so only the glyph shows.
          <LayoutGrid aria-hidden="true" />
        )}
        {editing ? tr("layout.doneEditing") : tr("layout.editLayout")}
      </button>

      {editing && (
        <>
          <button type="button" className="btn-secondary btn-sm" data-layout-undo disabled={!canUndo} onClick={undo}>
            {tr("layout.undoChange")}
          </button>
          <button type="button" className="btn-secondary btn-sm" data-layout-reset-view onClick={() => resetView(view)}>
            {tr("layout.resetView")}
          </button>
          <button type="button" className="btn-secondary btn-sm" data-layout-reset-all onClick={resetAll}>
            {tr("layout.resetAllViews")}
          </button>
          <button type="button" className="btn-secondary btn-sm" data-layout-presets-toggle aria-expanded={showPresets} onClick={() => setShowPresets(!showPresets)}>
            {tr("layout.presets")}
          </button>

          {hidden.length > 0 && (
            <div className="flex basis-full flex-wrap items-center justify-end gap-1 text-xs text-text-secondary">
              <span>{tr("layout.hiddenSections")}</span>
              {hidden.map((section) => {
                const descriptor = sectionById(section.id);
                return (
                  <button
                    key={section.id}
                    type="button"
                    className="cursor-pointer rounded border border-border px-1.5 py-0.5 transition-colors hover:border-accent hover:text-accent"
                    data-layout-restore={section.id}
                    onClick={() => setHidden(view, breakpoint, section.id, false)}
                  >
                    {descriptor ? tr(descriptor.labelKey) : section.id}, {tr("layout.restore")}
                  </button>
                );
              })}
            </div>
          )}

          {showPresets && <LayoutPresetPicker view={view} tr={tr} onClose={() => setShowPresets(false)} />}
        </>
      )}
    </div>
  );
}
