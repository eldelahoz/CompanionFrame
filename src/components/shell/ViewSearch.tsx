import { Search } from "lucide-react";
import { useEffect, useId, useMemo, useRef, useState } from "react";
import type { Translator } from "../../lib/i18n";
import { VIEWS, type ViewName } from "../../lib/viewRegistry";
import { NavIcon } from "../icons";
import { memoryMode } from "../../stores/dataSource";

/** Titlebar jump box: Ctrl+K focuses it, arrows pick a view, Enter opens it. */
export default function ViewSearch({ tr, onSelect }: { tr: Translator; onSelect: (view: ViewName) => void }) {
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState(false);
  const [index, setIndex] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const listId = useId();

  const memory = memoryMode.use();
  const matches = useMemo(() => {
    const q = query.trim().toLowerCase();
    return VIEWS.filter(([name, key]) => (memory || name !== "missions") && (!q || tr(key).toLowerCase().includes(q)));
  }, [query, tr, memory]);

  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        input.current?.focus();
        input.current?.select();
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, []);

  const go = (name: ViewName) => {
    onSelect(name);
    setQuery("");
    setOpen(false);
    input.current?.blur();
  };

  return (
    <div className="view-search app-region-no-drag">
      <Search aria-hidden="true" />
      <input
        ref={input}
        type="text"
        role="combobox"
        aria-label={tr("titlebar.searchLabel")}
        aria-expanded={open}
        aria-controls={listId}
        aria-activedescendant={open && matches[index] ? `${listId}-${matches[index][0]}` : undefined}
        placeholder={tr("titlebar.search")}
        value={query}
        onFocus={() => setOpen(true)}
        onBlur={() => setOpen(false)}
        onChange={(e) => {
          setQuery(e.target.value);
          setIndex(0);
          setOpen(true);
        }}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown" || e.key === "ArrowUp") {
            e.preventDefault();
            const step = e.key === "ArrowDown" ? 1 : -1;
            setIndex((i) => (matches.length ? (i + step + matches.length) % matches.length : 0));
          } else if (e.key === "Enter" && matches[index]) {
            go(matches[index][0]);
          } else if (e.key === "Escape") {
            setQuery("");
            input.current?.blur();
          }
        }}
      />
      <kbd>Ctrl K</kbd>
      {open && (
        <ul id={listId} role="listbox" className="view-search-list">
          {matches.length === 0 ? (
            <li className="view-search-empty" aria-disabled="true">
              {tr("titlebar.searchEmpty")}
            </li>
          ) : (
            matches.map(([name, key], i) => (
              <li
                key={name}
                id={`${listId}-${name}`}
                role="option"
                aria-selected={i === index}
                onMouseEnter={() => setIndex(i)}
                // pointerdown, not click: the input's blur would close the list first
                onPointerDown={(e) => {
                  e.preventDefault();
                  go(name);
                }}
              >
                <NavIcon view={name} />
                {tr(key)}
              </li>
            ))
          )}
        </ul>
      )}
    </div>
  );
}
