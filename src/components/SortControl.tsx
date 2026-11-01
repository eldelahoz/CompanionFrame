import type { Translator } from "../lib/i18n";

interface Props {
  value: string;
  options: ReadonlyArray<readonly [string, string]>;
  direction: "asc" | "desc";
  onSelect: (value: string) => void;
  onToggleDirection: () => void;
  tr: Translator;
}

export default function SortControl({ value, options, direction, onSelect, onToggleDirection, tr }: Props) {
  const asc = direction === "asc";
  return (
    <div className="shared-sort-controls">
      <span className="shared-sort-label">{tr("common.sort")}</span>
      <div className="sort-control">
        <select className="shared-filter-select sort-control-select" value={value} onChange={(e) => onSelect(e.target.value)}>
          {options.map(([key, text]) => (
            <option key={key} value={key}>
              {text}
            </option>
          ))}
        </select>
        <button
          type="button"
          className="sort-control-direction"
          onClick={onToggleDirection}
          title={asc ? tr("common.sortLowToHigh") : tr("common.sortHighToLow")}
          aria-label={asc ? tr("common.sortDirectionAscending") : tr("common.sortDirectionDescending")}
        >
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
            {asc ? (
              <>
                <path d="M2 4h3M2 8h5.5M2 12h8" />
                <path d="M12.5 13V3" />
                <path d="M10.4 5.1L12.5 3l2.1 2.1" />
              </>
            ) : (
              <>
                <path d="M2 4h8M2 8h5.5M2 12h3" />
                <path d="M12.5 3v10" />
                <path d="M10.4 10.9L12.5 13l2.1-2.1" />
              </>
            )}
          </svg>
        </button>
      </div>
    </div>
  );
}
