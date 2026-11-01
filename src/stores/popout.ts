import { createContext } from "react";

/** What this window was opened for (`?popout=view:world` / `?popout=section:world.fissures`); null in the main window. */
function readTarget(): { view: string | null; sectionId: string | null } | null {
  if (typeof window === "undefined") return null;
  const key = new URLSearchParams(window.location.search).get("popout");
  if (key?.startsWith("view:")) return { view: key.slice(5), sectionId: null };
  if (key?.startsWith("section:")) {
    // The World tab's dailies section became a dashboard widget; windows opened before that still point at it.
    const sectionId = key.slice(8) === "world.dailies" ? "dashboard.dailies" : key.slice(8);
    return { view: sectionId.split(".")[0] ?? null, sectionId };
  }
  return null;
}

const target = readTarget();
export const popoutView = target?.view ?? null;
export const popoutSectionId = target?.sectionId ?? null;
export const isPopoutWindow = target !== null;

/** Set around the hosted view in a section popout; LayoutGrid then renders that one section alone. */
export const PopoutSoloSection = createContext<string | null>(null);
