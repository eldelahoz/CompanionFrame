import { itemLabel } from "../lib/itemLabel";
import ComponentPanel from "../components/ComponentPanel";
import { setActiveComponent, type ActiveComponent } from "../stores/modals";
import DetailModalBase from "./DetailModalBase";
import type { ModalCtx } from "./types";

export default function ComponentDetailModal({ ctx, data }: { ctx: ModalCtx; data: ActiveComponent }) {
  const close = () => setActiveComponent(null);
  return (
    <DetailModalBase ariaLabel={itemLabel(data.comp.uniqueName ?? "", data.comp) || ctx.tr("detail.componentDetails")} tr={ctx.tr} overlayClass="comp-overlay" onClose={close} wrapPanel={false}>
      <ComponentPanel ctx={ctx} comp={data.comp} parentName={data.parentName} panelClass="comp-panel" onClose={close} />
    </DetailModalBase>
  );
}
