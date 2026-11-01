import type { Translator } from "../../lib/i18n";
import { answerConfirm, usePendingConfirm } from "../../stores/confirm";
import { Button, Modal } from "../ui";

/** Renders the dialog behind askConfirm(). Mount once (App). The first line of the message is the title, the rest the body. */
export default function ConfirmHost({ tr }: { tr: Translator }) {
  const p = usePendingConfirm();
  if (!p) return null;
  const [title, ...rest] = p.message.split("\n");
  return (
    <Modal
      title={title}
      tr={tr}
      width="26rem"
      onClose={() => answerConfirm(false)}
      footer={
        <>
          <Button onClick={() => answerConfirm(false)}>{tr("common.cancel")}</Button>
          <Button variant="primary" onClick={() => answerConfirm(true)}>
            {tr("common.confirm")}
          </Button>
        </>
      }
    >
      {rest.length > 0 && <p className="m-0 whitespace-pre-line text-sm text-text-secondary">{rest.join("\n")}</p>}
    </Modal>
  );
}
