import { useEffect, useRef } from "react";
import { applyCustomCss, safeMode, useCustomCss } from "../lib/customCss/customCssStore";
import { restartInSafeMode } from "../lib/customCss/safeMode";

/** Applies the sanitized user stylesheet; renders nothing. */
export default function CustomCssHost() {
  const state = useCustomCss();
  const sheet = useRef<CSSStyleSheet | null>(null);

  useEffect(() => {
    if (safeMode || typeof document.adoptedStyleSheets?.[Symbol.iterator] !== "function") return;
    // A constructed sheet sits outside any style-src rule that blocks an inline <style>, and still cascades after the token layer.
    const created = new CSSStyleSheet();
    document.adoptedStyleSheets = [...document.adoptedStyleSheets, created];
    sheet.current = created;
    return () => {
      document.adoptedStyleSheets = document.adoptedStyleSheets.filter((s) => s !== created);
      sheet.current = null;
    };
  }, []);

  useEffect(() => {
    try {
      sheet.current?.replaceSync(applyCustomCss(state).css);
    } catch {
      sheet.current?.replaceSync("");
    }
  }, [state]);

  // A sheet that hides the UI never throws, so no crash panel can offer recovery; this chord is the way back.
  useEffect(() => {
    const onKeydown = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.altKey && e.shiftKey && e.key.toLowerCase() === "r") {
        e.preventDefault();
        restartInSafeMode();
      }
    };
    window.addEventListener("keydown", onKeydown);
    return () => window.removeEventListener("keydown", onKeydown);
  }, []);

  return null;
}
