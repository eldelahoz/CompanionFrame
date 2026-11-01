import { useRef, useState } from "react";
import type { Translator } from "../../lib/i18n";
import { wfmSignInWebviewCancel } from "../../lib/ipc";

interface Props {
  onSignIn: (email: string, password: string) => Promise<void>;
  /** Opens the official site in its own window; no password is typed in this app. */
  onSignInWebview?: () => Promise<void>;
  tr: Translator;
}

const INPUT = "w-full rounded-md border border-border bg-surface-input px-2 py-1.5 text-sm text-text-primary outline-none focus:border-accent/70";

/** The password lives only in the uncontrolled field: it is read at submit and wiped before the request goes out. */
export default function SignInForm({ onSignIn, onSignInWebview, tr }: Props) {
  const form = useRef<HTMLFormElement>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [web, setWeb] = useState(false);
  const [before, after = ""] = tr("market.signInSteamHint").split("{link}");

  const [showForm, setShowForm] = useState(!onSignInWebview);

  const webview = async () => {
    setError("");
    setBusy(true);
    setWeb(true);
    try {
      await onSignInWebview!();
    } catch (err) {
      setError((err as Error).message || tr("market.signInFailed"));
    } finally {
      setBusy(false);
      setWeb(false);
    }
  };

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    const fields = form.current!.elements;
    const email = (fields.namedItem("email") as HTMLInputElement).value.trim();
    const pw = fields.namedItem("password") as HTMLInputElement;
    const password = pw.value;
    pw.value = "";
    setError("");
    setBusy(true);
    try {
      await onSignIn(email, password);
    } catch (err) {
      setError((err as Error).message || tr("market.signInFailed"));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex flex-col items-center gap-3 py-3">
      <div className="w-[min(560px,100%)] rounded-xl border border-border bg-bg-surface p-4" data-market-signin>
        <h2 className="m-0 font-display text-2xl font-bold">{tr("market.wfmTitle")}</h2>
        <p className="mb-3.5 mt-1.5 text-sm text-text-secondary">
          <strong>{tr("market.signInHint")}</strong>
          <br />
          {before}
          <a className="link-btn" href="https://warframe.market/profile/settings#password" target="_blank" rel="noreferrer noopener">
            {tr("market.wfmAccountSettings")}
          </a>
          {after}
        </p>
        {onSignInWebview && (
          <button type="button" className="btn-primary mb-2 w-full" disabled={busy} onClick={() => void webview()} data-market-signin-web>
            {busy ? tr("market.signingIn") : tr("market.signInWeb")}
          </button>
        )}
        {web && (
          <button type="button" className="link-btn mb-2" onClick={() => void wfmSignInWebviewCancel()} data-market-signin-cancel>
            {tr("common.cancel")}
          </button>
        )}
        {error && !showForm && (
          <div className="text-danger" role="alert">
            {error}
          </div>
        )}
        {!showForm && (
          <button type="button" className="link-btn" onClick={() => setShowForm(true)}>
            {tr("market.signInUsePassword")}
          </button>
        )}
        {showForm && <form ref={form} autoComplete="on" onSubmit={submit}>
          <div className="mb-2 grid gap-1">
            <label htmlFor="market-email" className="text-sm font-medium text-text-secondary">
              {tr("market.emailLabel")}
            </label>
            <input id="market-email" name="email" type="email" className={INPUT} placeholder="you@example.com" autoComplete="email" required />
          </div>
          <div className="mb-2 grid gap-1">
            <label htmlFor="market-password" className="text-sm font-medium text-text-secondary">
              {tr("market.passwordLabel")}
            </label>
            <input id="market-password" name="password" type="password" className={INPUT} placeholder="........" autoComplete="current-password" required />
          </div>
          {error && (
            <div className="text-danger" role="alert">
              {error}
            </div>
          )}
          <button type="submit" className="btn-primary mt-1 w-full" disabled={busy}>
            {busy ? tr("market.signingIn") : tr("market.signIn")}
          </button>
        </form>}
      </div>
    </div>
  );
}
