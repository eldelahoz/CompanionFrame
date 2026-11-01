import { Check } from "lucide-react";
import { useRef, useState } from "react";
import type { Translator } from "../../lib/i18n";
import { wfmSignInWebviewCancel, type WfmSession } from "../../lib/ipc";
import { Badge, Button, Field, Input } from "../ui";

interface Props {
  tr: Translator;
  session: WfmSession | null;
  signIn: (email: string, password: string) => Promise<void>;
  signInWebview?: () => Promise<void>;
}

/** warframe.market sign-in for the wizard: the official site in a window first, email and password as the fallback. */
export default function MarketStep({ tr, session, signIn, signInWebview }: Props) {
  const form = useRef<HTMLFormElement>(null);
  const [showForm, setShowForm] = useState(!signInWebview);
  const [busy, setBusy] = useState(false);
  const [web, setWeb] = useState(false);
  const [error, setError] = useState("");

  if (session?.logged_in) {
    return (
      <div role="status" data-setup-market-ok>
        <Badge tone="success" icon={<Check size={14} aria-hidden />}>
          {tr("setup.market.signedIn", { name: session.user_name ?? "" })}
        </Badge>
      </div>
    );
  }

  const run = async (go: () => Promise<void>, viaWeb = false) => {
    setError("");
    setBusy(true);
    setWeb(viaWeb);
    try {
      await go();
    } catch (e) {
      setError((e as Error).message || tr("market.signInFailed"));
    } finally {
      setBusy(false);
      setWeb(false);
    }
  };
  // The password stays in the uncontrolled field: read at submit and wiped before the request goes out.
  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    const fields = form.current!.elements;
    const email = (fields.namedItem("email") as HTMLInputElement).value.trim();
    const pw = fields.namedItem("password") as HTMLInputElement;
    const password = pw.value;
    pw.value = "";
    void run(() => signIn(email, password));
  };
  const [before, after = ""] = tr("market.signInSteamHint").split("{link}");

  return (
    <div className="flex max-w-[28rem] flex-col gap-3" data-market-signin>
      {signInWebview && (
        <Button variant="primary" loading={busy} disabled={busy} onClick={() => void run(signInWebview, true)} data-market-signin-web>
          {busy ? tr("market.signingIn") : tr("market.signInWeb")}
        </Button>
      )}
      {web && (
        <button type="button" className="link-btn self-start" onClick={() => void wfmSignInWebviewCancel()} data-market-signin-cancel>
          {tr("common.cancel")}
        </button>
      )}
      {!showForm && (
        <button type="button" className="link-btn self-start" onClick={() => setShowForm(true)}>
          {tr("market.signInUsePassword")}
        </button>
      )}
      {showForm && (
        <form ref={form} className="flex flex-col gap-3" autoComplete="on" onSubmit={submit}>
          <p className="m-0 text-xs text-text-muted">
            {before}
            <a className="link-btn" href="https://warframe.market/profile/settings#password" target="_blank" rel="noreferrer noopener">
              {tr("market.wfmAccountSettings")}
            </a>
            {after}
          </p>
          <Field label={tr("market.emailLabel")}>
            <Input name="email" type="email" placeholder="you@example.com" autoComplete="email" required />
          </Field>
          <Field label={tr("market.passwordLabel")}>
            <Input name="password" type="password" autoComplete="current-password" required />
          </Field>
          <Button type="submit" disabled={busy}>
            {busy ? tr("market.signingIn") : tr("market.signIn")}
          </Button>
        </form>
      )}
      {error && (
        <p className="m-0 text-xs text-danger" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
