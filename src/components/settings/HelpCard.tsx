import { useState } from "react";
import { COMMUNITY_URL, DOCS_URL, ISSUES_URL } from "../../config/links";
import type { Translator } from "../../lib/i18n";
import { setSetupDone } from "../../lib/ipc";
import { openView } from "../../stores/nav";
import { startTour } from "../../stores/onboarding";
import OverlayPlacement from "../setup/OverlayPlacement";
import { Button, Modal, SettingRow } from "../ui";

const link = (href: string, label: string) => (
  <a className="ui-btn ui-btn--sm" href={href} target="_blank" rel="noreferrer noopener">
    {label}
  </a>
);

/** Settings > Help: restart the tour, replace the overlays, community and docs, and the issue link (no feedback backend). */
export default function HelpCard({ tr }: { tr: Translator }) {
  const [placing, setPlacing] = useState(false);
  return (
    <div data-setting="help">
      <SettingRow label={tr("help.tour")} description={tr("help.tourDesc")} control={<Button size="sm" onClick={startTour}>{tr("help.tourRestart")}</Button>} />
      <SettingRow label={tr("help.setup")} description={tr("help.setupDesc")} control={<Button size="sm" onClick={() => void setSetupDone(false).then(() => openView("setup"))}>{tr("help.setupRerun")}</Button>} />
      <SettingRow label={tr("help.placement")} description={tr("help.placementDesc")} control={<Button size="sm" onClick={() => setPlacing(true)}>{tr("help.placementOpen")}</Button>} />
      <SettingRow label={tr("help.docs")} description={tr("help.docsDesc")} control={link(DOCS_URL, tr("help.docsOpen"))} />
      <SettingRow label={tr("help.community")} description={tr("help.communityDesc")} control={link(COMMUNITY_URL, tr("help.communityOpen"))} />
      <SettingRow label={tr("help.issue")} description={tr("help.issueDesc")} control={link(ISSUES_URL, tr("help.issueOpen"))} />
      {placing && (
        <Modal title={tr("setup.overlay.title")} tr={tr} onClose={() => setPlacing(false)} width="52rem">
          <OverlayPlacement tr={tr} />
        </Modal>
      )}
    </div>
  );
}
