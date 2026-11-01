import React from "react";
import ReactDOM from "react-dom/client";
import "./overlays/overlay.css";
import { OverlayShell } from "./overlays/common/OverlayShell";
import { tauriOverlayIpc } from "./overlays/common/overlayIpc";
import { parseOverlayKind } from "./overlays/common/overlayTheme";
import { installExternalLinks } from "./lib/ipc";
import { SCREENS } from "./overlays/screens";

// WebView2 swallows external links; listing rows open in the default browser.
installExternalLinks();

// One entry, four windows: the overlay manager opens overlay.html?kind=<reward|riven|arbi|trade>.
const kind = parseOverlayKind(window.location.search);
if (kind) {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <OverlayShell kind={kind} ipc={tauriOverlayIpc}>
        {SCREENS[kind]}
      </OverlayShell>
    </React.StrictMode>,
  );
}
